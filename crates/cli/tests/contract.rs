// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The library converter contract: `describe`, option parsing, the summary
//! and progress shapes, the report files and exit codes.

mod common;

use common::{run, schema, scratch, stdout_lines, validate};
use serde_json::{Value, json};

fn write_sym(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("Tiny.kicad_sym");
    std::fs::write(
        &path,
        "(kicad_symbol_lib (version 20231120) (generator test))\n",
    )
    .expect("sym");
    path
}

#[test]
fn describe_matches_the_schema_but_for_the_two_table_names() {
    let out = run(&["describe", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    let value: Value = serde_json::from_slice(&out.stdout).expect("one JSON document");
    let errors = validate(&schema(), "#/$defs/describe", &value);
    // The schema's extension pattern `^[a-z0-9_]+$` has no room for a hyphen,
    // and a library table has no extension, only the names `sym-lib-table`
    // and `fp-lib-table` (docs/LIBRARY_CONVERTER.md). Those two entries are
    // the only deviation; anything else is a bug.
    let expected = [
        "$.inputs[3].extensions[0]: \"sym-lib-table\" does not match ^[a-z0-9_]+$",
        "$.inputs[3].extensions[1]: \"fp-lib-table\" does not match ^[a-z0-9_]+$",
    ];
    assert_eq!(errors, expected, "{errors:#?}");
    assert_eq!(value["contract"], "1.0");
    assert_eq!(value["converter"], "signex-kicad-import");
    let exts: Vec<&str> = value["inputs"]
        .as_array()
        .expect("inputs")
        .iter()
        .flat_map(|i| i["extensions"].as_array().expect("exts"))
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        exts,
        [
            "kicad_sym",
            "kicad_symdir",
            "pretty",
            "sym-lib-table",
            "fp-lib-table"
        ]
    );
    assert!(
        value["inputs"]
            .as_array()
            .expect("inputs")
            .iter()
            .all(|i| i["kind"] == "library")
    );
}

#[test]
fn describe_works_without_the_json_flag() {
    let out = run(&["describe"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(serde_json::from_slice::<Value>(&out.stdout).is_ok());
}

#[test]
fn library_reads_then_exits_one_with_a_summary_and_reports() {
    let dir = scratch("lib-ok");
    let sym = write_sym(&dir);
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 1, "only the summary on stdout");
    let summary = &lines[0];
    let errors = validate(&schema(), "#/$defs/summary", summary);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(summary["ok"], false);
    assert_eq!(summary["exit"], "failed");
    assert_eq!(summary["error"], "library output not implemented yet");
    let report_path = summary["report"].as_str().expect("report path");
    assert!(report_path.ends_with("import-report.json"));
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(report_path).expect("json report"))
            .expect("JSON");
    let errors = validate(&schema(), "#/$defs/report", &report);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(report["sources"].as_array().expect("sources").len(), 1);
    assert_eq!(report["library_id"], Value::Null);
    assert!(out_dir.join("import-report.md").is_file());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn progress_events_are_ndjson_with_the_summary_last() {
    let dir = scratch("lib-progress");
    let sym = write_sym(&dir);
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--progress",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let lines = stdout_lines(&out);
    assert_eq!(
        lines[0],
        json!({ "event": "phase", "phase": "read", "total": 1 })
    );
    for line in &lines {
        let errors = validate(&schema(), "#/$defs/event", line);
        assert!(errors.is_empty(), "{line}: {errors:#?}");
    }
    assert!(lines.iter().any(|l| l["event"] == "progress"));
    assert_eq!(lines.last().expect("lines")["event"], "summary");
    assert_eq!(lines.iter().filter(|l| l["event"] == "summary").count(), 1);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn all_contract_options_parse() {
    let dir = scratch("lib-opts");
    let sym = write_sym(&dir);
    let search = dir.join("search");
    let models = dir.join("models");
    std::fs::create_dir_all(&search).expect("d");
    std::fs::create_dir_all(&models).expect("d");
    let map = dir.join("map.toml");
    std::fs::write(&map, "").expect("map");
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--name",
        "Tiny",
        "--library-id",
        "6f9619ff-8b86-d011-b42d-00c04fc964ff",
        "--db",
        "ignored.csv",
        "--search-path",
        search.to_str().expect("utf8"),
        "--search-path",
        search.to_str().expect("utf8"),
        "--map",
        map.to_str().expect("utf8"),
        "--models",
        "skip",
        "--models-dir",
        models.to_str().expect("utf8"),
        "--dry-run",
        "--json",
    ]);
    let lines = stdout_lines(&out);
    // Parsed and validated: the run got to the read phase and its report.
    assert_eq!(
        lines[0]["error"], "library output not implemented yet",
        "{lines:?}"
    );
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join("import-report.json")).expect("report"),
    )
    .expect("JSON");
    assert_eq!(report["library_id"], "6f9619ff-8b86-d011-b42d-00c04fc964ff");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

