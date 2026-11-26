// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The library footprint converter against the hand-written fixtures in
//! `tests/fixtures/footprints`.

use std::path::{Path, PathBuf};

use snx_writer::footprint_file::GraphicKind;
use snx_writer::write_footprint_file;

use super::*;
use crate::models::ModelCx;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/footprints/Test.pretty")
}

fn ids(is_file: bool, name: &str) -> String {
    format!("{}-{name}", if is_file { "file" } else { "fp" })
}

fn read() -> FootprintLibRead {
    read_pretty_dir(&dir(), &ids, &ModelCx::default()).expect("folder reads")
}

fn by_name<'a>(read: &'a FootprintLibRead, name: &str) -> &'a out::Footprint {
    &read
        .files
        .iter()
        .find(|f| f.footprint.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
        .footprint
}

fn codes(read: &FootprintLibRead, source: &str) -> Vec<String> {
    read.items
        .iter()
        .filter(|i| i.source == source)
        .map(|i| i.code.clone())
        .collect()
}

#[test]
fn a_folder_reads_each_file_as_one_footprint() {
    let read = read();
    let mut names: Vec<&str> = read
        .files
        .iter()
        .map(|f| f.footprint.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["Conn_Test_2", "R_Test_0603"]);
    assert_eq!(read.found, 3, "the truncated file counts as found");
    let f = read
        .files
        .iter()
        .find(|f| f.footprint.name == "R_Test_0603")
        .expect("file");
    assert_eq!(f.footprint.uuid, "fp-R_Test_0603");
    assert_eq!(f.file_uuid, "file-R_Test_0603");
    assert_eq!(f.display_name, "R_Test_0603");
    assert!(
        read.items
            .iter()
            .any(|i| i.code == "footprint.unreadable" && i.severity == Severity::Error)
    );
    assert!(
        read.items
            .iter()
            .any(|i| i.code == "footprint.file_ignored")
    );
}

#[test]
fn pads_follow_the_spec_layer_sets_and_shape_tokens() {
    let read = read();
    let r = by_name(&read, "R_Test_0603");
    assert_eq!(r.pads.len(), 2);
    let p = &r.pads[0];
    assert_eq!(
        (p.kind.as_str(), p.shape.as_str()),
        ("Smd", "round_rect:0.25")
    );
    assert_eq!(p.layers.join("|"), "Top Layer|Top Solder|Top Paste");
    assert_eq!((p.pos, p.size), ([-0.8, 0.0], [0.8, 0.95]));
    assert_eq!((p.mask_margin, p.paste_margin), (Some(0.05), None));
    assert_eq!(r.pads[1].paste_margin, Some(-0.02));

    let c = by_name(&read, "Conn_Test_2");
    let layers = |i: usize| c.pads[i].layers.join("|");
    assert_eq!(c.pads[0].kind, "Tht");
    assert_eq!(layers(0), "Top Layer|Bottom Layer|Top Solder|Bottom Solder");
    assert_eq!(
        (c.pads[0].shape.as_str(), c.pads[0].drill),
        ("rect", Some(1.0))
    );
    // A 90 degree pad (counter-clockwise on screen in the source) is 270 in
    // the footprint frame, whose angles run from +x towards +y.
    assert_eq!(
        (c.pads[1].shape.as_str(), c.pads[1].rotation, c.pads[1].pos),
        ("oval", 270.0, [2.54, 0.0])
    );
    assert_eq!(
        (c.pads[2].kind.as_str(), layers(2).as_str()),
        ("NptHole", "Top Solder|Bottom Solder")
    );
    assert!(codes(&read, "Test:Conn_Test_2").contains(&"pad.npth_copper_dropped".to_string()));
}

