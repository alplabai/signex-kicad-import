// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `describe` and `library` subcommands of the library converter
//! contract. The board conversion (no subcommand) lives in `main.rs`.

use std::ffi::OsString;

use clap::{Parser, Subcommand};

use crate::contract::{Counts, Exit, Outcome, describe};
use crate::lib_inputs::{InputKind, LibraryArgs, validate};
use crate::lib_read::read_inputs;
use crate::lib_report::{LibraryReport, Source, now_utc};
use crate::lib_update::Previous;
use crate::lib_write::UpdateCx;
use crate::lib_write::build;
use crate::progress::{Emitter, HEARTBEAT};
use crate::staging::Staging;
use kicad_parser::libids::library_id;

#[derive(Parser, Debug)]
#[command(
    name = "signex-kicad-import",
    version,
    about = "KiCad library converter (library converter contract 1.0)"
)]
struct ContractCli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the converter's description as JSON.
    Describe {
        /// Accepted for the contract's `describe --json`; the output is always JSON.
        #[arg(long)]
        json: bool,
    },
    /// Convert KiCad libraries into one Signex library.
    Library(LibraryArgs),
}

/// Run a contract subcommand; returns the process exit code.
pub fn main_contract(args: Vec<OsString>) -> i32 {
    let cli = match ContractCli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(e) => {
            let help = !e.use_stderr();
            let first = e.to_string().lines().next().unwrap_or("").to_string();
            let _ = e.print();
            if help {
                return 0;
            }
            // A usage error is a failed run (exit 1), not clap's 2, with
            // the summary when the caller asked for JSON.
            let wants_json = args.iter().any(|a| a == "--json" || a == "--progress");
            if wants_json {
                println!("{}", Outcome::failed(first).summary());
            }
            return Exit::Failed.code();
        }
    };
    match cli.command {
        Command::Describe { .. } => {
            println!("{}", describe());
            0
        }
        Command::Library(args) => run_library(&args),
    }
}

fn run_library(args: &LibraryArgs) -> i32 {
    let json_mode = args.json || args.progress;
    let emitter = if args.progress {
        Emitter::on(Box::new(std::io::stdout()), HEARTBEAT)
    } else {
        Emitter::off()
    };
    // A panic is a failed run (exit 1) with a summary, never a bare abort;
    // the staging folder is dropped while unwinding, and `finish` below stops
    // the heartbeat either way.
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| library(args, &emitter)))
            .unwrap_or_else(|payload| {
                let what = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_string());
                Outcome::failed(format!("internal error: {what}"))
            });
    if let Some(error) = &outcome.error {
        eprintln!("error: {error}");
    }
    if args.progress {
        emitter.finish(&outcome.summary());
    } else if json_mode {
        println!("{}", outcome.summary());
    } else if let Some(report) = &outcome.report {
        println!("report: {}", report.display());
    }
    outcome.exit.code()
}

