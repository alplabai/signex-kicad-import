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
        step: None,
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
        models: vec![],
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
    let file = FootprintFile {
        file_uuid: "01940000-0000-7000-8000-000000000033".into(),
        display_name: "SOT-23".into(),
        created: CREATED.into(),
        updated: UPDATED.into(),
        footprint: f,
        models: vec![],
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
        &snx_writer::LibraryMeta {
            description: Some(
                "Signex library format conformance fixture (docs/LIBRARY_FORMAT.md).".into(),
            ),
            ..Default::default()
        },
        &[],
        &[],
    );
    // The fixture has classes; with none the writer adds the root key `classes = []`.
    let got = got.replacen("classes = []\n", "", 1);
    assert_eq!(got.trim_end_matches('\n'), head.trim_end_matches('\n'));
}

mod parts {
    use snx_writer::part_table::{
        AlternateSymbol, Class, Document, FootprintBinding, LabelledDocument, Mpn, Param, PartRow,
        Ref, Supply, Table, hash_document, write_classes, write_table,
    };

    use super::fixture;

    const LIB: &str = "01940000-0000-7000-8000-000000000001";

    fn r(n: u8) -> Ref {
        Ref {
            library_id: LIB.into(),
            uuid: format!("01940000-0000-7000-8000-0000000000{n:02x}"),
        }
    }

    fn mpn(manufacturer: &str, mpn: &str) -> Mpn {
        Mpn {
            manufacturer: manufacturer.into(),
            mpn: mpn.into(),
            status: "Primary".into(),
            notes: None,
        }
    }

    fn row(id: u8, pn: &str, class: &str) -> PartRow {
        PartRow {
            row_id: format!("01940000-0000-7000-8000-0000000000{id:02x}"),
            internal_pn: pn.into(),
            class: class.into(),
            datasheets: vec![],
            state: "Draft".into(),
            symbol: r(0x11),
            footprint: None,
            alternate_footprints: vec![],
            sim: None,
            primary_mpn: mpn("Example Corp", "X"),
            alternates: vec![],
            supply: vec![],
            parameters: vec![],
            pin_map_overrides: vec![],
            created: "2026-01-01T00:00:00+00:00".into(),
            updated: "2026-01-02T03:04:05+00:00".into(),
            version: "1.0.0".into(),
            released: false,
            symbol_version: "1.0.0".into(),
            footprint_version: String::new(),
            sim_version: String::new(),
            description: String::new(),
            alternate_symbols: vec![],
        }
    }