#[test]
fn drawings_land_on_their_lists_and_the_courtyard_closes() {
    let read = read();
    let r = by_name(&read, "R_Test_0603");
    assert_eq!(r.silk_f.len(), 2);
    assert!(matches!(
        r.silk_f[0].kind,
        GraphicKind::Line {
            from: [-0.2, -0.6],
            to: [0.2, -0.6]
        }
    ));
    assert_eq!(r.silk_f[0].stroke_width, 0.12);
    assert_eq!(r.fab_f.len(), 1);
    // Four lines end to end are one polygon, no approximation.
    assert_eq!(
        r.courtyard.as_deref(),
        Some(&[[-1.5, -0.75], [1.5, -0.75], [1.5, 0.75], [-1.5, 0.75]][..])
    );
    assert!(
        !codes(&read, "Test:R_Test_0603").contains(&"footprint.courtyard_approximated".to_string())
    );
    // A drawing on a layer the library footprint does not keep is reported.
    let layer_item = read
        .items
        .iter()
        .find(|i| i.code == "footprint.graphic_layer_dropped")
        .expect("item");
    assert_eq!(layer_item.severity, Severity::Dropped);

    let c = by_name(&read, "Conn_Test_2");
    assert!(
        matches!(&c.silk_f[0].kind, GraphicKind::Circle { radius, .. } if (radius - 0.1).abs() < 1e-12)
    );
    assert!(c.silk_f[0].filled);
    // The arc runs from +x towards +y: 0 to 90 degrees, y down as in the source.
    let GraphicKind::Arc {
        start_deg,
        end_deg,
        radius,
        center,
    } = c.fab_f[0].kind
    else {
        panic!("arc")
    };
    // The fixture's mid point is rounded to four digits.
    assert!(
        start_deg.abs() < 0.01 && (end_deg - 90.0).abs() < 0.01,
        "{start_deg} {end_deg}"
    );
    assert!((radius - 1.0).abs() < 1e-3 && center[0].abs() < 1e-3 && center[1].abs() < 1e-3);
    assert!(matches!(&c.fab_f[1].kind, GraphicKind::Text { content, .. } if content == "X"));
    assert_eq!(c.courtyard.as_ref().map(Vec::len), Some(4));
}

#[test]
fn description_tags_attr_and_designator_are_carried() {
    let read = read();
    let r = by_name(&read, "R_Test_0603");
    assert_eq!(
        r.description,
        "Resistor SMD 0603, test fixture (tags: resistor test)"
    );
    assert_eq!(r.default_designator, "", "REF** names no prefix");
    assert_eq!(r.component_type, None);
    let c = by_name(&read, "Conn_Test_2");
    assert_eq!(c.default_designator, "J?");
    assert_eq!(c.component_type.as_deref(), Some("StandardNoBom"));
    assert!(
        codes(&read, "Test:R_Test_0603").contains(&"footprint.tags_in_description".to_string())
    );
}

#[test]
fn models_zones_and_board_messages_are_reported() {
    let read = read();
    // With no models folder the variable is unset and the model is not found.
    let model = read
        .items
        .iter()
        .find(|i| i.code == "model.not_found")
        .expect("model item");
    assert_eq!(
        (model.kind, model.severity),
        (ItemKind::Model, Severity::Dropped)
    );
    assert!(
        model.detail["path"]
            .as_str()
            .expect("path")
            .ends_with("R_0603.step")
    );
    assert!(
        !read
            .items
            .iter()
            .any(|i| i.code == "footprint.model_dropped")
    );
    let zone = read
        .items
        .iter()
        .find(|i| i.code == "footprint.zone_dropped")
        .expect("zone item");
    assert_eq!(zone.severity, Severity::Dropped);
}

#[test]
fn the_envelope_is_valid_toml_and_deterministic() {
    let read = read();
    for f in &read.files {
        let text = write_footprint_file(f);
        let value: toml::Value =
            toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", f.footprint.name));
        assert_eq!(value["format"].as_str(), Some("snxfpt/1.1"));
        assert_eq!(value["footprints"].as_array().map(Vec::len), Some(1));
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"));
    }
    let again = self::read();
    assert_eq!(read, again);
}

#[test]
fn damaged_input_is_an_item_and_never_a_panic() {
    let whole = std::fs::read_to_string(dir().join("Conn_Test_2.kicad_mod")).expect("fixture");
    let mut cut = 0;
    while cut < whole.len() {
        if whole.is_char_boundary(cut) {
            let mut items = Vec::new();
            let _ = convert_footprint(
                &whole[..cut],
                "Test",
                &ids,
                &mut items,
                &ModelCx::default(),
                Path::new("."),
            );
        }
        cut += 11;
    }
    for garbage in [
        "",
        "(",
        "(footprint)",
        "(footprint \"A\" (pad))",
        "(symbol \"A\")",
        "\u{0}\u{1}",
    ] {
        let mut items = Vec::new();
        let _ = convert_footprint(
            garbage,
            "Test",
            &ids,
            &mut items,
            &ModelCx::default(),
            Path::new("."),
        );
    }
}

fn special(name: &str) -> (Result<FootprintFile, ParseError>, Vec<Item>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/footprints/Special.pretty")
        .join(name);
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut items = Vec::new();
    let result = convert_footprint(
        &text,
        "Special",
        &ids,
        &mut items,
        &ModelCx::default(),
        Path::new("."),
    );
    (result, items)
}

fn item_codes(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.code.as_str()).collect()
}

