// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The board conversion, called without a subcommand, is unchanged by the
//! `describe` / `library` subcommands: `--help` and `--json` output are
//! compared with captures from the commit before them (see
//! `tests/fixtures/README.md`).

mod common;

use common::{fixtures, run, scratch};

#[test]
fn help_is_unchanged() {
    let out = run(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let want = std::fs::read_to_string(fixtures().join("legacy-help.txt")).expect("golden");
    assert_eq!(String::from_utf8(out.stdout).expect("utf8"), want);
}

#[test]
fn json_summary_of_a_real_board_is_unchanged() {
    let board = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kicad-parser/tests/fixtures/v7-microwave-probe.kicad_pcb");
    let dir = scratch("legacy");
    let out_dir = dir.join("o1");
    let out = run(&[
        "--json",
        board.to_str().expect("utf8"),
        "-o",
        out_dir.to_str().expect("utf8"),
    ]);
    assert_eq!(out.status.code(), Some(0));
    let got = String::from_utf8(out.stdout)
        .expect("utf8")
        .replace(out_dir.to_str().expect("utf8"), "<OUT>");
    let want = std::fs::read_to_string(fixtures().join("legacy-v7-summary.json")).expect("golden");
    assert_eq!(got, want);
    assert!(out_dir.join("v7-microwave-probe.snxpcb").is_file());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_failed_run_keeps_its_summary_and_exit_code() {
    let out = run(&["--json", "/nonexistent.kicad_pcb"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).expect("utf8");
    assert_eq!(
        text,
        "{\"converter\":\"signex-kicad-import 0.2.0\",\"error\":\"input path not found: /nonexistent.kicad_pcb: No such file or directory (os error 2)\",\"files\":[],\"ok\":false,\"warnings\":[]}\n"
    );
}

#[test]
fn a_usage_error_still_exits_two() {
    assert_eq!(run(&[]).status.code(), Some(2));
}
