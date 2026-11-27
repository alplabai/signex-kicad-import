// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The pad layer sets the converter writes against the Signex library
//! format's conformance footprint with one pad of each layer set. The
//! fixture belongs to the Signex repository: set `SIGNEX_FORMAT_FIXTURES` to
//! its `docs/fixtures/library-format` folder (commit 0b167c854 or later);
//! without it the test prints a skip message and passes.

use std::path::Path;

use kicad_parser::footprint_lib::convert_footprint;
use snx_writer::write_footprint_file;

#[test]
fn the_layer_set_pads_match_the_fixture_rows() {
    let Some(dir) = std::env::var_os("SIGNEX_FORMAT_FIXTURES") else {
        eprintln!("skipped: SIGNEX_FORMAT_FIXTURES is not set");
        return;
    };
    let want_path =
        Path::new(&dir).join("library/footprints/01940000-0000-7000-8000-000000000024.snxfpt");
    let want = std::fs::read_to_string(&want_path)
        .unwrap_or_else(|e| panic!("{}: {e}", want_path.display()));
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/footprints/Special.pretty/Layers_Test.kicad_mod");
    let text = std::fs::read_to_string(source).expect("fixture");
    let mut items = Vec::new();
    let file = convert_footprint(
        &text,
        "Layers",
        &|_, n| n.to_string(),
        &mut items,
        &kicad_parser::models::ModelCx::default(),
        Path::new("."),
    )
    .expect("converts");
    let got = write_footprint_file(&file);
    // The converter's footprint carries a castellated and a test-point pad the
    // fixture has not; the fixture's four rows must be among ours, as written.
    let rows = |t: &str| -> Vec<String> {
        t.lines()
            .skip_while(|l| !l.starts_with("pads_tsv"))
            .skip(2)
            .take_while(|l| *l != "'''")
            .map(str::to_string)
            .collect()
    };
    let (got_rows, want_rows) = (rows(&got), rows(&want));
    assert_eq!(want_rows.len(), 4, "{want_rows:?}");
    for row in &want_rows {
        assert!(
            got_rows.contains(row),
            "missing row {row:?} in {got_rows:#?}"
        );
    }
}
