// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The symbol reader against the hand-written fixtures in
//! `tests/fixtures/symbols` and against damaged input.

use std::path::{Path, PathBuf};

use super::*;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/symbols")
}

fn read(name: &str) -> SymbolLibRead {
    let text = std::fs::read_to_string(dir().join(name)).expect("fixture");
    let stem = name.trim_end_matches(".kicad_sym");
    read_symbol_lib(&text, stem).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn codes(read: &SymbolLibRead) -> Vec<&str> {
    read.items.iter().map(|i| i.code.as_str()).collect()
}

fn unit(symbol: &Symbol, unit: u32, style: u32) -> &Unit {
    symbol
        .units
        .iter()
        .find(|u| u.unit == unit && u.style == style)
        .unwrap_or_else(|| panic!("no unit {unit} style {style}"))
}

#[test]
fn a_plain_symbol_reads_with_properties_and_settings() {
    let read = read("basic.kicad_sym");
    assert!(read.items.is_empty(), "{:?}", read.items);
    assert_eq!(read.lib.version.as_deref(), Some("20231120"));
    assert_eq!(read.lib.generator.as_deref(), Some("handwritten"));
    let s = read.lib.symbol("R_Test").expect("symbol");
    assert_eq!(s.property("Reference"), Some("R"));
    assert_eq!(s.property("Datasheet"), Some("~"));
    assert_eq!(s.property("Manufacturer"), Some("Acme"));
    assert_eq!(s.description(), Some("Resistor, test fixture"));
    assert_eq!(s.keywords(), Some("R res resistor"));
    assert_eq!(s.footprint_filters(), Some("R_* Resistor_*"));
    assert!(s.pin_numbers_hidden && s.pin_names_hidden);
    assert_eq!(s.pin_names_offset, Some(0.0));
    assert!(s.in_bom && s.on_board);
    assert_eq!(s.exclude_from_sim, Some(false));
    assert!(!s.power);
    let footprint = s
        .properties
        .iter()
        .find(|p| p.key == "Footprint")
        .expect("property");
    assert!(footprint.hidden);
    assert_eq!(
        footprint.position,
        Some((Point { x: -1.778, y: 0.0 }, 90.0))
    );
    assert_eq!(footprint.effects.size, Some((1.27, 1.27)));
    assert_eq!(s.pin_count(), 2);
    let pin = &unit(s, 1, 1).pins[0];
    assert_eq!(pin.electrical, PinType::Passive);
    assert_eq!(pin.style, PinStyle::Line);
    assert_eq!(
        (pin.position, pin.angle, pin.length),
        (Point { x: 0.0, y: 3.81 }, 270.0, 1.27)
    );
    assert_eq!((pin.name.as_str(), pin.number.as_str()), ("~", "1"));
    assert!(!pin.hidden && pin.alternates.is_empty());
}

#[test]
fn units_body_styles_and_every_graphic_kind_read() {
    let read = read("multiunit.kicad_sym");
    assert!(read.items.is_empty(), "{:?}", read.items);
    let s = read.lib.symbol("Dual_Gate").expect("symbol");
    assert_eq!(s.pin_names_offset, Some(0.254));
    assert!(!s.pin_names_hidden);
    let keys: Vec<(u32, u32)> = s.units.iter().map(|u| (u.unit, u.style)).collect();
    assert_eq!(keys, [(0, 1), (1, 1), (1, 2), (2, 1), (3, 1)]);
    // Unit 0 is common to all units and holds the shared drawing.
    let common = unit(s, 0, 1);
    assert_eq!(common.graphics.len(), 4);
    assert!(common.pins.is_empty());
    assert!(
        matches!(&common.graphics[0], Graphic::Text { text, effects, .. }
        if text == "NOTE" && effects.italic && effects.justify == ["left", "bottom"])
    );
    assert!(
        matches!(&common.graphics[1], Graphic::Circle { radius, fill: Fill::Background, .. } if *radius == 0.5)
    );
    assert!(
        matches!(&common.graphics[2], Graphic::Arc { mid, fill: Fill::Outline, stroke, .. }
        if *mid == Point { x: 1.27, y: 0.0 } && stroke.color == Some([0.0; 4]))
    );
    assert!(matches!(&common.graphics[3], Graphic::Bezier { points, .. } if points.len() == 4));
    // De Morgan: the same unit in body style 2 has its own drawing and pins.
    assert_eq!(unit(s, 1, 2).pins.len(), 1);
    assert_eq!(unit(s, 1, 2).pins[0].style, PinStyle::Inverted);
    assert_eq!(unit(s, 1, 1).name.as_deref(), Some("Gate A"));
    assert_eq!(unit(s, 2, 1).name.as_deref(), Some("Gate B"));
    assert!(
        matches!(&unit(s, 1, 1).graphics[0], Graphic::Polyline { points, fill: Fill::Background, .. } if points.len() == 4)
    );
    let gate_a = &unit(s, 1, 1).pins;
    assert_eq!(gate_a[1].electrical, PinType::Output);
    assert_eq!(gate_a[1].style, PinStyle::Inverted);
    assert_eq!(unit(s, 2, 1).pins[0].style, PinStyle::Clock);
    let power = &unit(s, 3, 1).pins;
    assert_eq!(power[0].electrical, PinType::PowerIn);
    assert!(power[1].name_hidden && !power[1].number_hidden && !power[1].hidden);
    assert_eq!(s.pin_count(), 6);
}

#[test]
fn alternates_and_both_hide_forms_read() {
    let read = read("alternates.kicad_sym");
    assert!(read.items.is_empty(), "{:?}", read.items);
    let s = read.lib.symbol("MCU_Alt").expect("symbol");
    let pins = &unit(s, 1, 1).pins;
    assert_eq!(pins[0].electrical, PinType::Bidirectional);
    let alts: Vec<(&str, PinType, PinStyle)> = pins[0]
        .alternates
        .iter()
        .map(|a| (a.name.as_str(), a.electrical, a.style))
        .collect();
    assert_eq!(
        alts,
        [
            ("ADC0", PinType::Input, PinStyle::Line),
            ("TIM2_CH1", PinType::Bidirectional, PinStyle::Clock),
            ("UART_TX", PinType::Output, PinStyle::Inverted),
        ]
    );
    assert!(!pins[0].hidden);
    assert!(pins[1].hidden, "old form: a bare hide");
    assert!(pins[2].hidden, "new form: (hide yes)");
    assert_eq!(pins[2].electrical, PinType::NoConnect);
}

#[test]
fn extends_resolves_through_the_chain_and_breaks_are_reported() {
    let read = read("extends.kicad_sym");
    let lib = &read.lib;
    let derived = lib.symbol("Derived_Part").expect("symbol");
    assert_eq!(derived.extends.as_deref(), Some("Base_Part"));
    assert!(derived.units.is_empty());
    assert_eq!(derived.property("Footprint"), Some("Package_X:DERIVED"));
    // Drawing comes from the parent, two levels up for the grandchild.
    let units = lib.effective_units(derived).expect("units");
    assert_eq!(units.len(), 1);
    let grandchild = lib.symbol("Grandchild_Part").expect("symbol");
    assert_eq!(lib.effective_units(grandchild), Some(units));
    // The repeated name keeps the first; the report says so.
    assert_eq!(
        lib.symbol("Base_Part")
            .expect("symbol")
            .property("Reference"),
        Some("U")
    );
    assert_eq!(
        lib.symbols.iter().filter(|s| s.name == "Base_Part").count(),
        1
    );
    let by_code = |code: &str| -> Vec<&str> {
        read.items
            .iter()
            .filter(|i| i.code == code)
            .map(|i| i.source.as_str())
            .collect()
    };
    assert_eq!(by_code("symbol.duplicate_name"), ["extends:Base_Part"]);
    assert_eq!(by_code("symbol.extends_missing"), ["extends:Orphan_Part"]);
    let mut cycle = by_code("symbol.extends_cycle");
    cycle.sort_unstable();
    assert_eq!(cycle, ["extends:Loop_A", "extends:Loop_B"]);
    assert!(read.items.iter().all(|i| i.severity == Severity::Error));
}

#[test]
fn power_flag_and_bom_flags_read() {
    let read = read("power.kicad_sym");
    assert!(read.items.is_empty(), "{:?}", read.items);
    let gnd = read.lib.symbol("GND_Test").expect("symbol");
    assert!(gnd.power);
    assert_eq!(gnd.power_scope, None);
    assert!(!gnd.in_bom && gnd.on_board);
    assert_eq!(gnd.exclude_from_sim, Some(true));
    assert_eq!(unit(gnd, 1, 1).pins[0].electrical, PinType::PowerIn);
    assert_eq!(unit(gnd, 1, 1).pins[0].length, 0.0);
    let vcc = read.lib.symbol("VCC_Test").expect("symbol");
    assert!(vcc.power);
    assert_eq!(vcc.power_scope.as_deref(), Some("global"));
    assert!(!vcc.in_bom && !vcc.on_board);
    assert_eq!(vcc.exclude_from_sim, None);
}

#[test]
fn unknown_and_unreadable_parts_are_reported_not_fatal() {
    let read = read("unknown.kicad_sym");
    let mut got = codes(&read);
    got.sort_unstable();
    assert_eq!(
        got,
        [
            "symbol.body_style_beyond_two",
            "symbol.fill_unknown",
            "symbol.graphic_unreadable",
            "symbol.pin_angle_unusual",
            "symbol.pin_length_missing",
            "symbol.pin_style_unknown",
            "symbol.pin_type_unknown",
            "symbol.pin_unreadable",
            "symbol.token_unknown",
            "symbol.unit_name_mismatch",
            "symbol.unit_token_unknown",
            "symbol.unit_unreadable",
            "symlib.token_unknown",
        ]
    );
    let severity_of = |code: &str| {
        read.items
            .iter()
            .find(|i| i.code == code)
            .map(|i| i.severity)
            .expect("item")
    };
    assert_eq!(severity_of("symbol.token_unknown"), Severity::Info);
    assert_eq!(severity_of("symbol.pin_unreadable"), Severity::Dropped);
    assert_eq!(severity_of("symbol.unit_unreadable"), Severity::Dropped);
    let tokens: Vec<&str> = read
        .items
        .iter()
        .filter(|i| i.code == "symbol.token_unknown")
        .filter_map(|i| i.detail["token"].as_str())
        .collect();
    // A flag at the symbol level; embedded_fonts is quiet.
    assert_eq!(tokens, ["shiny_new_flag"]);
    let unit_token = read
        .items
        .iter()
        .find(|i| i.code == "symbol.unit_token_unknown")
        .expect("item");
    assert_eq!(unit_token.detail["token"], "text_box");
    let s = read.lib.symbol("Odd_Part").expect("symbol");
    // What could be read was kept: the good rectangle, two pins with fallbacks.
    let u = unit(s, 1, 1);
    assert_eq!(u.graphics.len(), 1);
    assert_eq!(u.pins.len(), 3);
    assert_eq!(u.pins[0].electrical, PinType::Unspecified);
    assert_eq!(u.pins[1].style, PinStyle::Line);
    assert_eq!(u.pins[2].angle, 45.0);
    let two = read.lib.symbol("Odd_Part_Two").expect("symbol");
    // The unreadable unit is dropped; a unit named after another symbol and
    // a third body style are kept (reported as info).
    let styles: Vec<u32> = two.units.iter().map(|u| u.style).collect();
    assert_eq!(styles, [3, 1]);
}

#[test]
fn a_symdir_reads_one_symbol_per_file() {
    let read = read_symbol_dir(&dir().join("Parts.kicad_symdir")).expect("dir");
    let names: Vec<&str> = read.lib.symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["Alpha", "Beta", "Gamma"],
        "files in name order; the bare-symbol file reads too"
    );
    let beta = read.lib.symbol("Beta").expect("symbol");
    // Beta's parent is in another file of the folder.
    let units = read.lib.effective_units(beta).expect("units");
    assert_eq!(units[0].pins[0].name, "IN");
    let by_code = |code: &str| -> Vec<(&str, Severity)> {
        read.items
            .iter()
            .filter(|i| i.code == code)
            .map(|i| (i.source.as_str(), i.severity))
            .collect()
    };
    assert_eq!(
        by_code("symlib.file_unreadable"),
        [("Parts", Severity::Error)]
    );
    assert_eq!(by_code("symlib.file_ignored"), [("Parts", Severity::Info)]);
    assert!(by_code("symbol.extends_missing").is_empty());
}