    fn rows() -> (PartRow, PartRow, PartRow) {
        let mut a = row(0x41, "R-10K-0603", "resistor");
        a.datasheets = vec![LabelledDocument {
            label: String::new(),
            document: Document::Url("https://example.com/datasheets/r0603.pdf".into()),
        }];
        a.state = "Released".into();
        a.footprint = Some(r(0x21));
        a.alternate_footprints = vec![FootprintBinding {
            footprint: r(0x22),
            version: "1.0.0".into(),
            pin_map_overrides: vec![],
        }];
        a.primary_mpn = mpn("Example Corp", "EX-R0603-10K");
        a.alternates = vec![Mpn {
            manufacturer: "Sample Inc".into(),
            mpn: "SI-10K-0603".into(),
            status: "Approved".into(),
            notes: Some("drop-in".into()),
        }];
        a.supply = vec![Supply {
            distributor: "ExampleDistributor".into(),
            sku: "ED-123".into(),
            url: Some("https://example.com/p/ED-123".into()),
            moq: Some(100),
        }];
        a.parameters = vec![
            ("tolerance".into(), Param::Text("1%".into())),
            (
                "resistance".into(),
                Param::Measurement {
                    value: 10000.0,
                    unit: "ohm".into(),
                },
            ),
            (
                "power".into(),
                Param::Measurement {
                    value: 0.1,
                    unit: "W".into(),
                },
            ),
        ];
        a.released = true;
        a.footprint_version = "1.0.0".into();
        a.description = "10 kΩ 1% 0.1 W 0603".into();
        a.alternate_symbols = vec![AlternateSymbol {
            symbol: r(0x12),
            symbol_version: "1.0.0".into(),
            label: "Alternate drawing".into(),
        }];

        let mut b = row(0x42, "R-1K-0805", "resistor");
        b.datasheets = vec![
            LabelledDocument {
                label: "Datasheet".into(),
                document: Document::Url("https://example.com/datasheets/r0805.pdf".into()),
            },
            LabelledDocument {
                label: "Errata".into(),
                document: Document::HashPinned {
                    hash: "0d0838668315bb2a467097ac6a308b2241c798498df919291e9a42ae13240b5b".into(),
                    filename: "errata.pdf".into(),
                },
            },
        ];
        b.footprint = Some(r(0x22));
        b.primary_mpn = mpn("Example Corp", "EX-R0805-1K");
        b.parameters = vec![
            ("pad_count".into(), Param::Number(2.0)),
            ("aec_q200".into(), Param::Bool(true)),
        ];
        b.pin_map_overrides = vec![("1".into(), "2".into()), ("2".into(), "1".into())];
        b.version = "0.0.1".into();
        b.footprint_version = "1.0.0".into();

        let mut c = row(0x43, "U-NAND-DUAL", "ic");
        c.state = "InReview".into();
        c.symbol = r(0x12);
        c.primary_mpn = mpn("Example Corp", "EX-74-00");
        c.parameters = vec![
            (
                "supply_voltage".into(),
                Param::Measurement {
                    value: 5.0,
                    unit: "V".into(),
                },
            ),
            ("designator_prefix".into(), Param::Text("U".into())),
        ];
        c.description = "Dual NAND, symbol only".into();
        (a, b, c)
    }

    #[test]
    fn the_classes_and_part_tables_match_the_fixture_byte_for_byte() {
        let Some(want) = fixture("Conformance.snxlib") else {
            return;
        };
        let (a, b, c) = rows();
        let classes = [
            Class {
                key: "resistor".into(),
                label: "Resistor".into(),
            },
            Class {
                key: "ic".into(),
                label: "Integrated Circuit".into(),
            },
        ];
        let got = format!(
            "{}{}{}",
            write_classes(&classes),
            write_table(&Table {
                name: "logic".into(),
                rows: vec![c],
                kept: vec![],
                raw_tsv: None
            }),
            write_table(&Table {
                name: "resistors".into(),
                rows: vec![b, a],
                kept: vec![],
                raw_tsv: None
            })
        );
        let from = want.find("\n[[classes]]").expect("classes");
        let to = want
            .find("\n[tables.resistors.column_types]")
            .expect("sidecar");
        assert_eq!(got, &want[from..to]);
    }

    #[test]
    fn the_hash_documents_match_the_fixture_bytes() {
        let (a, b, c) = rows();
        for (id, row) in [(0x41, a), (0x42, b), (0x43, c)] {
            let name = format!("content-hash/01940000-0000-7000-8000-0000000000{id:02x}.json");
            // `fixture` reads below `library/`; the hash documents sit beside it.
            let Some(dir) = std::env::var_os("SIGNEX_FORMAT_FIXTURES") else {
                eprintln!("skipped: SIGNEX_FORMAT_FIXTURES is not set");
                return;
            };
            let want = std::fs::read_to_string(std::path::Path::new(&dir).join(&name))
                .expect("hash document");
            assert_eq!(hash_document(&row), want, "{name}");
        }
    }
}

