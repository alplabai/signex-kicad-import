// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Real boards from four source format generations (see
//! `tests/fixtures/README.md`): each converts, every track end that meets
//! a pad meets one of its own net, and the written file is valid TOML in
//! the current format.

use std::path::Path;

use kicad_parser::convert_pcb;
use kicad_parser::placement_check::tally_track_ends;

fn convert(name: &str) -> kicad_parser::Converted {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    convert_pcb(&text, None).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn check(name: &str, min_footprints: usize) {
    let converted = convert(name);
    let board = &converted.board;
    assert!(
        board.footprints.len() >= min_footprints,
        "{name}: {} footprints",
        board.footprints.len()
    );
    assert!(!board.nets.is_empty(), "{name}: no nets");
    assert!(
        board.layers.iter().any(|l| l.token == "TopCopper")
            && board.layers.iter().any(|l| l.token == "BottomCopper"),
        "{name}: copper stack"
    );
    let tally = tally_track_ends(board);
    assert_eq!(tally.other_net, 0, "{name}: {tally:?}");
    let text = snx_writer::write_board(board);
    let value: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("{name}: not TOML: {e}"));
    assert_eq!(value["format"].as_str(), Some(snx_writer::FORMAT_TOKEN));
    assert_eq!(value["generator"].as_str(), Some(snx_writer::GENERATOR));
    for property in board.footprints.iter().flat_map(|f| &f.properties) {
        assert!(
            !property.key.starts_with("ki_") && property.key != "Sheetfile",
            "{name}: source bookkeeping field {}",
            property.key
        );
    }
}

#[test]
fn a_version_6_board_converts() {
    check("v6-rp2040-designguide.kicad_pcb", 10);
}

#[test]
fn a_version_7_board_converts() {
    check("v7-microwave-probe.kicad_pcb", 1);
}

#[test]
fn a_version_8_board_converts() {
    check("v8-n64-expansion-pak.kicad_pcb", 5);
}

#[test]
fn a_version_9_board_converts() {
    check("v9-pi-tin-display.kicad_pcb", 5);
}
