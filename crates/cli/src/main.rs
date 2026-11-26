// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! `signex-kicad-import` -- one-way converter from KiCad boards to the
//! current Signex formats: `<stem>.snxpcb` plus a `<stem>.snxprj` that
//! carries the project's net classes, for each board.
//!
//! Input is a `.kicad_pcb`, or a `.kicad_pro`, which converts every
//! `.kicad_pcb` in the project's folder. Schematic conversion is not part
//! of this release.

use std::path::{Path, PathBuf};

mod contract;
mod lib_cmd;
mod lib_inputs;
mod lib_manifest_edit;
mod lib_notice;
mod lib_read;
mod lib_report;
mod lib_update;
mod lib_write;
mod progress;
mod staging;

use anyhow::{Context, Result, bail};
use clap::Parser;
use kicad_parser::placement_check::{EndTally, tally_track_ends};
use kicad_parser::{ProjectRules, Report, convert_pcb, parse_project_rules};
use serde_json::{Value, json};

#[derive(Parser, Debug)]
#[command(
    name = "signex-kicad-import",
    version,
    about = "Convert KiCad boards (.kicad_pcb or .kicad_pro) and symbol libraries (.kicad_sym) to Signex native files"
)]
struct Cli {
    /// A .kicad_pcb, a .kicad_pro whose folder holds the boards, or a
    /// .kicad_sym symbol library (written as one .snxsym).
    input: PathBuf,

    /// Output directory. Defaults to the input file's directory.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Replace output files that already exist.
    #[arg(long)]
    force: bool,

    /// Print a JSON summary on stdout instead of human-readable progress.
    #[arg(long)]
    json: bool,
}

#[derive(Default)]
struct Outcome {
    /// A symbol file was converted: there are no track ends to tally.
    symbols: bool,
    files: Vec<PathBuf>,
    report: Report,
    ends: EndTally,
}

fn main() {
    // `describe` and `library` are the library converter contract; anything
    // else is the board conversion, unchanged.
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if matches!(
        args.get(1).and_then(|a| a.to_str()),
        Some("describe" | "library")
    ) {
        std::process::exit(lib_cmd::main_contract(args));
    }
    let cli = Cli::parse();
    let result = run(&cli);
    if cli.json {
        println!("{}", summary(&result));
    } else {
        match &result {
            Ok(outcome) => print_outcome(outcome),
            Err(e) => eprintln!("error: {e:#}"),
        }
    }
    if result.is_err() {
        std::process::exit(1);
    }
}

fn converter_name() -> String {
    format!("signex-kicad-import {}", env!("CARGO_PKG_VERSION"))
}

fn summary(result: &Result<Outcome>) -> Value {
    match result {
        Ok(outcome) => {
            let files: Vec<String> = outcome
                .files
                .iter()
                .map(|f| f.display().to_string())
                .collect();
            let warnings: Vec<Value> = outcome
                .report
                .entries()
                .map(|(message, count)| json!({ "message": message, "count": count }))
                .collect();
            let mut summary = json!({
                "ok": true,
                "error": null,
                "converter": converter_name(),
                "files": files,
                "warnings": warnings,
            });
            if !outcome.symbols {
                summary["track_ends"] = json!({
                    "total": outcome.ends.total,
                    "on_own_net_pad": outcome.ends.own_net,
                    "on_other_net_pad": outcome.ends.other_net,
                });
            }
            summary
        }
        Err(e) => json!({
            "ok": false,
            "error": format!("{e:#}"),
            "converter": converter_name(),
            "files": [],
            "warnings": [],
        }),
    }
}

fn print_outcome(outcome: &Outcome) {
    for file in &outcome.files {
        println!("wrote {}", file.display());
    }
    for (message, count) in outcome.report.entries() {
        println!("warning: {count} x {message}");
    }
    if !outcome.symbols {
        println!(
            "track ends: {} total, {} on a pad of their net, {} on another net's pad",
            outcome.ends.total, outcome.ends.own_net, outcome.ends.other_net
        );
    }
}

/// One board ready to write: both files' paths and text.
struct Pending {
    pcb: (PathBuf, String),
    project: (PathBuf, String),
}