#[test]
fn damaged_input_is_an_error_and_never_a_panic() {
    let whole = std::fs::read_to_string(dir().join("multiunit.kicad_sym")).expect("fixture");
    // Every prefix of a real file: truncation anywhere is an error or a
    // (smaller) library, never a panic.
    let mut cut = 0;
    let mut errors = 0;
    while cut < whole.len() {
        if whole.is_char_boundary(cut) && read_symbol_lib(&whole[..cut], "cut").is_err() {
            errors += 1;
        }
        cut += 7;
    }
    assert!(errors > 10, "truncations should be refused ({errors})");
    for garbage in [
        "",
        "(",
        ")",
        "(((((",
        "\"unterminated",
        "just words",
        "(kicad_symbol_lib",
        "(kicad_symbol_lib (symbol))",
        "(kicad_symbol_lib (symbol \"A\" (symbol \"A_1_1\" (pin))))",
        "(kicad_symbol_lib (symbol \"A\" (pin passive line (at x y))))",
        "(footprint \"A\")",
        "\u{0}\u{1}\u{2}",
        "(kicad_symbol_lib (symbol \"\u{fffd}\" (property)))",
    ] {
        let result = read_symbol_lib(garbage, "g");
        if let Ok(read) = &result {
            assert!(read.lib.symbols.len() <= 1, "{garbage:?}");
        }
    }
    assert!(read_symbol_lib("", "g").is_err());
    assert!(read_symbol_lib("(footprint \"A\")", "g").is_err());
    let deep = format!(
        "(kicad_symbol_lib {}{})",
        "(a ".repeat(3000),
        ")".repeat(3000)
    );
    // Deep nesting neither overflows the stack nor panics.
    let _ = read_symbol_lib(&deep, "deep");
}

