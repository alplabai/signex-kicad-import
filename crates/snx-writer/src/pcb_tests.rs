// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

use uuid::Uuid;

use super::*;
use crate::model::{Net, PadType, Setup, StackLayer, Track, Via, ViaType, Zone, ZoneKind};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn sample() -> Board {
    let pad = |n: u128, shape: PadShape, rotation: f64| Pad {
        uuid: id(0x100 + n),
        number: n.to_string(),
        pad_type: PadType::Smd,
        shape,
        position: Point::new(n as f64, 0.0),
        size: Point::new(1.0, 0.5),
        rotation,
        drill: None,
        drill_shape: String::new(),
        layers: vec!["TopCopper".into(), "TopPaste".into(), "TopSolderMask".into()],
        net: Some(Net {
            number: 1,
            name: "GND".into(),
        }),
    };
    let square = vec![
        Point::new(-0.5, -0.5),
        Point::new(0.5, -0.5),
        Point::new(0.5, 0.5),
        Point::new(-0.5, 0.5),
    ];
    Board {
        uuid: id(1),
        thickness: 1.6,
        layers: vec![
            StackLayer {
                token: "TopCopper".into(),
                layer_type: "copper".into(),
            },
            StackLayer {
                token: "BottomCopper".into(),
                layer_type: "copper".into(),
            },
            StackLayer {
                token: "EdgeCuts".into(),
                layer_type: String::new(),
            },
        ],
        setup: Setup::default(),
        nets: vec![Net {
            number: 1,
            name: "GND".into(),
        }],
        footprints: vec![Footprint {
            uuid: id(0x10),
            reference: "U 1".into(),
            value: "".into(),
            footprint_id: "Lib:Part".into(),
            position: Point::new(10.0, 20.0),
            rotation: 270.0,
            layer: "BottomCopper".into(),
            locked: true,
            pads: vec![
                pad(1, PadShape::RoundRect(0.25), 0.0),
                pad(2, PadShape::Custom(square), 90.0),
            ],
            graphics: vec![FootprintGraphic {
                graphic_type: "line".into(),
                layer: "BottomSilkscreen".into(),
                width: 0.12,
                start: Some(Point::new(0.0, 0.0)),
                end: Some(Point::new(1.0, 0.0)),
                ..FootprintGraphic::default()
            }],
            properties: vec![Property {
                key: "Reference".into(),
                value: "U 1".into(),
                position: Some(Point::new(0.0, -2.0)),
                layer: Some("BottomSilkscreen".into()),
                font_size: Some(1.0),
                ..Property::default()
            }],
        }],
        tracks: vec![Track {
            uuid: id(0x200),
            net: 1,
            layer: "TopCopper".into(),
            width: 0.25,
            start: Point::new(0.0, 0.0),
            end: Point::new(1.5, 0.0000004),
        }],
        arcs: vec![TrackArc {
            uuid: id(0x300),
            net: 1,
            layer: "BottomCopper".into(),
            width: 0.2,
            start: Point::new(0.0, 0.0),
            mid: Point::new(1.0, 1.0),
            end: Point::new(2.0, 0.0),
        }],
        vias: vec![Via {
            uuid: id(0x400),
            net: 1,
            position: Point::new(3.0, 3.0),
            drill: 0.3,
            diameter: 0.6,
            layers: vec!["TopCopper".into(), "BottomCopper".into()],
            via_type: ViaType::Through,
        }],
        zones: vec![Zone {
            uuid: id(0x500),
            kind: ZoneKind::Pour,
            net: 1,
            net_name: "GND".into(),
            layer: "BottomCopper".into(),
            outline: vec![
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(5.0, 5.0),
            ],
            priority: 0,
            thermal_relief: true,
            thermal_gap: 0.5,
            thermal_width: 0.5,
            clearance: 0.2,
            min_thickness: 0.25,
            remove_dead_islands: true,
            min_island_area: 0.0,
        }],
        outline: vec![
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 40.0),
            Point::new(0.0, 40.0),
        ],
        graphics: Vec::new(),
        texts: Vec::new(),
    }
}

