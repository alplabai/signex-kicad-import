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

#[test]
fn a_symbol_library_without_a_subcommand_becomes_one_snxsym() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kicad-parser/tests/fixtures/symbols/multiunit.kicad_sym");
    let dir = scratch("legacy-sym");
    let out = run(&[
        "--json",
        fixture.to_str().expect("utf8"),
        "-o",
        dir.to_str().expect("utf8"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(summary["ok"], true);
    assert!(summary.get("track_ends").is_none());
    let file = dir.join("multiunit.snxsym");
    assert_eq!(summary["files"][0], file.to_str().expect("utf8"));
    let text = std::fs::read_to_string(&file).expect("snxsym");
    let value: toml::Value = toml::from_str(&text).expect("valid TOML");
    assert_eq!(value["format"].as_str(), Some("snxsym/v4"));
    assert_eq!(value["symbols"][0]["name"].as_str(), Some("Dual_Gate"));
    assert!(
        summary["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w["message"].as_str().is_some_and(|m| m.contains("fill")))
    );
    // The same input gives the same bytes; an existing file needs --force.
    assert_ne!(
        run(&[
            "--json",
            fixture.to_str().expect("utf8"),
            "-o",
            dir.to_str().expect("utf8")
        ])
        .status
        .code(),
        Some(0)
    );
    let again = run(&[
        "--json",
        "--force",
        fixture.to_str().expect("utf8"),
        "-o",
        dir.to_str().expect("utf8"),
    ]);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&file).expect("snxsym"), text);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn derived_symbols_stand_alone_in_a_single_file() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kicad-parser/tests/fixtures/symbols/extends.kicad_sym");
    let dir = scratch("legacy-sym-extends");
    let out = run(&[
        "--json",
        fixture.to_str().expect("utf8"),
        "-o",
        dir.to_str().expect("utf8"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: toml::Value =
        toml::from_str(&std::fs::read_to_string(dir.join("extends.snxsym")).expect("f"))
            .expect("toml");
    let names: Vec<&str> = value["symbols"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names, ["Base_Part", "Derived_Part", "Grandchild_Part"]);
    // Each carries the parent's pin.
    for s in value["symbols"].as_array().expect("symbols") {
        assert!(
            s["pins_tsv"].as_str().expect("pins").lines().count() == 2,
            "{}",
            s["name"]
        );
    }
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