#[test]
fn a_symbol_with_no_units_and_no_extends_is_empty_not_broken() {
    let read = read_symbol_lib("(kicad_symbol_lib (symbol \"Bare\"))", "bare").expect("reads");
    let s = read.lib.symbol("Bare").expect("symbol");
    assert_eq!(read.lib.effective_units(s), Some(&[][..]));
    assert!(read.items.is_empty());
}

#[test]
fn bytes_that_are_not_utf8_are_replaced_and_reported() {
    let path = std::env::temp_dir().join(format!("snx-utf8-{}.kicad_sym", std::process::id()));
    let mut bytes = b"(kicad_symbol_lib (symbol \"A".to_vec();
    bytes.extend([0xff, 0xfe]);
    bytes.extend(b"\"))");
    std::fs::write(&path, bytes).expect("file");
    let read = read_symbol_file(&path).expect("reads");
    std::fs::remove_file(&path).expect("cleanup");
    assert_eq!(read.lib.symbols.len(), 1);
    assert!(read.lib.symbols[0].name.contains('\u{fffd}'));
    let item = read
        .items
        .iter()
        .find(|i| i.code == "symlib.invalid_utf8")
        .expect("item");
    assert_eq!(item.severity, Severity::Approximated);
}

#[test]
fn current_default_tokens_are_known_and_only_a_jumper_flag_that_is_on_is_reported() {
    let text = "(kicad_symbol_lib (symbol \"A\" (in_pos_files yes) (duplicate_pin_numbers_are_jumpers no) (body_styles demorgan)\n  (symbol \"A_1_1\" (pin passive line (at 0 0 0) (length 1) (name \"A\") (number \"1\"))))\n  (symbol \"B\" (duplicate_pin_numbers_are_jumpers yes)))";
    let read = read_symbol_lib(text, "t").expect("reads");
    let codes: Vec<&str> = read.items.iter().map(|i| i.code.as_str()).collect();
    assert_eq!(codes, ["symbol.jumper_pins_not_carried"], "{codes:?}");
    assert_eq!(read.items[0].source, "t:B");
}