fn failed_summary(args: &[&str]) -> Value {
    let out = run(args);
    assert_eq!(out.status.code(), Some(1), "{args:?}");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 1, "{args:?}: {lines:?}");
    let errors = validate(&schema(), "#/$defs/summary", &lines[0]);
    assert!(errors.is_empty(), "{args:?}: {errors:#?}");
    assert_eq!(lines[0]["ok"], false);
    assert_eq!(lines[0]["exit"], "failed");
    lines[0].clone()
}

#[test]
fn bad_options_fail_with_exit_one_and_a_summary() {
    let dir = scratch("lib-bad");
    let sym = write_sym(&dir);
    let sym = sym.to_str().expect("utf8");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");

    // Unknown option, missing --output, bad enum value.
    failed_summary(&["library", sym, "--output", out, "--bogus", "--json"]);
    failed_summary(&["library", sym, "--json"]);
    failed_summary(&[
        "library", sym, "--output", out, "--models", "maybe", "--json",
    ]);
    // Validation: not a UUID, missing input, unsupported input, missing map.
    let s = failed_summary(&[
        "library",
        sym,
        "--output",
        out,
        "--library-id",
        "nope",
        "--json",
    ]);
    assert!(s["error"].as_str().expect("error").contains("--library-id"));
    failed_summary(&["library", "/no/such/x.kicad_sym", "--output", out, "--json"]);
    let txt = dir.join("x.txt");
    std::fs::write(&txt, "").expect("f");
    failed_summary(&[
        "library",
        txt.to_str().expect("utf8"),
        "--output",
        out,
        "--json",
    ]);
    failed_summary(&[
        "library",
        sym,
        "--output",
        out,
        "--map",
        "/no/map.toml",
        "--json",
    ]);
    // None of the failures got as far as writing the output.
    assert!(!out_dir.exists());

    // A non-empty output needs --update; --update needs an existing one.
    std::fs::create_dir_all(&out_dir).expect("d");
    std::fs::write(out_dir.join("old"), "x").expect("f");
    let s = failed_summary(&["library", sym, "--output", out, "--json"]);
    assert!(s["error"].as_str().expect("error").contains("--update"));
    assert!(!out_dir.join("import-report.json").exists());
    failed_summary(&[
        "library",
        sym,
        "--output",
        dir.join("fresh").to_str().expect("utf8"),
        "--update",
        "--json",
    ]);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_usage_error_without_json_prints_to_stderr_only() {
    let out = run(&["library", "--bogus"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
}

fn parser_fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kicad-parser/tests/fixtures/symbols")
        .join(name)
        .display()
        .to_string()
}

fn report_of(out_dir: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(out_dir.join("import-report.json")).expect("report");
    serde_json::from_str(&text).expect("JSON")
}

#[test]
fn symbol_inputs_fill_the_source_counts_and_items() {
    let dir = scratch("lib-symbols");
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        &parser_fixture("multiunit.kicad_sym"),
        &parser_fixture("unknown.kicad_sym"),
        &parser_fixture("Parts.kicad_symdir"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let report = report_of(&out_dir);
    let errors = validate(&schema(), "#/$defs/report", &report);
    assert!(errors.is_empty(), "{errors:#?}");
    // 1 + 2 + 3 symbols in the three inputs.
    assert_eq!(report["counts"]["source"]["symbols"], 6);
    assert_eq!(report["sources"].as_array().expect("sources").len(), 3);
    let codes: Vec<&str> = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str())
        .collect();
    assert!(codes.contains(&"symbol.pin_unreadable"), "{codes:?}");
    assert!(codes.contains(&"symlib.file_unreadable"), "{codes:?}");
    // The summary counts warnings from the dropped / error items.
    let summary: Value = serde_json::from_slice(&out.stdout).expect("summary");
    assert!(summary["warnings"].as_array().expect("warnings").len() >= 2);
    // `skipped` counts source items not written; an unreadable file is a
    // library item (its symbols are unknown), so none here.
    assert_eq!(summary["counts"]["skipped"], 0);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_unreadable_symbol_file_is_an_item_not_a_failed_run() {
    let dir = scratch("lib-broken");
    let sym = dir.join("Cut.kicad_sym");
    std::fs::write(&sym, "(kicad_symbol_lib (symbol \"A\"").expect("sym");
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--json",
    ]);
    // Still the unimplemented-writer failure, with the read problem reported.
    assert_eq!(
        stdout_lines(&out)[0]["error"],
        "library output not implemented yet"
    );
    let report = report_of(&out_dir);
    assert_eq!(report["items"][0]["code"], "symlib.unreadable");
    assert_eq!(report["items"][0]["severity"], "error");
    assert_eq!(report["items"][0]["kind"], "library");
    assert_eq!(report["counts"]["source"]["symbols"], 0);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