fn run(cli: &Cli) -> Result<Outcome> {
    let input = cli
        .input
        .canonicalize()
        .with_context(|| format!("input path not found: {}", cli.input.display()))?;
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let (boards, project_file) = match ext.as_str() {
        "kicad_pcb" => (vec![input.clone()], None),
        "kicad_pro" => (boards_beside(&input)?, Some(input.clone())),
        "kicad_sym" => return convert_symbol_file(cli, &input),
        "kicad_sch" => bail!(
            "schematic conversion is not part of this release; \
             convert the board (.kicad_pcb or .kicad_pro) or a symbol library (.kicad_sym)"
        ),
        other => bail!("unsupported input .{other}: expected .kicad_pcb, .kicad_pro or .kicad_sym"),
    };
    let out_dir = match &cli.output {
        Some(dir) => {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating output directory {}", dir.display()))?;
            dir.clone()
        }
        None => input
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    // Convert and check every board before writing any, so a refused one
    // leaves no output behind (see `write_all` for a failed write).
    let mut outcome = Outcome::default();
    let mut pending = Vec::new();
    for board_path in &boards {
        let stem = board_path
            .file_stem()
            .and_then(|s| s.to_str())
            .with_context(|| format!("no usable file name: {}", board_path.display()))?
            .to_string();
        let text = std::fs::read_to_string(board_path)
            .with_context(|| format!("reading {}", board_path.display()))?;
        let own_project = board_path.with_extension("kicad_pro");
        let rules = match read_rules(&own_project)? {
            Some(rules) => Some(rules),
            None => match &project_file {
                Some(p) => read_rules(p)?,
                None => None,
            },
        };
        let converted = convert_pcb(&text, rules.as_ref())
            .with_context(|| format!("converting {}", board_path.display()))?;
        let pcb_name = format!("{stem}.snxpcb");
        let pcb_path = out_dir.join(&pcb_name);
        let prj_path = out_dir.join(format!("{stem}.snxprj"));
        for path in [&pcb_path, &prj_path] {
            if path.exists() && !cli.force {
                bail!("{} exists; pass --force to replace it", path.display());
            }
        }
        let tally = tally_track_ends(&converted.board);
        outcome.ends.total += tally.total;
        outcome.ends.own_net += tally.own_net;
        outcome.ends.other_net += tally.other_net;
        outcome.report.merge(&converted.report);
        let written = snx_writer::write_board(&converted.board);
        for _ in 0..written.non_finite {
            outcome
                .report
                .note("number that is not finite written as 0");
        }
        pending.push(Pending {
            pcb: (pcb_path, written.text),
            project: (
                prj_path,
                snx_writer::write_project(&stem, &pcb_name, &converted.classes),
            ),
        });
    }
    write_all(&pending)?;
    for board in pending {
        outcome.files.push(board.pcb.0);
        outcome.files.push(board.project.0);
    }
    Ok(outcome)
}

/// A `.kicad_sym` as `<stem>.snxsym` (no board, no project file). Derived
/// symbols (`extends`) are written as symbols of their own; what could not be
/// carried over is reported by message and count, as for a board.
fn convert_symbol_file(cli: &Cli, input: &Path) -> Result<Outcome> {
    use kicad_parser::libids::{item_id, library_id};
    use kicad_parser::symlib::{flatten_derived, read_symbol_file, to_snx::to_symbol_file};

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("no usable file name: {}", input.display()))?
        .to_string();
    let read = read_symbol_file(input).with_context(|| format!("reading {}", input.display()))?;
    let name = input.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let id = library_id(&stem, &[name]);
    let mut items = read.items.clone();
    let lib = flatten_derived(&read.lib);
    let file = to_symbol_file(
        &lib,
        &stem,
        item_id(id, "symbol-file", &stem, &stem).to_string(),
        &|symbol| item_id(id, "symbol", &stem, symbol).to_string(),
        &mut items,
    );
    let out_dir = match &cli.output {
        Some(dir) => {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating output directory {}", dir.display()))?;
            dir.clone()
        }
        None => input
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let path = out_dir.join(format!("{stem}.snxsym"));
    if path.exists() && !cli.force {
        bail!("{} exists; pass --force to replace it", path.display());
    }
    let text = snx_writer::write_symbol_file(&file);
    let written = write_temp(&path, &text).and_then(|()| {
        std::fs::rename(temp_path(&path), &path)
            .with_context(|| format!("replacing {}", path.display()))
    });
    if let Err(e) = written {
        // Leave no temporary file behind.
        let _ = std::fs::remove_file(temp_path(&path));
        return Err(e);
    }
    sync_dir(&path)?;
    let mut outcome = Outcome {
        symbols: true,
        ..Outcome::default()
    };
    for item in items
        .iter()
        .filter(|i| i.severity != kicad_parser::Severity::Info)
    {
        let count = item
            .detail
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        for _ in 0..count {
            outcome.report.note(item.message.clone());
        }
    }
    outcome.files.push(path);
    Ok(outcome)
}

/// Every `.kicad_pcb` in the project file's folder, by name.
fn boards_beside(project: &Path) -> Result<Vec<PathBuf>> {
    let dir = project
        .parent()
        .with_context(|| format!("project file has no folder: {}", project.display()))?;
    let mut boards: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("kicad_pcb"))
        })
        .collect();
    boards.sort();
    if boards.is_empty() {
        bail!("{} holds no .kicad_pcb to convert", dir.display());
    }
    Ok(boards)
}

