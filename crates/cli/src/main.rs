// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! `signex-kicad-import` -- one-way converter from a KiCad board to the
//! current Signex formats: `<stem>.snxpcb` plus a `<stem>.snxprj` that
//! carries the project's net classes.
//!
//! Input is a `.kicad_pcb`, or a `.kicad_pro` whose board has the same
//! stem. Schematic conversion is not part of this release.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use kicad_parser::check::{EndCheck, track_ends};
use kicad_parser::{ProjectRules, Report, convert_pcb, parse_project_rules};
use serde_json::{Value, json};

#[derive(Parser, Debug)]
#[command(
    name = "signex-kicad-import",
    version,
    about = "Convert a KiCad board (.kicad_pcb or .kicad_pro) to Signex native files"
)]
struct Cli {
    /// A .kicad_pcb, or a .kicad_pro beside a .kicad_pcb of the same name.
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

struct Outcome {
    files: Vec<PathBuf>,
    report: Report,
    ends: EndCheck,
}

fn main() {
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
            json!({
                "ok": true,
                "error": null,
                "converter": converter_name(),
                "files": files,
                "warnings": warnings,
                "track_ends": {
                    "total": outcome.ends.ends,
                    "on_own_net_pad": outcome.ends.on_own_net_pad,
                    "on_other_net_pad": outcome.ends.on_other_net_pad,
                },
            })
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
    println!(
        "track ends: {} total, {} on a pad of their net, {} on another net's pad",
        outcome.ends.ends, outcome.ends.on_own_net_pad, outcome.ends.on_other_net_pad
    );
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
    let board_path = match ext.as_str() {
        "kicad_pcb" => input.clone(),
        "kicad_pro" => input.with_extension("kicad_pcb"),
        "kicad_sch" | "kicad_sym" => bail!(
            "schematic and symbol conversion is not part of this release; \
             convert the board (.kicad_pcb or .kicad_pro)"
        ),
        other => bail!("unsupported input .{other}: expected .kicad_pcb or .kicad_pro"),
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
    let stem = board_path
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("no usable file name: {}", board_path.display()))?
        .to_string();

    let text = std::fs::read_to_string(&board_path)
        .with_context(|| format!("reading {}", board_path.display()))?;
    let rules = read_rules(&board_path.with_extension("kicad_pro"))?;
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
    write_atomically(&pcb_path, &snx_writer::write_board(&converted.board))?;
    write_atomically(
        &prj_path,
        &snx_writer::write_project(&stem, &pcb_name, &converted.classes),
    )?;
    Ok(Outcome {
        files: vec![pcb_path, prj_path],
        ends: track_ends(&converted.board),
        report: converted.report,
    })
}

/// The project file's classes and rules; `None` when there is no project
/// file beside the board.
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

/// Write `text` to a temporary file beside `path`, flush it to disk, then
/// rename it over `path`, so a failed write never leaves a half file.
fn write_atomically(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;

    let tmp = path.with_extension("tmp-import");
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("writing {}", tmp.display()))?;
    drop(file);
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}
