// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The mapping of the symbol fixtures to Signex symbols.

use std::path::{Path, PathBuf};

use snx_writer::symbol_file::{GraphicKind, Orientation, SymbolFile};
use snx_writer::write_symbol_file;

use super::*;
use crate::items::Severity;
use to_snx::to_symbol_file;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/symbols")
        .join(name)
}

fn map(name: &str) -> (SymbolFile, Vec<Item>) {
    let read = read_symbol_file(&fixture(name)).expect("reads");
    let mut items = Vec::new();
    let stem = name.trim_end_matches(".kicad_sym");
    let file = to_symbol_file(
        &read.lib,
        stem,
        format!("file-{stem}"),
        &|n| format!("sym-{n}"),
        &mut items,
    );
    (file, items)
}

fn codes<'a>(items: &'a [Item], source: &str) -> Vec<&'a str> {
    items
        .iter()
        .filter(|i| i.source == source)
        .map(|i| i.code.as_str())
        .collect()
}

#[test]
fn every_fixture_writes_valid_toml_with_one_trailing_newline() {
    for name in [
        "basic",
        "multiunit",
        "alternates",
        "extends",
        "power",
        "unknown",
    ] {
        let (file, _) = map(&format!("{name}.kicad_sym"));
        let text = write_symbol_file(&file);
        toml::from_str::<toml::Value>(&text).unwrap_or_else(|e| panic!("{name}: {e}\n{text}"));
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"), "{name}");
    }
}

#[test]
fn a_plain_symbol_maps_names_visibility_and_positions() {
    let (file, items) = map("basic.kicad_sym");
    assert!(items.is_empty(), "{items:?}");
    assert_eq!(
        (file.display_name.as_str(), file.file_uuid.as_str()),
        ("basic", "file-basic")
    );
    let s = &file.symbols[0];
    assert_eq!(
        (s.uuid.as_str(), s.name.as_str(), s.designator.as_str()),
        ("sym-R_Test", "R_Test", "R?")
    );
    assert_eq!(s.description, "Resistor, test fixture");
    assert_eq!(s.designator_pos, Some([2.032, 0.0]));
    assert_eq!(s.component_type, "Standard");
    // Same y-up frame: nothing is flipped.
    let p = &s.pins[0];
    assert_eq!(
        (p.pos, p.orientation, p.length),
        ([0.0, 3.81], Orientation::Down, 1.27)
    );
    assert_eq!(
        (p.name.as_str(), p.name_visible, p.designator_visible),
        ("", false, false)
    );
    assert_eq!(
        (p.part_number, p.body_style, p.electrical.as_str()),
        (1, 1, "Passive")
    );
    assert!(matches!(
        s.graphics[0].kind,
        GraphicKind::Rectangle {
            from: [-1.016, -2.54],
            to: [1.016, 2.54]
        }
    ));
    assert_eq!(
        (
            s.graphics[0].part_number,
            s.graphics[0].body_style,
            s.graphics[0].stroke_width
        ),
        (0, 1, 0.254)
    );
}

#[test]
fn units_styles_and_the_lowest_token_carry_over() {
    let (file, items) = map("multiunit.kicad_sym");
    let s = &file.symbols[0];
    assert_eq!(s.part_count, 3);
    let units: Vec<(u8, u8)> = s
        .pins
        .iter()
        .map(|p| (p.part_number, p.body_style))
        .collect();
    assert_eq!(units, [(1, 1), (1, 1), (1, 2), (2, 1), (3, 1), (3, 1)]);
    assert!(s.pins[1].decorations[2] == "Dot", "inverted output");
    assert_eq!(s.pins[3].decorations[2], "ClockEdge");
    assert!(!s.pins[5].hidden && !s.pins[5].name_visible);
    // Unit 0 holds the common drawing: text, circle, arc, bezier.
    let common: Vec<&GraphicKind> = s
        .graphics
        .iter()
        .filter(|g| g.part_number == 0)
        .map(|g| &g.kind)
        .collect();
    assert_eq!(common.len(), 4);
    assert!(matches!(common[2], GraphicKind::Arc { .. }));
    // The bezier makes the file v4; a closed polyline is a polygon.
    let text = write_symbol_file(&file);
    assert!(text.starts_with("format = \"snxsym/v4\"\n"));
    assert!(
        s.graphics
            .iter()
            .any(|g| matches!(&g.kind, GraphicKind::Polygon { vertices } if vertices.len() == 3))
    );
    let sources = codes(&items, "multiunit:Dual_Gate");
    assert!(sources.contains(&"symbol.fill_approximated"), "{sources:?}");
    // The arc and the bezier carry fills Signex cannot hold.
    assert!(sources.contains(&"symbol.fill_dropped"), "{sources:?}");
}

#[test]
fn alternates_become_the_pin_function_text() {
    let (file, items) = map("alternates.kicad_sym");
    let p = &file.symbols[0].pins[0];
    assert_eq!(p.function, ["ADC0", "TIM2_CH1", "UART_TX"]);
    assert!(write_symbol_file(&file).contains("\tADC0|TIM2_CH1|UART_TX\t"));
    assert_eq!(
        codes(&items, "alternates:MCU_Alt"),
        ["symbol.pin_alternates_approximated"]
    );
    assert_eq!(items[0].severity, Severity::Approximated);
    assert_eq!(items[0].detail["count"], 1);
    let hidden: Vec<bool> = file.symbols[0].pins.iter().map(|p| p.hidden).collect();
    assert_eq!(hidden, [false, true, true]);
}

#[test]
fn power_and_bom_flags_are_reported_where_signex_has_no_field() {
    let (file, items) = map("power.kicad_sym");
    assert_eq!(file.symbols[0].component_type, "StandardNoBom");
    assert_eq!(file.symbols[0].designator, "#PWR?");
    assert!(codes(&items, "power:GND_Test").contains(&"symbol.power_not_carried"));
    assert!(codes(&items, "power:GND_Test").contains(&"symbol.exclude_from_sim_not_carried"));
    assert!(codes(&items, "power:VCC_Test").contains(&"symbol.on_board_not_carried"));
    assert!(items.iter().all(|i| i.severity == Severity::Approximated));
}

#[test]
fn derived_symbols_are_part_rows_not_symbols() {
    let (file, _) = map("extends.kicad_sym");
    let names: Vec<&str> = file.symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Base_Part"]);
    assert_eq!(file.symbols[0].pins.len(), 1);
}

#[test]
fn styles_above_two_are_dropped_and_unlisted_pin_words_are_approximated() {
    let (file, items) = map("unknown.kicad_sym");
    let two = file
        .symbols
        .iter()
        .find(|s| s.name == "Odd_Part_Two")
        .expect("symbol");
    // Style 3 dropped; the unit named after another symbol (style 1) is kept.
    assert_eq!(two.pins.len(), 0);
    assert!(
        items
            .iter()
            .any(|i| i.code == "symbol.body_style_dropped" && i.severity == Severity::Dropped)
    );
    let odd = file
        .symbols
        .iter()
        .find(|s| s.name == "Odd_Part")
        .expect("symbol");
    assert_eq!(
        odd.pins[2].orientation,
        Orientation::Up,
        "45 degrees is a tie and rounds up"
    );
    assert!(codes(&items, "unknown:Odd_Part").contains(&"symbol.pin_angle_approximated"));
}