/// The project file's classes and rules; `None` when there is no project
/// file there.
fn read_rules(path: &Path) -> Result<Option<ProjectRules>> {
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rules =
        parse_project_rules(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(Some(rules))
}

/// The temporary file beside `path`: its whole name plus `.tmp-import`, so
/// a board's two files (same stem) get two different temporary files.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp-import");
    PathBuf::from(name)
}

/// Write every board's two files in two steps.
///
/// First every file goes to its temporary file, flushed to disk; a failure
/// there (a full disk, a folder that refuses writes) removes the temporary
/// files and writes nothing under the real names. Then each board's files
/// take their places, the project file before the board so a `.snxpcb`
/// never stands without its `.snxprj`, and the folder is flushed so the
/// renames survive a power cut. A rename can still fail after earlier
/// boards took their places; the error then names the file, and the boards
/// before it stay written.
fn write_all(boards: &[Pending]) -> Result<()> {
    let files = || boards.iter().flat_map(|b| [&b.pcb, &b.project]);
    let remove_temps = || {
        for (path, _) in files() {
            // Best effort: a temporary file that was never made is fine.
            let _ = std::fs::remove_file(temp_path(path));
        }
    };
    for (path, text) in files() {
        if let Err(e) = write_temp(path, text) {
            remove_temps();
            return Err(e.context("nothing was written"));
        }
    }
    for board in boards {
        for (path, _) in [&board.project, &board.pcb] {
            if let Err(e) = std::fs::rename(temp_path(path), path) {
                remove_temps();
                return Err(e).with_context(|| format!("replacing {}", path.display()));
            }
            sync_dir(path)?;
        }
    }
    Ok(())
}

/// Flush the entries of the folder `path` is in, so a rename into it is on
/// disk. Unix only: Windows cannot open a folder as a file to flush it.
#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .with_context(|| format!("flushing {}", dir.display()))
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<()> {
    Ok(())
}

/// Write `text` to the temporary file beside `path` and flush it to disk.
fn write_temp(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;

    let tmp = temp_path(path);
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("writing {}", tmp.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boards_two_files_get_two_temporary_files() {
        let pcb = temp_path(Path::new("out/board.snxpcb"));
        let prj = temp_path(Path::new("out/board.snxprj"));
        assert_ne!(pcb, prj);
        assert_eq!(pcb, Path::new("out/board.snxpcb.tmp-import"));
    }

    #[test]
    fn a_pair_is_written_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("snx-pair-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let board = Pending {
            pcb: (dir.join("b.snxpcb"), "pcb".into()),
            project: (dir.join("b.snxprj"), "prj".into()),
        };
        write_all(std::slice::from_ref(&board)).expect("writes");
        assert_eq!(std::fs::read_to_string(&board.pcb.0).expect("pcb"), "pcb");
        assert_eq!(
            std::fs::read_to_string(&board.project.0).expect("prj"),
            "prj"
        );
        assert!(!temp_path(&board.pcb.0).exists());
        assert!(!temp_path(&board.project.0).exists());
        std::fs::remove_dir_all(&dir).expect("cleans up");
    }

    #[test]
    fn a_write_that_fails_leaves_no_board_written() {
        let dir = std::env::temp_dir().join(format!("snx-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let missing = dir.join("no-such-folder");
        let boards = [
            Pending {
                pcb: (dir.join("a.snxpcb"), "pcb".into()),
                project: (dir.join("a.snxprj"), "prj".into()),
            },
            Pending {
                pcb: (missing.join("b.snxpcb"), "pcb".into()),
                project: (missing.join("b.snxprj"), "prj".into()),
            },
        ];
        let error = write_all(&boards).expect_err("the second board cannot be written");
        assert!(
            format!("{error:#}").contains("nothing was written"),
            "{error:#}"
        );
        for board in &boards {
            for (path, _) in [&board.pcb, &board.project] {
                assert!(!path.exists(), "{} written", path.display());
                assert!(!temp_path(path).exists(), "{} left", path.display());
            }
        }
        std::fs::remove_dir_all(&dir).expect("cleans up");
    }
}
