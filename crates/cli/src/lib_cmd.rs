// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `describe` and `library` subcommands of the library converter
//! contract. The board conversion (no subcommand) lives in `main.rs`.

use std::ffi::OsString;

use clap::{Parser, Subcommand};

use crate::contract::{Counts, Exit, Outcome, describe};
use crate::lib_inputs::{LibraryArgs, validate};
use crate::lib_read::read_inputs;
use crate::lib_report::{LibraryReport, now_utc};
use crate::progress::{Emitter, HEARTBEAT};

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
    let outcome = library(args, &emitter);
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
    let started = now_utc();
    emitter.phase("read", Some(inputs.len()));
    let read = match read_inputs(&inputs, emitter) {
        Ok(r) => r,
        Err(e) => return Outcome::failed(format!("{e:#}")),
    };
    let report = LibraryReport {
        started,
        sources: read.sources,
        library_id: args.library_id.clone().map(|s| s.to_ascii_lowercase()),
        source_counts: read.counts,
        written_counts: Counts::default(),
        items: read.items,
    };
    // A run that read its sources leaves its report even when it fails.
    let report_path = match report.write(&out) {
        Ok(p) => p,
        Err(e) => return Outcome::failed(format!("{e:#}")),
    };
    Outcome {
        exit: Exit::Failed,
        error: Some("library output not implemented yet".to_string()),
        library: Some(out),
        report: Some(report_path),
        files: Vec::new(),
        written: Counts::default(),
        items: report.items,
    }
}