#[test]
fn fiducial_castellated_and_layer_sets_follow_the_spec_table() {
    let (file, items) = special("Layers_Test.kicad_mod");
    let pads = file.expect("converts").footprint.pads;
    let row = |i: usize| {
        format!(
            "{}|{}|{}",
            pads[i].number,
            pads[i].kind,
            pads[i].layers.join("|")
        )
    };
    assert_eq!(
        row(0),
        "1|Tht|Top Layer|Bottom Layer|Top Solder|Bottom Solder"
    );
    assert_eq!(row(1), "2|Smd|Bottom Layer|Bottom Solder|Bottom Paste");
    assert_eq!(row(2), "FID1|Fiducial|Top Layer|Top Solder");
    assert_eq!(row(3), "FID2|Fiducial|Bottom Layer|Bottom Solder");
    assert_eq!(
        row(4),
        "C1|Castellated|Top Layer|Bottom Layer|Top Solder|Bottom Solder"
    );
    assert_eq!(pads[5].kind, "Smd");
    assert!(item_codes(&items).contains(&"pad.property_not_carried"));
}

#[test]
fn an_oval_drill_is_a_slot_of_the_larger_size() {
    let (file, items) = special("Slot_Test.kicad_mod");
    let pads = file.expect("converts").footprint.pads;
    assert_eq!((pads[0].drill, pads[0].slot_length), (Some(0.8), Some(1.6)));
    // A slot longer in y: the pad is swapped and turned a quarter, exactly.
    assert_eq!((pads[0].size, pads[0].rotation), ([1.5, 2.5], 90.0));
    assert_eq!((pads[1].drill, pads[1].slot_length), (Some(0.8), Some(1.6)));
    assert_eq!((pads[1].size, pads[1].rotation), ([2.5, 1.5], 0.0));
    // Equal sizes are a round hole.
    assert_eq!((pads[2].drill, pads[2].slot_length), (Some(0.8), None));
    // Exact for these shapes: no orientation item; one not-placeable item per footprint.
    assert!(!item_codes(&items).contains(&"pad.slot_orientation_approximated"));
    let item = items
        .iter()
        .find(|i| i.code == "pad.slot_not_placeable")
        .expect("item");
    assert_eq!(item.severity, Severity::Approximated);
    assert_eq!(
        item.message,
        "Signex cannot place this footprint on a board until slotted drills are supported (signex #3128); the footprint is written exactly"
    );
    assert_eq!(
        item_codes(&items)
            .iter()
            .filter(|c| **c == "pad.slot_not_placeable")
            .count(),
        1
    );
    assert!(!item_codes(&items).contains(&"pad.slot_approximated"));
}

#[test]
fn footprint_level_margins_are_inherited_and_ratios_reported() {
    let (file, items) = special("Margins_Test.kicad_mod");
    let pads = file.expect("converts").footprint.pads;
    assert_eq!(
        (pads[0].mask_margin, pads[0].paste_margin),
        (Some(0.02), Some(-0.03))
    );
    assert_eq!(
        (pads[1].mask_margin, pads[1].paste_margin),
        (Some(0.07), Some(-0.03))
    );
    let ratio = items
        .iter()
        .find(|i| i.code == "pad.paste_ratio_not_carried")
        .expect("item");
    assert_eq!(ratio.detail["count"], 2);
    let lost = items
        .iter()
        .find(|i| i.code == "footprint.graphic_unreadable")
        .expect("item");
    assert_eq!(lost.severity, Severity::Dropped);
}

#[test]
fn a_bottom_side_footprint_is_refused() {
    let (file, _) = special("Bottom_Test.kicad_mod");
    assert!(file.is_err_and(|e| e.to_string().contains("bottom side")));
}

fn models_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/footprints/models")
}

fn model_fp(name: &str, skip: bool) -> (FootprintFile, Vec<Item>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/footprints/Models.pretty");
    let text = std::fs::read_to_string(dir.join(format!("{name}.kicad_mod"))).expect("fixture");
    let cx = ModelCx {
        skip,
        models_dir: Some(models_dir()),
        env: vec![(
            "KICAD8_3DMODEL_DIR".into(),
            models_dir().display().to_string(),
        )],
        ..ModelCx::default()
    };
    let mut items = Vec::new();
    let file = convert_footprint(&text, "Models", &ids, &mut items, &cx, &dir).expect("converts");
    (file, items)
}

fn codes_of(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.code.as_str()).collect()
}