fn library(args: &LibraryArgs, emitter: &Emitter) -> Outcome {
    let (inputs, out) = match validate(args) {
        Ok(v) => v,
        Err(e) => return Outcome::failed(e),
    };
    let names: Vec<String> = inputs
        .iter()
        .map(|(p, _)| {
            p.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        })
        .collect();
    // Default name: the first input's stem; for a library table (whose own
    // name says nothing) the folder it is in.
    let name = args.name.clone().unwrap_or_else(|| {
        let from = inputs.first().map(|(p, kind)| match kind {
            InputKind::SymLibTable | InputKind::FpLibTable => {
                p.parent().and_then(std::path::Path::file_stem)
            }
            _ => p.file_stem(),
        });
        from.flatten().map_or_else(
            || "library".to_string(),
            |s| s.to_string_lossy().into_owned(),
        )
    });
    let library_id = match &args.library_id {
        Some(id) => uuid::Uuid::parse_str(id).unwrap_or_default(),
        None => library_id(&name, &names.iter().map(String::as_str).collect::<Vec<_>>()),
    };
    if args.update
        && let Err(e) = check_update_target(&out, library_id)
    {
        return Outcome::failed(e);
    }
    let started = now_utc();
    // What the earlier run wrote, read before this run replaces its report.
    let previous = args.update.then(|| Previous::load(&out));
    emitter.phase("read", Some(inputs.len()));
    let read = match read_inputs(&inputs, library_id, &args.search_path, emitter) {
        Ok(r) => r,
        Err(e) => return Outcome::failed(format!("{e:#}")),
    };
    let mut report = LibraryReport {
        started: started.clone(),
        sources: read
            .sources
            .iter()
            .map(|s| Source {
                path: s.path.clone(),
                sha256: s.sha256.clone(),
            })
            .collect(),
        library_id: Some(library_id.to_string()),
        source_counts: read.counts,
        written_counts: Counts::default(),
        items: read.items.clone(),
        files: Vec::new(),
        file_libraries: Vec::new(),
        ledger_rows: Vec::new(),
        ledger_symbols: Vec::new(),
        ledger_footprints: Vec::new(),
    };
    // A run that read its sources leaves its report even when it fails.
    let report_path = match report.write(&out) {
        Ok(p) => p,
        Err(e) => return Outcome::failed(format!("{e:#}")),
    };
    let failed = |error: String, report: &LibraryReport| Outcome {
        exit: Exit::Failed,
        error: Some(error),
        // Nothing of the library was written; the report stays for inspection.
        library: None,
        report: Some(report_path.clone()),
        files: Vec::new(),
        written: Counts::default(),
        items: report.items.clone(),
    };
    if read.counts.symbols + read.counts.footprints == 0 {
        return failed(
            "no symbols or footprints could be read from the inputs".to_string(),
            &report,
        );
    }
    let update_cx = previous.as_ref().map(|p| UpdateCx {
        previous: p,
        now: &started,
    });
    let built = build(&read, library_id, &name, emitter, update_cx.as_ref());
    report.items.extend(built.items);
    report.written_counts = built.written;
    report.source_counts.parts = built.candidates;
    let mut keep: Vec<String> = built.files.iter().map(|(rel, _)| rel.clone()).collect();
    keep.extend(built.protected.iter().cloned());
    // An update removes only what the earlier run wrote from libraries that
    // read cleanly this run; the rest of the folder is not touched.
    let stale = previous
        .as_ref()
        .map(|p| p.stale(&keep, &read.clean_libraries, &read.attempted_libraries))
        .unwrap_or_default();
    report.files = match &previous {
        Some(p) => p.files_after(&keep, &stale),
        None => keep.clone(),
    };
    report.file_libraries = report
        .files
        .iter()
        .filter_map(|f| {
            built
                .file_libs
                .iter()
                .find(|(rel, _)| rel == f)
                .map(|(_, l)| (f.clone(), l.clone()))
                .or_else(|| {
                    previous
                        .as_ref()
                        .and_then(|p| p.lib_of(f))
                        .map(|l| (f.clone(), l.clone()))
                })
        })
        .collect();
    // The ledger: what this run wrote, plus what an earlier run wrote that stays.
    report.ledger_rows = built.ledger_rows.clone();
    report.ledger_symbols = built.ledger_symbols.clone();
    report.ledger_footprints = built.ledger_footprints.clone();
    if let Some(p) = &previous {
        let stay: Vec<String> = report
            .files
            .iter()
            .filter(|f| !built.files.iter().any(|(r, _)| r == *f))
            .cloned()
            .collect();
        let (syms, fps) = p.retained_ids(&stay);
        report.ledger_symbols.extend(syms);
        report.ledger_footprints.extend(fps);
    }
    report.ledger_symbols.sort();
    report.ledger_symbols.dedup();
    report.ledger_footprints.sort();
    report.ledger_footprints.dedup();
    report.ledger_rows.sort();
    report.ledger_rows.dedup();
    if args.dry_run {
        // Nothing was written: a later update must not think these files exist.
        report.files.clear();
        report.file_libraries.clear();
        return finish(&report, &out, Vec::new(), &report_path);
    }
    emitter.phase("write", Some(built.files.len()));
    let staged = (|| -> anyhow::Result<Vec<std::path::PathBuf>> {
        let mut stage = Staging::new(&out)?;
        for (done, (rel, bytes)) in built.files.iter().enumerate() {
            emitter.progress(done, Some(built.files.len()), rel);
            stage.write(rel, bytes)?;
        }
        test_panic("write");
        stage.commit(&stale)
    })();
    match staged {
        Ok(files) => {
            emitter.progress(built.files.len(), Some(built.files.len()), "done");
            finish(&report, &out, files, &report_path)
        }
        Err(e) => failed(format!("{e:#}"), &report),
    }
}

/// The library was written (or, for a dry run, mapped): rewrite the report
/// with the written counts and report the exit by the items.
fn finish(
    report: &LibraryReport,
    out: &std::path::Path,
    files: Vec<std::path::PathBuf>,
    report_path: &std::path::Path,
) -> Outcome {
    if let Err(e) = report.write(out) {
        return Outcome::failed(format!("{e:#}"));
    }
    Outcome {
        exit: Exit::of_items(&report.items),
        error: None,
        library: Some(out.to_path_buf()),
        report: Some(report_path.to_path_buf()),
        files,
        written: report.written_counts,
        items: report.items.clone(),
    }
}

/// `--update` needs a library this converter wrote from the same source: its
/// report names the same library id.
fn check_update_target(out: &std::path::Path, library_id: uuid::Uuid) -> Result<(), String> {
    let text = std::fs::read_to_string(out.join("import-report.json")).map_err(|_| {
        format!(
            "--update: {} holds no import-report.json; it is not a library this converter wrote",
            out.display()
        )
    })?;
    let id = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["library_id"].as_str().map(str::to_string));
    if id.as_deref() == Some(library_id.to_string().as_str()) {
        Ok(())
    } else {
        Err(format!(
            "--update: the library in {} has id {}, this run derives {library_id}; pass --library-id to update it",
            out.display(),
            id.unwrap_or_else(|| "(none)".to_string())
        ))
    }
}

/// Debug builds only: panic at the named point when
/// `SIGNEX_KICAD_IMPORT_TEST_PANIC` names it (the tests of the failure path).
#[cfg(debug_assertions)]
pub fn test_panic(point: &str) {
    if std::env::var("SIGNEX_KICAD_IMPORT_TEST_PANIC").is_ok_and(|v| v == point) {
        panic!("test panic at {point}");
    }
}

#[cfg(not(debug_assertions))]
pub fn test_panic(_point: &str) {}
