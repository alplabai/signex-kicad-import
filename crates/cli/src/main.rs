// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! `signex-kicad-import` -- one-way converter from KiCad files to
//! Signex native formats (`.snxsch`, `.snxpcb`, `.snxprj`).
//!
//! This binary lives in the GPL-3.0 companion repo because it depends
//! on `kicad-parser` / `kicad-writer`, which were factored out of the
//! main Apache-2.0 Signex repo as part of the issue-62 licensing
//! remediation. The conversion target — the `signex_types::format`
//! types — is Apache-2.0 and consumed via a path/crates.io dependency.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;
use signex_types::format::{SnxPcb, SnxSchematic};

#[derive(Parser, Debug)]
#[command(
    name = "signex-kicad-import",
    version,
    about = "Convert KiCad files (.kicad_sch / .kicad_pcb / .kicad_pro) to Signex native formats"
)]
struct Cli {
    /// Path to a .kicad_pro / .kicad_sch / .kicad_pcb file.
    input: PathBuf,

    /// Output directory. Defaults to the input file's parent directory.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let input = args
        .input
        .canonicalize()
        .with_context(|| format!("input path not found: {}", args.input.display()))?;

    let out_dir = match args.output {
        Some(o) => {
            if !o.exists() {
                std::fs::create_dir_all(&o)
                    .with_context(|| format!("creating output directory {}", o.display()))?;
            }
            o.canonicalize()
                .with_context(|| format!("output path not found: {}", o.display()))?
        }
        None => input
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    println!("signex-kicad-import v{}", env!("CARGO_PKG_VERSION"));
    println!("input:  {}", input.display());
    println!("output: {}", out_dir.display());

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "kicad_sch" => convert_schematic(&input, &out_dir).map(|_| ()),
        "kicad_pcb" => convert_pcb(&input, &out_dir).map(|_| ()),
        "kicad_pro" => convert_project(&input, &out_dir),
        "kicad_sym" => bail!(
            "symbol-library import (.kicad_sym -> .snxlib) is not yet implemented; \
             see issue-62 plan Phase 4.4"
        ),
        other if other.is_empty() => bail!(
            "input has no extension; expected one of .kicad_sch, .kicad_pcb, .kicad_pro"
        ),
        other => bail!(
            "unsupported input extension: .{other} (expected .kicad_sch, .kicad_pcb, .kicad_pro)"
        ),
    }
}

// ---------------------------------------------------------------------------
// Schematic conversion
// ---------------------------------------------------------------------------

/// Parse a single `.kicad_sch` file and emit `<stem>.snxsch` in `out_dir`.
/// Returns the stem so callers (e.g. project conversion) can reference it.
fn convert_schematic(input: &Path, out_dir: &Path) -> Result<String> {
    let mut sheet = kicad_parser::parse_schematic_file(input)
        .with_context(|| format!("parsing schematic {}", input.display()))?;

    // Rewrite child-sheet references from `.kicad_sch` to `.snxsch` so the
    // emitted .snxsch points at sibling Signex schematics rather than the
    // original KiCad files. This is the import-side responsibility; Signex
    // itself does not rewrite extensions on load.
    for child in &mut sheet.child_sheets {
        if let Some(stem) = child.filename.strip_suffix(".kicad_sch") {
            child.filename = format!("{stem}.snxsch");
        }
    }

    let snx = SnxSchematic::new(sheet);
    let toml_text = snx
        .write_string()
        .with_context(|| format!("serialising .snxsch for {}", input.display()))?;

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("input has no usable file stem: {}", input.display()))?
        .to_string();

    let out_path = out_dir.join(format!("{stem}.snxsch"));
    std::fs::write(&out_path, toml_text)
        .with_context(|| format!("writing {}", out_path.display()))?;
    println!("wrote {}", out_path.display());
    Ok(stem)
}

// ---------------------------------------------------------------------------
// PCB conversion
// ---------------------------------------------------------------------------

/// Parse a single `.kicad_pcb` file and emit `<stem>.snxpcb` in `out_dir`.
fn convert_pcb(input: &Path, out_dir: &Path) -> Result<String> {
    let board = kicad_parser::parse_pcb_file(input)
        .with_context(|| format!("parsing PCB {}", input.display()))?;

    let snx = SnxPcb::new(board);
    let toml_text = snx
        .write_string()
        .with_context(|| format!("serialising .snxpcb for {}", input.display()))?;

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("input has no usable file stem: {}", input.display()))?
        .to_string();

    let out_path = out_dir.join(format!("{stem}.snxpcb"));
    std::fs::write(&out_path, toml_text)
        .with_context(|| format!("writing {}", out_path.display()))?;
    println!("wrote {}", out_path.display());
    Ok(stem)
}