#[test]
fn a_step_model_is_embedded_with_its_hash_and_placement() {
    let (file, items) = model_fp("Good", false);
    assert!(items.iter().all(|i| i.kind != ItemKind::Model), "{items:?}");
    let m = &file.models[0];
    let bytes = std::fs::read(models_dir().join("Good.step")).expect("step");
    assert_eq!(&m.bytes[..], &bytes[..]);
    assert_eq!(m.hash.len(), 64);
    let step = file.footprint.step.clone().expect("attachment");
    assert_eq!(
        (step.content_hash.as_str(), step.filename.as_str()),
        (m.hash.as_str(), "Good.step")
    );
    assert_eq!(
        (step.offset_xyz, step.rotation_xyz),
        ([0.5, -1.0, 2.0], [0.0, 0.0, 90.0])
    );
    let text = write_footprint_file(&file);
    assert!(text.starts_with("format = \"snxfpt/1.4\"") && text.contains("[[models]]"));
    toml::from_str::<toml::Value>(&text).expect("valid TOML");
}

#[test]
fn skip_wrl_scale_and_extra_models_are_reported() {
    let (file, items) = model_fp("Good", true);
    assert!(file.models.is_empty() && file.footprint.step.is_none());
    assert_eq!(
        codes_of(&items)
            .iter()
            .filter(|c| **c == "model.skipped")
            .count(),
        1
    );
    assert!(write_footprint_file(&file).starts_with("format = \"snxfpt/1.1\""));
    let (file, items) = model_fp("Scaled", false);
    assert_eq!(file.models.len(), 1);
    let scale = items
        .iter()
        .find(|i| i.code == "model.scale_not_carried")
        .expect("item");
    assert_eq!(scale.severity, Severity::Approximated);
    let (file, items) = model_fp("WrlOnly", false);
    assert!(file.models.is_empty());
    assert_eq!(
        items
            .iter()
            .find(|i| i.code == "model.format_unsupported")
            .map(|i| i.severity),
        Some(Severity::Dropped)
    );
    // A WRL with a STEP file beside it embeds the STEP file and says so.
    let (file, items) = model_fp("Both", false);
    assert_eq!(file.models[0].filename, "Both.step");
    assert!(codes_of(&items).contains(&"model.step_substituted"));
    let (file, items) = model_fp("Two", false);
    assert_eq!(file.models.len(), 1);
    assert!(codes_of(&items).contains(&"model.extra_dropped"));
}

#[test]
fn a_model_path_cannot_leave_the_folders_it_may_be_read_from() {
    for name in ["Escape", "Absolute", "Linked"] {
        let (file, items) = model_fp(name, false);
        assert!(file.models.is_empty(), "{name}: embedded");
        let model_items: Vec<&Item> = items.iter().filter(|i| i.kind == ItemKind::Model).collect();
        assert_eq!(model_items.len(), 1, "{name}: {items:?}");
        assert_eq!(model_items[0].severity, Severity::Dropped, "{name}");
        // The message shows the path as written, never a resolved one.
        assert!(
            !model_items[0].message.contains("/srv/") && !model_items[0].message.contains("/tmp/"),
            "{name}: {}",
            model_items[0].message
        );
    }
    let (_, items) = model_fp("Escape", false);
    assert!(codes_of(&items).contains(&"model.path_refused"));
    let (_, items) = model_fp("Missing", false);
    let missing = items
        .iter()
        .find(|i| i.code == "model.not_found")
        .expect("item");
    assert!(missing.message.contains("${NOPE}"));
}

#[test]
fn every_pad_and_footprint_setting_is_mapped_or_reported() {
    let (file, items) = special("Overrides_Test.kicad_mod");
    assert!(file.is_ok());
    let by_code = |code: &str| {
        items
            .iter()
            .find(|i| i.code == code)
            .unwrap_or_else(|| panic!("no {code} in {:?}", codes_of(&items)))
    };
    let pad = by_code("pad.settings_not_carried");
    let names: Vec<&str> = pad.detail["settings"]
        .as_object()
        .expect("settings")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        [
            "clearance",
            "die_length",
            "pinfunction",
            "pintype",
            "remove_unused_layers",
            "thermal_bridge_width",
            "thermal_gap",
            "zone_connect"
        ]
    );
    assert_eq!(pad.detail["settings"]["clearance"], 2);
    assert_eq!(pad.severity, Severity::Approximated);
    let fp = by_code("footprint.settings_not_carried");
    let names: Vec<&str> = fp.detail["settings"]
        .as_object()
        .expect("settings")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(names, ["clearance", "net_tie_pad_groups", "zone_connect"]);
    // Each attribute word with no field, the drill offset and the chamfered corners.
    assert_eq!(by_code("footprint.attr_not_carried").detail["count"], 3);
    assert_eq!(
        by_code("pad.drill_offset_ignored").severity,
        Severity::Dropped
    );
   