fn parsed() -> (String, toml::Value) {
    let text = write_board(&sample());
    let value = toml::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    (text, value)
}

fn rows(value: &toml::Value, table: &str) -> Vec<Vec<String>> {
    value[table]["content"]
        .as_str()
        .expect("a content string")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(str::to_string).collect())
        .collect()
}

#[test]
fn the_header_names_the_current_format_first() {
    let (text, value) = parsed();
    assert!(text.starts_with("format = \"snxpcb/1.4\"\n"), "{text}");
    assert_eq!(value["pcb_id"].as_str(), Some(id(1).to_string().as_str()));
    assert_eq!(value["generator"].as_str(), Some(GENERATOR));
    assert_eq!(value["thickness"].as_float(), Some(1.6));
}

#[test]
fn the_stackup_lists_tokens_and_types_in_parallel() {
    let (_, value) = parsed();
    let layers = value["stackup"]["layers"].as_array().expect("layers");
    let types = value["stackup"]["layer_types"].as_array().expect("types");
    assert_eq!(layers.len(), types.len());
    assert_eq!(layers[0].as_str(), Some("TopCopper"));
    assert_eq!(types[2].as_str(), Some(""));
    assert_eq!(value["stackup"]["setup"]["clearance"].as_float(), Some(0.2));
}

#[test]
fn pads_key_to_their_footprint_uuid_and_carry_optional_columns() {
    let (_, value) = parsed();
    let pads = rows(&value, "pads");
    let header = &pads[0];
    assert_eq!(header.last().map(String::as_str), Some("custom_outline"));
    assert!(header.contains(&"rotation".to_string()));
    for row in &pads[1..] {
        assert_eq!(row.len(), header.len(), "{row:?}");
        assert_eq!(row[1], id(0x10).to_string());
    }
    assert_eq!(pads[1][8], "roundrect");
    assert_eq!(pads[1][13], "0.25");
    assert_eq!(pads[2][8], "custom");
    assert_eq!(
        pads[2][15],
        "-500000,-500000;500000,-500000;500000,500000;-500000,500000"
    );
}

#[test]
fn a_footprint_row_quotes_a_designator_with_a_space() {
    let (_, value) = parsed();
    let content = value["footprints"]["content"].as_str().expect("content");
    let row = content.lines().nth(1).expect("one row");
    assert!(row.contains("\"U 1\""), "{row}");
    assert!(row.contains("BottomCopper"), "{row}");
    assert!(row.contains(" 270 "), "{row}");
}

#[test]
fn track_lengths_are_whole_nanometres() {
    let (_, value) = parsed();
    let tracks = rows(&value, "tracks");
    assert_eq!(tracks[1][3], "250000");
    assert_eq!(tracks[1][6], "1500000");
    assert_eq!(tracks[1][7], "0");
}

#[test]
fn zones_arcs_and_extras_land_in_their_tables() {
    let (_, value) = parsed();
    let zone = &value["zones"][0];
    assert_eq!(zone["fill_type"].as_str(), Some("pour"));
    assert_eq!(zone["outline"].as_array().map(Vec::len), Some(3));
    let arc = &value["free"]["arcs"][0];
    assert_eq!(arc["mid_y"].as_integer(), Some(1_000_000));
    let fp = &value["extras"]["footprints"][id(0x10).to_string().as_str()];
    assert_eq!(fp["footprint_id"].as_str(), Some("Lib:Part"));
    assert_eq!(fp["locked"].as_bool(), Some(true));
    assert_eq!(fp["graphics"][0]["end"]["x"].as_float(), Some(1.0));
    assert_eq!(fp["properties"][0]["value"].as_str(), Some("U 1"));
    let outline = value["extras"]["board"]["outline"].as_array().expect("outline");
    assert_eq!(outline.len(), 4);
}