#[test]
fn an_embedded_model_inflates_to_the_fixtures_step_file() {
    use base64::Engine as _;
    use std::io::Read as _;
    let Some(dir) = std::env::var_os("SIGNEX_FORMAT_FIXTURES") else {
        eprintln!("skipped: SIGNEX_FORMAT_FIXTURES is not set");
        return;
    };
    let Some(want) = fixture("footprints/01940000-0000-7000-8000-000000000022.snxfpt") else {
        return;
    };
    let hash = "25bdbce8f3930e778844aff9e6deb9dd4aa5f01315979d7f7eebc68bbc301313";
    let step = std::fs::read(std::path::Path::new(&dir).join(format!("library/step/{hash}.step")))
        .expect("step file");
    let mut f = footprint(
        "01940000-0000-7000-8000-000000000022",
        "R_0805",
        "Chip resistor, 0805 (2012 metric)",
        "R?",
    );
    f.step = Some(fpt::StepAttachment {
        content_hash: hash.into(),
        filename: "R_0805.step".into(),
        offset_xyz: [0.0, 0.0, 0.0],
        rotation_xyz: [0.0, 0.0, 90.0],
    });
    let file = FootprintFile {
        file_uuid: "01940000-0000-7000-8000-000000000032".into(),
        display_name: "R_0805".into(),
        created: CREATED.into(),
        updated: UPDATED.into(),
        footprint: f,
        models: vec![fpt::Model {
            hash: hash.into(),
            filename: "R_0805.step".into(),
            bytes: step.clone().into(),
        }],
    };
    let got = write_footprint_file(&file);
    assert!(got.starts_with("format = \"snxfpt/1.4\"\n"));
    // The attachment block is the fixture's, byte for byte.
    let block = |t: &str| {
        let a = t.find("[footprints.step_attachment]").expect("attachment");
        t[a..t.find("[footprints.pcb_params]").expect("params")].to_string()
    };
    assert_eq!(block(&got), block(&want));
    // The model entry is last, valid TOML, and inflates to the same bytes.
    let value: toml::Value = toml::from_str(&got).expect("valid TOML");
    let m = &value["models"][0];
    assert_eq!(m["hash"].as_str(), Some(hash));
    assert_eq!(m["size"].as_integer(), Some(step.len() as i64));
    assert_eq!(m["encoding"].as_str(), Some("deflate+base64"));
    let data: String = m["data"]
        .as_str()
        .expect("data")
        .split_whitespace()
        .collect();
    let packed = base64::engine::general_purpose::STANDARD
        .decode(data)
        .expect("base64");
    let mut raw = Vec::new();
    flate2::read::DeflateDecoder::new(&packed[..])
        .read_to_end(&mut raw)
        .expect("inflates");
    assert_eq!(raw, step);
    let body = got.split("data = '''\n").nth(1).expect("data block");
    assert!(body.lines().all(|l| l.len() <= 76 || l == "'''"));
    assert!(got.ends_with("'''\n"));
    // Without a model the file keeps the older token.
    let plain = FootprintFile {
        models: vec![],
        ..file
    };
    assert!(write_footprint_file(&plain).starts_with("format = \"snxfpt/1.1\"\n"));
}

#[test]
fn an_empty_courtyard_is_written_and_a_library_without_classes_has_the_root_key() {
    let f = footprint("01940000-0000-7000-8000-000000000021", "E", "", "");
    assert!(f.courtyard.is_none());
    let file = FootprintFile {
        file_uuid: "01940000-0000-7000-8000-000000000031".into(),
        display_name: "E".into(),
        created: CREATED.into(),
        updated: UPDATED.into(),
        footprint: f,
        models: vec![],
    };
    let text = write_footprint_file(&file);
    assert!(
        text.contains("\n[footprints.courtyard]\npoints = []\n\n[footprints.body_3d]\n"),
        "{text}"
    );
    toml::from_str::<toml::Value>(&text).expect("valid TOML");
    // A non-empty one is as before (the fixtures compare byte for byte above).
    let m = write_manifest("01940000-0000-7000-8000-000000000001", "N", &Default::default(), &[], &[]);
    let lines: Vec<&str> = m.lines().collect();
    assert_eq!(lines[2], "classes = []");
    toml::from_str::<toml::Value>(&m).expect("valid TOML");
    let with = write_manifest(
        "01940000-0000-7000-8000-000000000001",
        "N",
        &Default::default(),
        &[snx_writer::part_table::Class { key: "k".into(), label: "K".into() }],
        &[],
    );
    assert!(!with.contains("classes = []") && with.contains("[[classes]]"));
}
