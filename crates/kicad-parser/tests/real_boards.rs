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
use snx_writer::model::{PadShape, Point};

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
    let written = snx_writer::write_board(board);
    assert_eq!(written.non_finite, 0, "{name}: numbers that are not finite");
    let text = written.text;
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

/// Whether `p` lies inside `ring` (even-odd rule).
fn inside(ring: &[Point], p: Point) -> bool {
    let n = ring.len();
    let mut crossings = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            crossings = !crossings;
        }
    }
    crossings
}

#[test]
fn a_custom_pad_keeps_the_copper_of_its_anchor() {
    // Pad "0" of the edge connector: a 0.75 x 2.8 anchor rectangle with a
    // polygon that only touches its top end. Signex draws a custom pad from
    // its outline alone, so the pad centre must be copper in it.
    let converted = convert("v8-n64-expansion-pak.kicad_pcb");
    let pad = converted
        .board
        .footprints
        .iter()
        .flat_map(|f| &f.pads)
        .find(|p| p.number == "0" && matches!(p.shape, PadShape::Custom(_)))
        .expect("custom pad 0");
    let PadShape::Custom(outline) = &pad.shape else {
        unreachable!()
    };
    assert!(
        inside(outline, Point::new(0.0, 0.0)),
        "pad centre is not copper: {outline:?}"
    );
    // The anchor's far end and the polygon's far corner are both copper.
    assert!(inside(outline, Point::new(0.0, 1.3)), "{outline:?}");
    assert!(inside(outline, Point::new(-1.8, -5.3)), "{outline:?}");
}

#[test]
fn the_physical_stackup_is_carried() {
    let converted = convert("v7-microwave-probe.kicad_pcb");
    let rows = &converted.board.physical_stack;
    let copper: Vec<&str> = rows
        .iter()
        .filter(|r| !r.layer.is_empty())
        .map(|r| r.layer.as_str())
        .collect();
    assert_eq!(
        copper,
        ["TopCopper", "Inner1Copper", "Inner2Copper", "BottomCopper"]
    );
    assert_eq!(rows.iter().filter(|r| r.kind == "dielectric").count(), 3);
    let text = snx_writer::write_board(&converted.board).text;
    let value: toml::Value = toml::from_str(&text).expect("valid TOML");
    let written = value["stackup"]["rows"].as_array().expect("stackup rows");
    assert_eq!(written.len(), rows.len());
    assert_eq!(written[4]["kind"].as_str(), Some("dielectric"));
    assert_eq!(written[4]["thickness_mm"].as_float(), Some(0.17));
}
