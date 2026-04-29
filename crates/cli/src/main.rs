// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "signex-kicad-import", version, about = "Convert KiCad files to Signex native formats")]
struct Cli {
    /// Path to a .kicad_pro project file (or .kicad_sch / .kicad_pcb / .kicad_sym for single-file conversion).
    project: PathBuf,

    /// Output directory. Defaults to the same directory as the input.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let input = args
        .project
        .canonicalize()
        .with_context(|| format!("input path not found: {}", args.project.display()))?;

    println!("signex-kicad-import v{}", env!("CARGO_PKG_VERSION"));
    println!("input: {}", input.display());

    // TODO(phase-4.3): translate KiCad → Signex via signex_types.
    eprintln!("error: CLI conversion is not yet implemented (Phase 4.3 in progress).");
    eprintln!("       The companion-tool setup is complete; conversion logic lands next.");
    std::process::exit(2);
}
