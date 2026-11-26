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
    assert_eq!(files.len(), 3, "{files:?}");
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
    // The derived symbol Beta is a part row on Alpha, not a symbol.
    assert_eq!(report["counts"]["written"]["symbols"], 5);
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
        codes.contains(&"model.not_found") && codes.contains(&"footprint.unreadable"),
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

fn fixtures_dir(sub: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kicad-parser/tests/fixtures")
        .join(sub)
}

#[test]
fn library_tables_convert_every_library_they_list_and_bind_footprints_by_nickname() {
    let dir = scratch("lib-tables");
    let fp_dir = fixtures_dir("footprints/Test.pretty");
    // The part's footprint names the fp table's nickname `Test`.
    let sym_dir = dir.join("proj");
    std::fs::create_dir_all(&sym_dir).expect("d");
    std::fs::write(
        sym_dir.join("mine.kicad_sym"),
        TINY.replace(
            "(property \"Value\" \"R\")",
            "(property \"Value\" \"R\") (property \"Footprint\" \"Test:R_Test_0603\")",
        ),
    )
    .expect("sym");
    std::fs::write(
        sym_dir.join("sym-lib-table"),
        "(sym_lib_table (version 7)
  (lib (name \"Mine\")(type \"KiCad\")(uri \"${KIPRJMOD}/mine.kicad_sym\")(options \"\")(descr \"\"))
  (lib (name \"Shared\")(type \"KiCad\")(uri \"${SNX_TEST_SHARED}/power.kicad_sym\")(options \"\")(descr \"\"))
  (lib (name \"Gone\")(type \"KiCad\")(uri \"${SECRET_TOKEN}/gone.kicad_sym\")(options \"\")(descr \"\"))
  (lib (name \"Old\")(type \"Legacy\")(uri \"old.lib\")(options \"\")(descr \"\"))
  (lib (name \"Off\")(type \"KiCad\")(uri \"off.kicad_sym\")(options \"\")(descr \"\")(disabled))
)",
    )
    .expect("table");
    std::fs::write(
        sym_dir.join("fp-lib-table"),
        format!("(fp_lib_table (version 7) (lib (name \"Test\")(type \"KiCad\")(uri \"{}\")(options \"\")(descr \"\")))", fp_dir.display()),
    )
    .expect("table");
    let out_dir = dir.join("out");
    let out = common::bin()
        // A secret in the environment that a crafted table must not reach.
        .env("SECRET_TOKEN", "hunter2")
        .args([
            "library",
            sym_dir.join("sym-lib-table").to_str().expect("utf8"),
            sym_dir.join("fp-lib-table").to_str().expect("utf8"),
            "--search-path",
            &format!("SNX_TEST_SHARED={}", fixtures_dir("symbols").display()),
            "--output",
            out_dir.to_str().expect("utf8"),
            "--json",
        ])
        .output()
        .expect("runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = report_of(&out_dir);
    // The secret is never read: nothing shows its value, the error shows the uri as written.
    let everything = format!(
        "{}{}{}{}",
        report,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        std::fs::read_to_string(out_dir.join("import-report.md")).expect("md")
    );
    assert!(!everything.contains("hunter2"));
    assert!(everything.contains("${SECRET_TOKEN}/gone.kicad_sym"));
    let items = report["items"].as_array().expect("items");
    let by_code = |code: &str| -> Vec<&str> {
        items
            .iter()
            .filter(|i| i["code"] == code)
            .filter_map(|i| i["source"].as_str())
            .collect()
    };
    // Unresolvable and unsupported entries are error items; a disabled one is info; none stops the run.
    assert_eq!(
        by_code("library.table_entry_unresolved"),
        ["sym-lib-table:Gone"]
    );
    assert_eq!(
        by_code("library.table_type_unsupported"),
        ["sym-lib-table:Old"]
    );
    assert_eq!(
        by_code("library.table_entry_disabled"),
        ["sym-lib-table:Off"]
    );
    // Mine (1 symbol) and Shared (a power symbol library) were read, the footprint library too.
    assert_eq!(report["counts"]["written"]["footprints"], 2);
    assert!(out_dir.join("symbols/mine.snxsym").is_file());
    // The part's footprint binds the converted footprint of that nickname.
    let manifest = std::fs::read_to_string(out_dir.join("proj.snxlib")).expect("manifest");
    let value: toml::Value = toml::from_str(&manifest).expect("valid TOML");
    assert_eq!(
        value["tables"]["Mine"]["tsv"]
            .as_str()
            .map(|t| t.lines().count()),
        Some(2)
    );
    let row = value["tables"]["Mine"]["tsv"]
        .as_str()
        .expect("tsv")
        .lines()
        .nth(1)
        .expect("row")
        .to_string();
    let cells: Vec<&str> = row.split('\t').collect();
    assert_eq!(cells[1], "Mine:R");
    let footprint: Value = serde_json::from_str(cells[6]).expect("footprint ref");
    let uuid = footprint["uuid"].as_str().expect("uuid");
    assert!(out_dir.join(format!("footprints/{uuid}.snxfpt")).is_file());
    assert_eq!(value["classes"][0]["key"].as_str(), Some("resistor"));
    assert_eq!(report["counts"]["written"]["parts"], 1);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_part_whose_footprint_was_not_converted_is_skipped_and_reported() {
    let dir = scratch("lib-part-missing");
    let sym = dir.join("Tiny.kicad_sym");
    std::fs::write(
        &sym,
        TINY.replace(
            "(property \"Value\" \"R\")",
            "(property \"Value\" \"R\") (property \"Footprint\" \"Nowhere:X\")",
        ),
    )
    .expect("sym");
    let out_dir = dir.join("out");
    let out = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let report = report_of(&out_dir);
    assert_eq!(report["counts"]["written"]["parts"], 0);
    let missing = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["code"] == "part.footprint_missing")
        .expect("item");
    assert_eq!(missing["severity"], "error");
    assert_eq!(missing["kind"], "part");
    // No row, so no table.
    assert!(
        !std::fs::read_to_string(out_dir.join("Tiny.snxlib"))
            .expect("manifest")
            .contains("[tables")
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_update_leaves_user_files_and_unreadable_libraries_alone_and_bumps_what_changed() {
    let dir = scratch("lib-update-rules");
    let (a, b) = (dir.join("A.kicad_sym"), dir.join("B.kicad_sym"));
    std::fs::write(&a, TINY).expect("a");
    std::fs::write(&b, TINY).expect("b");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    let id = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    let args = |extra: &[&str]| {
        let mut v = vec![
            "library",
            a.to_str().expect("utf8"),
            b.to_str().expect("utf8"),
            "--output",
            out,
            "--library-id",
            id,
            "--name",
            "L",
            "--json",
        ];
        v.extend(extra);
        run(&v)
    };
    assert_eq!(args(&[]).status.code(), Some(0));
    let manifest = std::fs::read_to_string(out_dir.join("L.snxlib")).expect("manifest");
    let a_sym = std::fs::read(out_dir.join("symbols/a.snxsym")).expect("a");
    // A user's own file in the folder.
    std::fs::write(out_dir.join("symbols/mine.snxsym"), "mine").expect("user file");
    std::fs::write(out_dir.join("notes.txt"), "notes").expect("user file");

    // Unchanged source: every byte stays, user files too.
    assert_eq!(args(&["--update"]).status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(out_dir.join("L.snxlib")).expect("m"),
        manifest
    );
    assert_eq!(
        std::fs::read(out_dir.join("symbols/a.snxsym")).expect("a"),
        a_sym
    );
    assert!(out_dir.join("symbols/mine.snxsym").is_file() && out_dir.join("notes.txt").is_file());

    // B breaks, A changes: A's symbol and row get a patch bump, B's files and rows stay.
    std::fs::write(&b, "(kicad_symbol_lib (symbol \"R\"").expect("b");
    std::fs::write(&a, TINY.replace("(length 1.27)", "(length 2.54)")).expect("a");
    let r = args(&["--update"]);
    assert_eq!(
        r.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(
        out_dir.join("symbols/b.snxsym").is_file(),
        "an unreadable library keeps its file"
    );
    let a_new = std::fs::read_to_string(out_dir.join("symbols/a.snxsym")).expect("a");
    assert!(a_new.contains("version = \"1.0.1\""), "{a_new}");
    let value: toml::Value =
        toml::from_str(&std::fs::read_to_string(out_dir.join("L.snxlib")).expect("m"))
            .expect("toml");
    let tables = value["tables"].as_table().expect("tables");
    assert_eq!(tables.len(), 2, "B's table is carried over: {tables:?}");
    let row = |t: &str| {
        value["tables"][t]["tsv"]
            .as_str()
            .expect("tsv")
            .lines()
            .nth(1)
            .expect("row")
            .to_string()
    };
    assert_eq!(row("A").split('\t').nth(16), Some("1.0.1"));
    assert_eq!(
        row("A").split('\t').nth(18),
        Some("1.0.1"),
        "the row follows its symbol's version"
    );
    assert_eq!(row("B").split('\t').nth(16), Some("1.0.0"));

    // B leaves the inputs altogether.
    let r = run(&[
        "library",
        a.to_str().expect("utf8"),
        "--output",
        out,
        "--library-id",
        id,
        "--name",
        "L",
        "--update",
        "--json",
    ]);
    assert!(r.status.code() == Some(0) || r.status.code() == Some(2));
    // Policy (snapshot, contract section 5): a library that is no longer an
    // input is removed; one that is an input but failed to read (above) stays.
    assert!(!out_dir.join("symbols/b.snxsym").exists());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_update_removes_a_library_that_left_the_inputs_and_keeps_an_envelope_time_across_a_rename() {
    let dir = scratch("lib-removed");
    let (a, b) = (dir.join("A.kicad_sym"), dir.join("B.kicad_sym"));
    std::fs::write(&a, TINY).expect("a");
    std::fs::write(&b, TINY).expect("b");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    let id = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    let go = |inputs: &[&std::path::Path], extra: &[&str]| {
        let mut v: Vec<&str> = vec!["library"];
        v.extend(inputs.iter().map(|p| p.to_str().expect("utf8")));
        v.extend(["--output", out, "--library-id", id, "--name", "L", "--json"]);
        v.extend(extra);
        run(&v)
    };
    assert_eq!(go(&[&a, &b], &[]).status.code(), Some(0));
    let created_of = |f: &str| {
        let t: toml::Value =
            toml::from_str(&std::fs::read_to_string(out_dir.join(f)).expect("f")).expect("toml");
        t["created"].as_str().expect("created").to_string()
    };
    assert!(out_dir.join("symbols/b.snxsym").is_file());
    // B leaves the inputs: its envelope and its rows go, and it is said so.
    let r = go(&[&a], &["--update"]);
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(!out_dir.join("symbols/b.snxsym").exists());
    assert!(out_dir.join("symbols/a.snxsym").is_file());
    let report = report_of(&out_dir);
    let codes: Vec<&str> = report["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str())
        .collect();
    assert!(codes.contains(&"update.library_removed"), "{codes:?}");
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(out_dir.join("L.snxlib")).expect("m"))
            .expect("toml");
    assert!(
        manifest["tables"]
            .as_table()
            .expect("tables")
            .get("B")
            .is_none()
    );
    let _ = created_of("symbols/a.snxsym");
}

#[test]
fn a_report_that_lists_paths_outside_the_library_makes_an_update_delete_nothing() {
    let dir = scratch("lib-crafted");
    let a = dir.join("A.kicad_sym");
    let b = dir.join("B.kicad_sym");
    std::fs::write(&a, TINY).expect("a");
    std::fs::write(&b, TINY).expect("b");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    let id = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    let args = |inputs: &[&std::path::Path], extra: &[&str]| {
        let mut v: Vec<&str> = vec!["library"];
        v.extend(inputs.iter().map(|p| p.to_str().expect("utf8")));
        v.extend(["--output", out, "--library-id", id, "--name", "L", "--json"]);
        v.extend(extra);
        run(&v)
    };
    assert_eq!(args(&[&a, &b], &[]).status.code(), Some(0));
    std::fs::write(dir.join("victim"), "victim").expect("v");
    let path = out_dir.join("import-report.json");
    let mut report: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("r")).expect("json");
    report["files"]
        .as_array_mut()
        .expect("files")
        .push(json!("../victim"));
    std::fs::write(&path, report.to_string()).expect("r");
    // B leaves the inputs, which would delete its files, but the report is not trusted.
    let r = args(&[&a], &["--update"]);
    assert!(r.status.code() == Some(0) || r.status.code() == Some(2));
    assert_eq!(std::fs::read(dir.join("victim")).expect("v"), b"victim");
    assert!(
        out_dir.join("symbols/b.snxsym").is_file(),
        "nothing is deleted when an entry is bad"
    );
    let codes: Vec<String> = report_of(&out_dir)["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str().map(str::to_string))
        .collect();
    assert!(
        codes.contains(&"update.report_entry_ignored".to_string()),
        "{codes:?}"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_update_deletes_only_what_the_converter_wrote_itself() {
    let dir = scratch("lib-ledger");
    let (a, b) = (dir.join("A.kicad_sym"), dir.join("B.kicad_sym"));
    std::fs::write(&a, TINY).expect("a");
    std::fs::write(&b, TINY).expect("b");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    let id = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    let go = |inputs: &[&std::path::Path], extra: &[&str]| {
        let mut v: Vec<&str> = vec!["library"];
        v.extend(inputs.iter().map(|p| p.to_str().expect("utf8")));
        v.extend(["--output", out, "--library-id", id, "--name", "L", "--json"]);
        v.extend(extra);
        run(&v)
    };
    assert_eq!(go(&[&a, &b], &[]).status.code(), Some(0));

    // The user adds a part row (to our table and to a table of their own), a
    // footprint file, and a class.
    let manifest_path = out_dir.join("L.snxlib");
    let mut manifest = std::fs::read_to_string(&manifest_path).expect("m");
    let start = manifest.find("[tables.A]\ntsv = '''\n").expect("table A")
        + "[tables.A]\ntsv = '''\n".len();
    let header_end = start + manifest[start..].find('\n').expect("header") + 1;
    let row_end = header_end + manifest[header_end..].find('\n').expect("row");
    let mut cells: Vec<String> = manifest[header_end..row_end]
        .split('\t')
        .map(str::to_string)
        .collect();
    cells[0] = "00000000-0000-4000-8000-000000000001".into();
    cells[1] = "R-10K".into();
    let user_row = cells.join("\t");
    manifest.insert_str(header_end, &format!("{user_row}\n"));
    manifest.push_str("\n[tables.Mine]\ntsv = '''\nrow_id\tinternal_pn\n00000000-0000-4000-8000-000000000002\tMINE-1\n'''\n");
    std::fs::write(&manifest_path, &manifest).expect("m");
    std::fs::create_dir_all(out_dir.join("footprints")).expect("d");
    std::fs::write(
        out_dir.join("footprints/00000000-0000-4000-8000-0000000000aa.snxfpt"),
        "user footprint",
    )
    .expect("fp");
    std::fs::write(out_dir.join("symbols/user.snxsym"), "user symbols").expect("sym");

    // B leaves the inputs: B's recorded files and rows go, nothing else.
    let r = go(&[&a], &["--update"]);
    assert!(
        matches!(r.status.code(), Some(0 | 2)),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(
        out_dir
            .join("footprints/00000000-0000-4000-8000-0000000000aa.snxfpt")
            .is_file()
    );
    assert!(out_dir.join("symbols/user.snxsym").is_file());
    assert!(!out_dir.join("symbols/b.snxsym").exists());
    let after = std::fs::read_to_string(&manifest_path).expect("m");
    assert!(after.contains("R-10K"), "the user's row survives");
    assert!(
        after.contains("MINE-1") && after.contains("[tables.Mine]"),
        "the user's table survives"
    );
    assert!(after.contains(&user_row), "byte for byte");
    assert!(!after.contains("[tables.B]"));
    let report = report_of(&out_dir);
    let ledger_rows = report["ledger"]["rows"].as_array().expect("rows");
    assert!(
        ledger_rows.iter().all(|r| r["library"] == "A"),
        "{ledger_rows:?}"
    );
    assert!(
        !report
            .to_string()
            .contains("00000000-0000-4000-8000-000000000001")
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn models_are_embedded_with_models_dir_and_left_out_with_skip() {
    let dir = scratch("lib-models");
    let fp = fixtures_dir("footprints/Models.pretty/Good.kicad_mod");
    let models = fixtures_dir("footprints/models");
    let run_with = |out: &str, extra: &[&str]| {
        let mut v: Vec<&str> = vec![
            "library",
            fp.to_str().expect("utf8"),
            "--output",
            out,
            "--json",
        ];
        v.extend(extra);
        run(&v)
    };
    let out1 = dir.join("with");
    let r = run_with(
        out1.to_str().expect("utf8"),
        &["--models-dir", models.to_str().expect("utf8")],
    );
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let file = std::fs::read_dir(out1.join("footprints"))
        .expect("dir")
        .next()
        .expect("file")
        .expect("e")
        .path();
    let text = std::fs::read_to_string(file).expect("fpt");
    assert!(text.starts_with("format = \"snxfpt/1.4\"") && text.contains("[[models]]"));
    let report = report_of(&out1);
    assert_eq!(report["counts"]["written"]["models"], 1);

    let out2 = dir.join("skipped");
    let r = run_with(
        out2.to_str().expect("utf8"),
        &[
            "--models-dir",
            models.to_str().expect("utf8"),
            "--models",
            "skip",
        ],
    );
    assert_eq!(r.status.code(), Some(2));
    let file = std::fs::read_dir(out2.join("footprints"))
        .expect("dir")
        .next()
        .expect("file")
        .expect("e")
        .path();
    let text = std::fs::read_to_string(file).expect("fpt");
    assert!(text.starts_with("format = \"snxfpt/1.1\"") && !text.contains("[[models]]"));
    let codes: Vec<String> = report_of(&out2)["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str().map(str::to_string))
        .collect();
    assert!(codes.contains(&"model.skipped".to_string()));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn license_source_and_notice_are_written_and_a_missing_license_is_said() {
    let dir = scratch("lib-license");
    let sym = write_sym(&dir);
    let sym = sym.to_str().expect("utf8");
    let out_dir = dir.join("out");
    let out = out_dir.to_str().expect("utf8");
    let r = run(&[
        "library",
        sym,
        "--output",
        out,
        "--json",
        "--license",
        "CC-BY-SA-4.0",
        "--source",
        "https://example.com/libs@9.0.0",
    ]);
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(out_dir.join("Tiny.snxlib")).expect("m"))
            .expect("toml");
    let lib = &manifest["library"];
    assert_eq!(lib["license"].as_str(), Some("CC-BY-SA-4.0"));
    assert_eq!(
        lib["source"].as_str(),
        Some("https://example.com/libs@9.0.0")
    );
    assert!(
        lib["converted_by"]
            .as_str()
            .expect("converted_by")
            .starts_with("signex-kicad-import ")
    );
    let license = std::fs::read_to_string(out_dir.join("LICENSE")).expect("LICENSE");
    assert!(
        license.contains("waives article 3")
            && license.contains("Attribution-ShareAlike 4.0 International")
    );
    let notice = std::fs::read_to_string(out_dir.join("NOTICE.md")).expect("NOTICE");
    assert!(
        notice.contains("format conversion")
            && notice.contains("libs@9.0.0")
            && notice.contains("Tiny.kicad_sym")
    );
    let summary = &stdout_lines(&r)[0];
    assert!(summary["files"].as_array().expect("files").len() >= 4);
    // An update rewrites them in place and keeps the bytes.
    let before = std::fs::read(out_dir.join("LICENSE")).expect("l");
    let r = run(&[
        "library",
        sym,
        "--output",
        out,
        "--json",
        "--license",
        "CC-BY-SA-4.0",
        "--source",
        "https://example.com/libs@9.0.0",
        "--update",
    ]);
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert_eq!(std::fs::read(out_dir.join("LICENSE")).expect("l"), before);

    // Without --license: no LICENSE file, no key, an info item.
    let out2 = dir.join("none");
    let r = run(&[
        "library",
        sym,
        "--output",
        out2.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(r.status.code(), Some(0));
    assert!(!out2.join("LICENSE").exists() && out2.join("NOTICE.md").is_file());
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(out2.join("Tiny.snxlib")).expect("m"))
            .expect("toml");
    assert!(manifest["library"].get("license").is_none());
    let codes: Vec<String> = report_of(&out2)["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str().map(str::to_string))
        .collect();
    assert!(codes.contains(&"library.license_unknown".to_string()));
    // Another licence is recorded; its text is not invented.
    let out3 = dir.join("mit");
    run(&[
        "library",
        sym,
        "--output",
        out3.to_str().expect("utf8"),
        "--json",
        "--license",
        "MIT",
    ]);
    assert!(!out3.join("LICENSE").exists());
    assert!(
        std::fs::read_to_string(out3.join("NOTICE.md"))
            .expect("n")
            .contains("no text for this licence")
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn split_output_writes_two_libraries_and_the_parts_point_at_the_footprint_library() {
    let dir = scratch("lib-split");
    let sym = dir.join("Parts.kicad_sym");
    std::fs::write(
        &sym,
        TINY.replace(
            "(property \"Value\" \"R\")",
            "(property \"Value\" \"R\") (property \"Footprint\" \"Test:R_Test_0603\")",
        ),
    )
    .expect("sym");
    let pretty = fixtures_dir("footprints/Test.pretty");
    let run_split = |tag: &str| {
        let (main, fps) = (
            dir.join(format!("main-{tag}")),
            dir.join(format!("fps-{tag}")),
        );
        let r = run(&[
            "library",
            sym.to_str().expect("utf8"),
            pretty.to_str().expect("utf8"),
            "--output",
            main.to_str().expect("utf8"),
            "--split-footprints",
            fps.to_str().expect("utf8"),
            "--name",
            "Standard Symbols & Parts",
            "--footprints-name",
            "Standard Footprints",
            "--license",
            "CC-BY-SA-4.0",
            "--json",
        ]);
        assert!(
            matches!(r.status.code(), Some(0 | 2)),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        (main, fps)
    };
    let (main, fps) = run_split("a");
    let toml_of = |p: std::path::PathBuf| -> toml::Value {
        toml::from_str(&std::fs::read_to_string(p).expect("file")).expect("toml")
    };
    let parts_lib = toml_of(main.join("Standard Symbols & Parts.snxlib"));
    let fp_lib = toml_of(fps.join("Standard Footprints.snxlib"));
    let fp_id = fp_lib["library_id"].as_str().expect("id").to_string();
    assert_ne!(fp_id, parts_lib["library_id"].as_str().expect("id"));
    assert_eq!(
        parts_lib["library"]["requires"][0].as_str(),
        Some(fp_id.as_str())
    );
    assert!(fp_lib["library"].get("requires").is_none() && fp_lib.get("tables").is_none());
    // The footprints live only in the footprint library; both carry licence and notice.
    assert!(!main.join("footprints").exists() && main.join("symbols").is_dir());
    assert!(fps.join("footprints").is_dir() && !fps.join("symbols").exists());
    for d in [&main, &fps] {
        assert!(
            d.join("LICENSE").is_file() && d.join("NOTICE.md").is_file(),
            "{d:?}"
        );
    }
    // The part's footprint reference names the footprint library and an existing file.
    let tsv = parts_lib["tables"]["Parts"]["tsv"].as_str().expect("tsv");
    let cells: Vec<&str> = tsv.lines().nth(1).expect("row").split('\t').collect();
    let fp_ref: Value = serde_json::from_str(cells[6]).expect("ref");
    assert_eq!(fp_ref["library_id"].as_str(), Some(fp_id.as_str()));
    let uuid = fp_ref["uuid"].as_str().expect("uuid");
    assert!(fps.join(format!("footprints/{uuid}.snxfpt")).is_file());
    // Deterministic: a second run gives the same ids and bytes.
    let (main2, fps2) = run_split("b");
    assert_eq!(
        std::fs::read(main.join("Standard Symbols & Parts.snxlib")).expect("a"),
        std::fs::read(main2.join("Standard Symbols & Parts.snxlib")).expect("b")
    );
    assert_eq!(
        std::fs::read(fps.join("Standard Footprints.snxlib")).expect("a"),
        std::fs::read(fps2.join("Standard Footprints.snxlib")).expect("b")
    );
    // The footprint library's id follows --library-id, so another project does not share it.
    let other = dir.join("other-main");
    let other_fps = dir.join("other-fps");
    run(&[
        "library",
        sym.to_str().expect("utf8"),
        pretty.to_str().expect("utf8"),
        "--output",
        other.to_str().expect("utf8"),
        "--split-footprints",
        other_fps.to_str().expect("utf8"),
        "--name",
        "Standard Symbols & Parts",
        "--footprints-name",
        "Standard Footprints",
        "--library-id",
        "6f9619ff-8b86-d011-b42d-00c04fc964ff",
        "--json",
    ]);
    let other_lib = toml_of(other_fps.join("Standard Footprints.snxlib"));
    assert_ne!(other_lib["library_id"].as_str(), Some(fp_id.as_str()));
    // A split library is not updated, and the two folders must be apart.
    let r = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        main.to_str().expect("utf8"),
        "--split-footprints",
        fps.to_str().expect("utf8"),
        "--update",
        "--json",
    ]);
    assert_eq!(r.status.code(), Some(1));
    let r = run(&[
        "library",
        sym.to_str().expect("utf8"),
        "--output",
        dir.join("same").to_str().expect("utf8"),
        "--split-footprints",
        dir.join("same/inner").to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(r.status.code(), Some(1));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// A library of one symbol library "A" written by the converter, and a helper to update it.
fn updatable(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let dir = scratch(tag);
    let a = dir.join("A.kicad_sym");
    std::fs::write(&a, TINY).expect("a");
    let out_dir = dir.join("out");
    let r = run(&[
        "library",
        a.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--library-id",
        "6f9619ff-8b86-d011-b42d-00c04fc964ff",
        "--name",
        "L",
        "--json",
    ]);
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    (dir, a, out_dir)
}

fn update(a: &std::path::Path, out_dir: &std::path::Path) -> std::process::Output {
    run(&[
        "library",
        a.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--library-id",
        "6f9619ff-8b86-d011-b42d-00c04fc964ff",
        "--name",
        "L",
        "--update",
        "--json",
    ])
}

#[test]
fn an_update_keeps_what_the_user_set_in_the_manifest_byte_for_byte() {
    let (dir, a, out_dir) = updatable("lib-manifest-keep");
    let path = out_dir.join("L.snxlib");
    let mut text = std::fs::read_to_string(&path).expect("m");
    // The user's own settings, a user table with sidecars, an unknown key and a price record.
    text = text
        .replace("name = \"L\"\n", "name = \"L\"\ndescription = \"By hand\"\nfuture_key = 7\n")
        .replace("mode = \"personal\"\nreview_required = false\nreviewers_required = 1", "mode = \"team\"\nreview_required = true\nreviewers_required = 2")
        .replace("allow_self_signup = false\n", "allow_self_signup = false\n\n[users.\"ada@example.com\"]\ndisplay_name = \"Ada\"\nroles = [\"Librarian\"]\n");
    text.push_str("\n[[classes]]\nkey = \"mine\"\nlabel = \"Mine\"\n\n[tables.Mine]\ntsv = '''\nrow_id\tinternal_pn\n00000000-0000-4000-8000-0000000000c1\tMINE-1\n'''\n\n[tables.Mine.column_types]\ninternal_pn = \"string\"\n");
    std::fs::write(&path, &text).expect("m");
    let r = update(&a, &out_dir);
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let after = std::fs::read_to_string(&path).expect("m");
    for kept in [
        "description = \"By hand\"\n",
        "\nfuture_key = 7\n",
        "mode = \"team\"\nreview_required = true\nreviewers_required = 2\n",
        "[users.\"ada@example.com\"]\ndisplay_name = \"Ada\"\nroles = [\"Librarian\"]\n",
        "[[classes]]\nkey = \"mine\"\nlabel = \"Mine\"\n",
        "[tables.Mine]\ntsv = '''\nrow_id\tinternal_pn\n00000000-0000-4000-8000-0000000000c1\tMINE-1\n'''\n\n[tables.Mine.column_types]\ninternal_pn = \"string\"\n",
    ] {
        assert!(after.contains(kept), "lost {kept:?} in\n{after}");
    }
    // Only the converter-owned key (converted_by) was added; a second update changes nothing.
    // (it moves up to follow `description`, where the format puts it).
    let strip = |t: &str| t.replace("converted_by = \"signex-kicad-import 0.2.0\"\n", "");
    assert_eq!(strip(&after), strip(&text));
    let r = update(&a, &out_dir);
    assert_eq!(r.status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(&path).expect("m"),
        after,
        "byte-identical when nothing changed"
    );
    // A database library is refused.
    std::fs::write(
        &path,
        after.replace("kind = \"local_git\"", "kind = \"database\""),
    )
    .expect("m");
    let r = update(&a, &out_dir);
    assert_eq!(r.status.code(), Some(1));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn an_update_never_overwrites_a_users_file_and_keeps_files_a_user_row_refers_to() {
    let (dir, a, out_dir) = updatable("lib-untracked");
    // The converter's envelope is a.snxsym; remove it from the ledger's view by
    // deleting the report entry's file, then put a user file under that name.
    let mine = out_dir.join("symbols/a.snxsym");
    let ours = std::fs::read(&mine).expect("a");
    let report_path = out_dir.join("import-report.json");
    let mut report: Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("r")).expect("json");
    report["files"] = json!(
        report["files"]
            .as_array()
            .expect("files")
            .iter()
            .filter(|f| f != &&json!("symbols/a.snxsym"))
            .cloned()
            .collect::<Vec<_>>()
    );
    std::fs::write(&report_path, report.to_string()).expect("r");
    std::fs::write(&mine, "the user's own symbols").expect("user file");
    let r = update(&a, &out_dir);
    assert!(
        matches!(r.status.code(), Some(0 | 2)),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert_eq!(std::fs::read(&mine).expect("a"), b"the user's own symbols");
    let alt: Vec<_> = std::fs::read_dir(out_dir.join("symbols"))
        .expect("d")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        alt.iter()
            .any(|n| n != "a.snxsym" && n.ends_with(".snxsym")),
        "{alt:?}"
    );
    let _ = ours;
    std::fs::remove_dir_all(&dir).expect("cleanup");

    // A user row that refers to a symbol keeps the symbol's file when the source drops it.
    let (dir, a, out_dir) = updatable("lib-referenced");
    let path = out_dir.join("L.snxlib");
    let manifest = std::fs::read_to_string(&path).expect("m");
    let start =
        manifest.find("[tables.A]\ntsv = '''\n").expect("table") + "[tables.A]\ntsv = '''\n".len();
    let header_end = start + manifest[start..].find('\n').expect("h") + 1;
    let row_end = header_end + manifest[header_end..].find('\n').expect("r");
    let mut cells: Vec<String> = manifest[header_end..row_end]
        .split('\t')
        .map(str::to_string)
        .collect();
    cells[0] = "00000000-0000-4000-8000-0000000000e1".into();
    cells[1] = "USER-R".into();
    let mut edited = manifest.clone();
    edited.insert_str(header_end, &format!("{}\n", cells.join("\t")));
    std::fs::write(&path, edited).expect("m");
    // The source loses symbol R, so the converter would delete a.snxsym.
    std::fs::write(
        &a,
        "(kicad_symbol_lib (version 20231120) (generator test))\n",
    )
    .expect("a");
    let other = dir.join("B.kicad_sym");
    std::fs::write(
        &other,
        TINY.replace("\"R\"", "\"S\"").replace("R_1_1", "S_1_1"),
    )
    .expect("b");
    let r = run(&[
        "library",
        other.to_str().expect("utf8"),
        "--output",
        out_dir.to_str().expect("utf8"),
        "--library-id",
        "6f9619ff-8b86-d011-b42d-00c04fc964ff",
        "--name",
        "L",
        "--update",
        "--json",
    ]);
    assert!(
        matches!(r.status.code(), Some(0 | 2)),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(
        out_dir.join("symbols/a.snxsym").is_file(),
        "kept for the user's row"
    );
    let codes: Vec<String> = report_of(&out_dir)["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["code"].as_str().map(str::to_string))
        .collect();
    assert!(
        codes.contains(&"update.file_kept_referenced".to_string()),
        "{codes:?}"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_users_part_number_equal_to_a_new_rows_refuses_the_update_before_any_write() {
    let (dir, a, out_dir) = updatable("lib-pn-clash");
    let path = out_dir.join("L.snxlib");
    let manifest = std::fs::read_to_string(&path).expect("m");
    let start =
        manifest.find("[tables.A]\ntsv = '''\n").expect("table") + "[tables.A]\ntsv = '''\n".len();
    let header_end = start + manifest[start..].find('\n').expect("h") + 1;
    let row_end = header_end + manifest[header_end..].find('\n').expect("r");
    let mut cells: Vec<String> = manifest[header_end..row_end]
        .split('\t')
        .map(str::to_string)
        .collect();
    // The user's row takes the part number of a row the next import will write.
    let new_symbol = dir.join("A2.kicad_sym");
    let _ = new_symbol;
    cells[0] = "00000000-0000-4000-8000-0000000000e2".into();
    cells[1] = "A:S".into();
    let mut edited = manifest.clone();
    edited.insert_str(header_end, &format!("{}\n", cells.join("\t")));
    std::fs::write(&path, &edited).expect("m");
    std::fs::write(&a, TINY.replace("\"R\"", "\"S\"").replace("R_1_1", "S_1_1")).expect("a");
    let before = std::fs::read(out_dir.join("symbols/a.snxsym")).expect("a");
    let r = update(&a, &out_dir);
    assert_eq!(
        r.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let error = stdout_lines(&r)[0]["error"]
        .as_str()
        .expect("error")
        .to_string();
    assert!(
        error.contains("A:S") && error.contains("00000000-0000-4000-8000-0000000000e2"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(out_dir.join("symbols/a.snxsym")).expect("a"),
        before
    );
    assert_eq!(std::fs::read_to_string(&path).expect("m"), edited);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