// ---------------------------------------------------------------------------
// Project conversion
// ---------------------------------------------------------------------------

/// Convert a `.kicad_pro` project: walk its sibling directory, convert every
/// `.kicad_sch` and `.kicad_pcb` we find, and emit a minimal `<stem>.snxprj`.
///
/// The Signex `.snxprj` parser (`signex_types::project::parse_project`) does
/// not actually read project file contents — it discovers the schematic root
/// and PCB by probing for sibling `<name>.snxsch` / `<name>.snxpcb` filenames.
/// We therefore write a friendly TOML stub for human inspection but Signex
/// will rely on the sibling files we just emitted.
fn convert_project(input: &Path, out_dir: &Path) -> Result<()> {
    let project_dir = input
        .parent()
        .with_context(|| format!("project file has no parent dir: {}", input.display()))?;
    let project_stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("project has no usable file stem: {}", input.display()))?
        .to_string();

    let mut converted_sch: Vec<String> = Vec::new();
    let mut converted_pcb: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(project_dir)
        .with_context(|| format!("reading project dir {}", project_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "kicad_sch" => {
                let stem = convert_schematic(&path, out_dir)?;
                converted_sch.push(stem);
            }
            "kicad_pcb" => {
                let stem = convert_pcb(&path, out_dir)?;
                converted_pcb.push(stem);
            }
            _ => {}
        }
    }

    if converted_sch.is_empty() && converted_pcb.is_empty() {
        bail!(
            "project at {} contains no .kicad_sch or .kicad_pcb files to convert",
            project_dir.display()
        );
    }

    // Pick a schematic root: prefer one that matches the project stem, else
    // the first one we found. The on-load .snxprj parser probes for
    // <project_stem>.snxsch in the project dir, so renaming may be required
    // if the KiCad project's schematic was named differently.
    let schematic_root = converted_sch
        .iter()
        .find(|s| **s == project_stem)
        .cloned()
        .or_else(|| converted_sch.first().cloned());
    let pcb_file = converted_pcb
        .iter()
        .find(|s| **s == project_stem)
        .cloned()
        .or_else(|| converted_pcb.first().cloned());

    // Write a minimal .snxprj. Format-wise this is a simple TOML stub.
    // signex_types::project::parse_project ignores the contents and
    // discovers schematic_root / pcb_file by sibling filename probe, so the
    // body below is informational only.
    let mut body = String::new();
    body.push_str("# .snxprj — Signex project file\n");
    body.push_str("# Generated by signex-kicad-import.\n");
    body.push_str("# The Signex project parser discovers <name>.snxsch and\n");
    body.push_str("# <name>.snxpcb by directory probe; this body is informational.\n\n");
    body.push_str(&format!("name = {}\n", toml_string(&project_stem)));
    if let Some(root) = &schematic_root {
        body.push_str(&format!(
            "schematic_root = {}\n",
            toml_string(&format!("{root}.snxsch"))
        ));
    }
    if let Some(pcb) = &pcb_file {
        body.push_str(&format!(
            "pcb_file = {}\n",
            toml_string(&format!("{pcb}.snxpcb"))
        ));
    }
    body.push_str("\n[converted_from_kicad]\n");
    body.push_str(&format!(
        "tool = \"signex-kicad-import {}\"\n",
        env!("CARGO_PKG_VERSION")
    ));
    body.push_str(&format!(
        "schematics = [{}]\n",
        converted_sch
            .iter()
            .map(|s| toml_string(&format!("{s}.kicad_sch")))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    body.push_str(&format!(
        "pcbs = [{}]\n",
        converted_pcb
            .iter()
            .map(|s| toml_string(&format!("{s}.kicad_pcb")))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let snxprj_path = out_dir.join(format!("{project_stem}.snxprj"));
    std::fs::write(&snxprj_path, body)
        .with_context(|| format!("writing {}", snxprj_path.display()))?;
    println!("wrote {}", snxprj_path.display());

    // If the schematic_root rename is needed, hint at it.
    if let Some(root) = &schematic_root
        && root != &project_stem
    {
        eprintln!(
            "note: Signex .snxprj parser probes for '{project_stem}.snxsch' alongside the .snxprj. \
             The chosen schematic root is '{root}.snxsch'; rename it to '{project_stem}.snxsch' \
             if you want Signex to auto-discover it on open."
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Encode a string as a TOML basic string literal. The set of characters we
/// produce (ASCII filenames + project stems) means a simple escape pass is
/// sufficient; we do not attempt to reproduce the full TOML grammar here.
fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
