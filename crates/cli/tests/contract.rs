// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The library converter contract: `describe`, option parsing, the summary
//! and progress shapes, the report files and exit codes.

mod common;

use common::{run, schema, scratch, stdout_lines, validate};
use serde_json::{Value, json};

const TINY: &str = "(kicad_symbol_lib (version 20231120) (generator test)
  (symbol \"R\" (property \"Reference\" \"R\") (property \"Value\" \"R\")
    (symbol \"R_1_1\" (pin passive line (at 0 3.81 270) (length 1.27)
      (name \"~\") (number \"1\")))))\n";

fn write_sym(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("Tiny.kicad_sym");
    std::fs::write(&path, TINY).expect("sym");
    path
}

#[test]
fn describe_matches_the_schema() {
    let out = run(&["describe", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    let value: Value = serde_json::from_slice(&out.stdout).expect("one JSON document");
    let errors = validate(&schema(), "#/$defs/describe", &value);
    assert!(errors.is_empty(), "{errors:#?}");
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
            "kicad_mod",
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
fn library_writes_the_files_and_summarises_them() {
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
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 1, "only the summary on stdout");
    let summary = &lines[0];
    let errors = validate(&schema(), "#/$defs/summary", summary);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(
        (summary["ok"].clone(), summary["exit"].clone()),
        (json!(true), json!("ok"))
    );
    assert_eq!(summary["counts"]["symbols"], 1);
    assert_eq!(summary["library"], out_dir.to_str().expect("utf8"));
    let files: Vec<&str> = summary["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(out_dir.join("symbols/tiny.snxsym").is_file());
    assert!(out_dir.join("Tiny.snxlib").is_file());
    // No staging folder is left beside the output.
    assert!(!dir.join(".out.tmp-import").exists());
    let report = report_of(&out_dir);
    let errors = validate(&schema(), "#/$defs/report", &report);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(report["counts"]["source"]["symbols"], 1);
    assert_eq!(report["counts"]["written"]["symbols"], 1);
    assert!(report["library_id"].as_str().is_some());
    assert!(out_dir.join("import-report.md").is_file());
    // The same input gives the same bytes.
    let again = dir.join("again");
    run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        again.to_str().expect("utf8"),
        "--json",
    ]);
    for rel in ["symbols/tiny.snxsym", "Tiny.snxlib"] {
        assert_eq!(
            std::fs::read(out_dir.join(rel)).expect("a"),
            std::fs::read(again.join(rel)).expect("b"),
            "{rel}"
        );
    }
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
    assert_eq!(out.status.code(), Some(0));
    let lines = stdout_lines(&out);
    assert_eq!(
        lines[0],
        json!({ "event": "phase", "phase": "read", "total": 1 })
    );
    for line in &lines {
        let errors = validate(&schema(), "#/$defs/event", line);
        assert!(errors.is_empty(), "{line}: {errors:#?}");
    }
    let phases: Vec<&str> = lines
        .iter()
        .filter(|l| l["event"] == "phase")
        .filter_map(|l| l["phase"].as_str())
        .collect();
    assert_eq!(phases, ["read", "convert", "write"]);
    assert!(lines.iter().any(|l| l["event"] == "progress"));
    assert_eq!(lines.last().expect("lines")["event"], "summary");
    assert_eq!(lines.iter().filter(|l| l["event"] == "summary").count(), 1);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn all_contract_options_parse_and_a_dry_run_writes_only_the_report() {
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
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = report_of(&out_dir);
    assert_eq!(report["library_id"], "6f9619ff-8b86-d011-b42d-00c04fc964ff");
    // What would be written is counted; nothing but the report is.
    assert_eq!(report["counts"]["written"]["symbols"], 1);
    let names: Vec<String> = std::fs::read_dir(&out_dir)
        .expect("dir")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert!(
        names.iter().all(|n| n.starts_with("import-report")),
        "{names:?}"
    );
    let summary = &stdout_lines(&out)[0];
    assert_eq!(summary["files"], json!([]));
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
    // Dropped items and an unreadable file make the run partial.
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = report_of(&out_dir);
    let errors = validate(&schema(), "#/$defs/report", &report);
    assert!(errors.is_empty(), "{errors:#?}");
    // 1 + 2 + 3 symbols in the three inputs, all written.
    assert_eq!(report["counts"]["source"]["symbols"], 6);
    assert_eq!(report["counts"]["written"]["symbols"], 6);
    assert_eq!(report["sources"].as_array().expect("sources").len(), 3);
    let codes: Vec<&str> = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str())
        .collect();
    for want in [
        "symbol.pin_unreadable",
        "symlib.file_unreadable",
        "symbol.written",
        "symbol.body_style_dropped",
    ] {
        assert!(codes.contains(&want), "{want} in {codes:?}");
    }
    let written = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["code"] == "symbol.written")
        .expect("item");
    assert!(written["id"].as_str().is_some());
    let summary: Value = serde_json::from_slice(&out.stdout).expect("summary");
    assert_eq!(
        (summary["ok"].clone(), summary["exit"].clone()),
        (json!(true), json!("partial"))
    );
    assert!(summary["warnings"].as_array().expect("warnings").len() >= 2);
    assert_eq!(summary["counts"]["skipped"], 0);
    assert_eq!(
        std::fs::read_dir(out_dir.join("symbols"))
            .expect("dir")
            .count(),
        3
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_unreadable_symbol_file_alone_is_a_failed_run_with_its_report() {
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
    assert_eq!(out.status.code(), Some(1));
    let summary = &stdout_lines(&out)[0];
    assert_eq!(
        summary["error"],
        "no symbols or footprints could be read from the inputs"
    );
    assert_eq!(summary["library"], Value::Null);
    let report = report_of(&out_dir);
    assert_eq!(report["items"][0]["code"], "symlib.unreadable");
    assert_eq!(report["items"][0]["severity"], "error");
    assert_eq!(report["counts"]["source"]["symbols"], 0);
    assert!(!out_dir.join("symbols").exists());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

fn footprint_fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kicad-parser/tests/fixtures/footprints")
        .join(name)
        .display()
        .to_string()
}

#[test]
fn a_pretty_folder_and_a_single_mod_file_become_one_footprint_each() {
    let dir = scratch("lib-fp");
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        &footprint_fixture("Test.pretty"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--json",
    ]);
    // The truncated file and the dropped zone make it partial; two footprints are written.
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = report_of(&out_dir);
    assert_eq!(report["counts"]["source"]["footprints"], 3);
    assert_eq!(report["counts"]["written"]["footprints"], 2);
    let files: Vec<String> = std::fs::read_dir(out_dir.join("footprints"))
        .expect("dir")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(files.len(), 2);
    for f in &files {
        let (stem, ext) = f.rsplit_once('.').expect("extension");
        assert_eq!(ext, "snxfpt");
        let text = std::fs::read_to_string(out_dir.join("footprints").join(f)).expect("file");
        assert!(
            text.contains(&format!("uuid = \"{stem}\"")),
            "{f} is named by its footprint uuid"
        );
    }
    let codes: Vec<&str> = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str())
        .collect();
    assert!(
        codes.contains(&"model.deferred") && codes.contains(&"footprint.unreadable"),
        "{codes:?}"
    );

    let single = dir.join("single");
    let out = run(&[
        "library",
        &footprint_fixture("Test.pretty/R_Test_0603.kicad_mod"),
        "--output",
        single.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_dir(single.join("footprints"))
            .expect("dir")
            .count(),
        1
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_update_needs_the_same_library_and_overwrites_in_place() {
    let dir = scratch("lib-update");
    let sym = write_sym(&dir);
    let sym = sym.to_str().expect("utf8");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    assert_eq!(
        run(&["library", sym, "--output", out, "--json"])
            .status
            .code(),
        Some(0)
    );
    // The same source updates in place; a plain re-run is refused.
    assert_eq!(
        run(&["library", sym, "--output", out, "--update", "--json"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        run(&["library", sym, "--output", out, "--json"])
            .status
            .code(),
        Some(1)
    );
    // Another library id is refused and the library stays as it was.
    let before = std::fs::read(out_dir.join("symbols/tiny.snxsym")).expect("file");
    let other = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    let s = failed_summary(&[
        "library",
        sym,
        "--output",
        out,
        "--update",
        "--library-id",
        other,
        "--json",
    ]);
    assert!(s["error"].as_str().expect("error").contains("--update"));
    assert_eq!(
        std::fs::read(out_dir.join("symbols/tiny.snxsym")).expect("file"),
        before
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[cfg(debug_assertions)]
#[test]
fn a_panic_is_a_failed_run_that_keeps_the_report_and_removes_the_library_files() {
    let dir = scratch("lib-panic");
    let sym = write_sym(&dir);
    let out_dir = dir.join("out");
    let out = common::bin()
        .env("SIGNEX_KICAD_IMPORT_TEST_PANIC", "write")
        .args([
            "library",
            sym.to_str().expect("utf8"),
            "--output",
            out_dir.to_str().expect("utf8"),
            "--progress",
        ])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(1));
    let lines = stdout_lines(&out);
    let summary = lines.last().expect("lines");
    assert_eq!(summary["event"], "summary");
    assert_eq!(summary["ok"], false);
    assert!(
        summary["error"]
            .as_str()
            .expect("error")
            .starts_with("internal error: test panic at write")
    );
    assert!(validate(&schema(), "#/$defs/summary", summary).is_empty());
    // The run read its source, so the report stays; the staged library files do not.
    assert!(out_dir.join("import-report.json").is_file());
    assert!(!out_dir.join("symbols").exists() && !out_dir.join("Tiny.snxlib").exists());
    assert!(!dir.join(".out.tmp-import").exists());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[cfg(debug_assertions)]
#[test]
fn a_panic_during_an_update_leaves_the_existing_library_untouched() {
    let dir = scratch("lib-panic-update");
    let sym = write_sym(&dir);
    let sym = sym.to_str().expect("utf8");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    assert_eq!(
        run(&["library", sym, "--output", out, "--json"])
            .status
            .code(),
        Some(0)
    );
    let before = std::fs::read(out_dir.join("symbols/tiny.snxsym")).expect("file");
    let manifest = std::fs::read(out_dir.join("Tiny.snxlib")).expect("file");
    // A changed source would change the bytes; the panic must keep the old ones.
    let changed = TINY.replace("(length 1.27)", "(length 2.54)");
    std::fs::write(dir.join("Tiny.kicad_sym"), changed).expect("sym");
    let result = common::bin()
        .env("SIGNEX_KICAD_IMPORT_TEST_PANIC", "write")
        .args(["library", sym, "--output", out, "--update", "--json"])
        .output()
        .expect("runs");
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        std::fs::read(out_dir.join("symbols/tiny.snxsym")).expect("file"),
        before
    );
    assert_eq!(
        std::fs::read(out_dir.join("Tiny.snxlib")).expect("file"),
        manifest
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn repeated_library_names_and_footprints_are_errors_before_anything_is_written() {
    let dir = scratch("lib-dups");
    // Two symbol libraries named alike, in different folders.
    let (a, b) = (dir.join("a"), dir.join("b"));
    std::fs::create_dir_all(&a).expect("d");
    std::fs::create_dir_all(&b).expect("d");
    std::fs::write(a.join("Parts.kicad_sym"), TINY).expect("sym");
    std::fs::write(b.join("Parts.kicad_sym"), TINY).expect("sym");
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        a.join("Parts.kicad_sym").to_str().expect("utf8"),
        b.join("Parts.kicad_sym").to_str().expect("utf8"),
        // The same footprint twice: the folder and a file inside it.
        &footprint_fixture("Test.pretty"),
        &footprint_fixture("Test.pretty/R_Test_0603.kicad_mod"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let report = report_of(&out_dir);
    let codes: Vec<&str> = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str())
        .collect();
    assert!(
        codes.contains(&"symlib.duplicate_name") && codes.contains(&"footprint.duplicate_name"),
        "{codes:?}"
    );
    assert_eq!(report["counts"]["written"]["symbols"], 1);
    assert_eq!(
        std::fs::read_dir(out_dir.join("footprints"))
            .expect("dir")
            .count(),
        2
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_update_deletes_items_that_left_the_source() {
    let dir = scratch("lib-update-delete");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    let (fp, one) = (
        footprint_fixture("Test.pretty"),
        footprint_fixture("Test.pretty/R_Test_0603.kicad_mod"),
    );
    // The id derives from the library name; --library-id and --name pin it across both runs.
    let id = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    assert_eq!(
        run(&[
            "library",
            &fp,
            "--output",
            out,
            "--library-id",
            id,
            "--name",
            "T",
            "--json"
        ])
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        std::fs::read_dir(out_dir.join("footprints"))
            .expect("dir")
            .count(),
        2
    );
    assert_eq!(
        run(&[
            "library",
            &one,
            "--output",
            out,
            "--update",
            "--library-id",
            id,
            "--name",
            "T",
            "--json"
        ])
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        std::fs::read_dir(out_dir.join("footprints"))
            .expect("dir")
            .count(),
        1
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
