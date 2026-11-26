// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Conformance of the library writers with the Signex library format's
//! fixtures. The fixtures belong to the Signex repository (its own licence),
//! so they are not copied here: point `SIGNEX_FORMAT_FIXTURES` at the folder
//! `docs/fixtures/library-format` of a Signex checkout (commit 440d98329 or
//! later). Without the variable the tests print a skip message and pass.
//!
//! Each test builds, in memory, the data the fixture file holds and compares
//! the writer's text with the committed file byte for byte.

use snx_writer::footprint_file::{self as fpt, FootprintFile};
use snx_writer::symbol_file::{Graphic, GraphicKind, Orientation, Pin, Symbol, SymbolFile};
use snx_writer::{write_footprint_file, write_manifest, write_symbol_file};

fn fixture(rel: &str) -> Option<String> {
    let Some(dir) = std::env::var_os("SIGNEX_FORMAT_FIXTURES") else {
        eprintln!("skipped: SIGNEX_FORMAT_FIXTURES is not set (see the test's header)");
        return None;
    };
    let path = std::path::Path::new(&dir).join("library").join(rel);
    Some(std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

const CREATED: &str = "2026-01-01T00:00:00Z";
const UPDATED: &str = "2026-01-02T03:04:05Z";

fn symbol(uuid: &str, name: &str) -> Symbol {
    Symbol {
        uuid: uuid.to_string(),
        name: name.to_string(),
        anchor: [0.0, 0.0],
        pins: vec![],
        designator: "U?".into(),
        comment: "*".into(),
        designator_pos: None,
        comment_pos: None,
        description: String::new(),
        component_type: "Standard".into(),
        mirrored: false,
        local_fill_color: None,
        local_line_color: None,
        local_pin_color: None,
        version: "1.0.0".into(),
        released: false,
        part_count: 1,
        created: CREATED.into(),
        updated: UPDATED.into(),
        graphics: vec![],
        params: vec![],
    }
}

fn pin(
    number: &str,
    name: &str,
    electrical: &str,
    pos: [f64; 2],
    o: Orientation,
    unit: u8,
    style: u8,
) -> Pin {
    let mut p = Pin::new(number, name, electrical, pos, o, 2.54);
    p.part_number = unit;
    p.body_style = style;
    p
}

fn graphic(unit: u8, style: u8, fill: Option<[u8; 4]>, kind: GraphicKind) -> Graphic {
    Graphic {
        stroke_width: 0.254,
        fill,
        part_number: unit,
        body_style: style,
        kind,
    }
}

#[test]
fn the_symbol_envelope_matches_the_fixture_byte_for_byte() {
    let Some(want) = fixture("symbols/conformance-symbols.snxsym") else {
        return;
    };
    let mut resistor = symbol("01940000-0000-7000-8000-000000000011", "Resistor");
    resistor.designator = "R?".into();
    resistor.designator_pos = Some([0.0, 2.54]);
    resistor.comment_pos = Some([0.0, -2.54]);
    resistor.description = "Resistor".into();
    resistor.released = true;
    let mut p1 = pin("1", "1", "Passive", [-5.08, 0.0], Orientation::Right, 1, 1);
    let mut p2 = pin("2", "2", "Passive", [5.08, 0.0], Orientation::Left, 1, 1);
    for p in [&mut p1, &mut p2] {
        p.name_visible = false;
    }
    resistor.pins = vec![p1, p2];
    resistor.graphics = vec![graphic(
        0,
        1,
        None,
        GraphicKind::Rectangle {
            from: [-2.54, 1.016],
            to: [2.54, -1.016],
        },
    )];

    let mut nand = symbol("01940000-0000-7000-8000-000000000012", "Dual NAND");
    nand.description = "Dual 2-input \"NAND\" gate, De Morgan bodies".into();
    nand.local_line_color = Some([0, 0, 128, 255]);
    nand.part_count = 2;
    let mut a = pin("1", "A", "Input", [-7.62, 10.16], Orientation::Right, 1, 0);
    a.function = vec!["A1".into(), "SCL".into()];
    let mut b = pin("2", "B", "Input", [-7.62, 5.08], Orientation::Right, 1, 0);
    b.package_length = Some(1.5);
    b.propagation_delay_ns = Some(0.25);
    let y1 = pin("3", "Y", "Output", [7.62, 7.62], Orientation::Left, 1, 1);
    let mut y1n = pin("3", "Y", "Output", [10.16, 7.62], Orientation::Left, 1, 2);
    y1n.decorations[2] = "Dot".into();
    let a2 = pin("4", "A", "Input", [-7.62, -5.08], Orientation::Right, 2, 0);
    let b2 = pin("5", "B", "Input", [-7.62, -10.16], Orientation::Right, 2, 0);
    let y2 = pin("6", "Y", "Output", [7.62, -7.62], Orientation::Left, 2, 1);
    let mut y2n = pin("6", "Y", "Output", [10.16, -7.62], Orientation::Left, 2, 2);
    y2n.decorations[2] = "Dot".into();
    let mut gnd = pin("7", "GND", "Power", [0.0, -15.24], Orientation::Up, 0, 0);
    gnd.hidden = true;
    let mut vcc = pin("14", "VCC", "Power", [0.0, 15.24], Orientation::Down, 0, 0);
    vcc.hidden = true;
    nand.pins = vec![a, b, y1, y1n, a2, b2, y2, y2n, gnd, vcc];
    let fill = Some([255, 255, 194, 255]);
    nand.graphics = vec![
        graphic(
            1,
            1,
            fill,
            GraphicKind::Rectangle {
                from: [-5.08, 12.7],
                to: [5.08, 2.54],
            },
        ),
        graphic(
            1,
            2,
            None,
            GraphicKind::Circle {
                center: [0.0, 7.62],
                radius: 5.08,
            },
        ),
        graphic(
            2,
            1,
            fill,
            GraphicKind::Rectangle {
                from: [-5.08, -2.54],
                to: [5.08, -12.7],
            },
        ),
        graphic(
            2,
            2,
            None,
            GraphicKind::Circle {
                center: [0.0, -7.62],
                radius: 5.08,
            },
        ),
        graphic(
            0,
            0,
            None,
            GraphicKind::Text {
                position: [0.0, 0.0],
                content: "&".into(),
                size: 1.27,
            },
        ),
    ];
    nand.params = vec![("logic_family".into(), "HC".into())];

    let file = SymbolFile {
        file_uuid: "01940000-0000-7000-8000-000000000010".into(),
        display_name: "Conformance Symbols".into(),
        created: CREATED.into(),
        updated: UPDATED.into(),
        symbols: vec![resistor, nand],
    };
    assert_eq!(write_symbol_file(&file), want);
}

fn smd(number: &str, x: f64, y: f64) -> fpt::Pad {
    fpt::Pad {
        number: number.into(),
        kind: "Smd".into(),
        shape: "rect".into(),
        size: [0.8, 0.95],
        pos: [x, y],
        rotation: 0.0,
        layers: vec!["Top Layer".into(), "Top Solder".into(), "Top Paste".into()],
        drill: None,
        slot_length: None,
        mask_margin: Some(0.05),
        paste_margin: None,
    }
}

fn footprint(uuid: &str, name: &str, description: &str, designator: &str) -> fpt::Footprint {
    fpt::Footprint {
        uuid: uuid.into(),
        name: name.into(),
        pads: vec![],
        silk_f: vec![],
        silk_b: vec![],
        fab_f: vec![],
        fab_b: vec![],
        version: "1.0.0".into(),
        released: false,
        created: CREATED.into(),
        updated: UPDATED.into(),
        description: description.into(),
        default_designator: designator.into(),
        component_type: None,
        height_mm: None,
        courtyard: None,
        body: fpt::Body3d::default(),
    }
}

fn stroke(width: f64, filled: bool, kind: fpt::GraphicKind) -> fpt::Graphic {
    fpt::Graphic {
        stroke_width: width,
        filled,
        kind,
    }
}

#[test]
fn a_footprint_with_lines_matches_the_fixture_byte_for_byte() {
    let Some(want) = fixture("footprints/01940000-0000-7000-8000-000000000021.snxfpt") else {
        return;
    };
    let mut f = footprint(
        "01940000-0000-7000-8000-000000000021",
        "R_0603",
        "Chip resistor, 0603 (1608 metric)",
        "R?",
    );
    f.pads = vec![smd("1", -0.8, 0.0), smd("2", 0.8, 0.0)];
    f.courtyard = Some(vec![[-1.5, -0.75], [1.5, -0.75], [1.5, 0.75], [-1.5, 0.75]]);
    f.silk_f = vec![
        stroke(
            0.12,
            false,
            fpt::GraphicKind::Line {
                from: [-0.2, 0.6],
                to: [0.2, 0.6],
            },
        ),
        stroke(
            0.12,
            false,
            fpt::GraphicKind::Line {
                from: [-0.2, -0.6],
                to: [0.2, -0.6],
            },
        ),
    ];
    f.fab_f = vec![stroke(
        0.1,
        false,
        fpt::GraphicKind::Rectangle {
            from: [-0.8, -0.4],
            to: [0.8, 0.4],
        },
    )];
    f.height_mm = Some(0.55);
    f.body.height_mm = 0.55;
    let file = FootprintFile {
        file_uuid: "01940000-0000-7000-8000-000000000031".into(),
        display_name: "R_0603".into(),
        created: CREATED.into(),
        updated: UPDATED.into(),
        footprint: f,
    };
    assert_eq!(write_footprint_file(&file), want);
}

#[test]
fn a_footprint_with_a_pin_one_marker_matches_the_fixture_byte_for_byte() {
    let Some(want) = fixture("footprints/01940000-0000-7000-8000-000000000023.snxfpt") else {
        return;
    };
    let mut f = footprint(
        "01940000-0000-7000-8000-000000000023",
        "SOT-23",
        "SOT-23, pin 1 at bottom left",
        "Q?",
    );
    let pad = |n: &str, x: f64, y: f64| fpt::Pad {
        size: [0.6, 0.7],
        ..smd(n, x, y)
    };
    f.pads = vec![
        pad("1", -0.95, 1.1),
        pad("2", 0.95, 1.1),
        pad("3", 0.0, -1.1),
    ];
    f.courtyard = Some(vec![[-1.7, -1.75], [1.7, -1.75], [1.7, 1.75], [-1.7, 1.75]]);
    f.silk_f = vec![stroke(
        0.12,
        true,
        fpt::GraphicKind::Circle {
            center: [-1.45, 1.6],
            radius: 0.1,
        },
    )];
    f.body.height_mm = 1.0;
    let file = FootprintFile {
        file_uuid: "01940000-0000-7000-8000-000000000033".into(),
        display_name: "SOT-23".into(),
        created: CREATED.into(),
        updated: UPDATED.into(),
        footprint: f,
    };
    assert_eq!(write_footprint_file(&file), want);
}

#[test]
fn the_manifest_head_matches_the_fixture_byte_for_byte() {
    let Some(want) = fixture("Conformance.snxlib") else {
        return;
    };
    // The fixture goes on with a user, classes and tables; a manifest
    // without them is its text up to the first of those.
    let head = &want[..want.find("\n[users.\"").expect("a user table")];
    let got = write_manifest(
        "01940000-0000-7000-8000-000000000001",
        "Conformance",
        Some("Signex library format conformance fixture (docs/LIBRARY_FORMAT.md)."),
    );
    assert_eq!(got.trim_end_matches('\n'), head.trim_end_matches('\n'));
}
