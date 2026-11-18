// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

use super::*;
use crate::placement_check::tally_track_ends;

const BOARD: &str = r#"(kicad_pcb (version 20240108) (generator "pcbnew")
  (general (thickness 1.2))
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (37 "F.SilkS" user "F.Silkscreen")
    (44 "Edge.Cuts" user))
  (net 0 "") (net 1 "SIG") (net 2 "GND")
  (net_class "Default" "" (clearance 0.2) (trace_width 0.25) (add_net "SIG"))
  (net_class "Power" "" (clearance 0.3) (trace_width 0.6) (add_net "GND"))
  (footprint "Lib:A" (layer "B.Cu") (at 20 10 90)
    (property "Reference" "J1" (at 0 0 0) (layer "B.SilkS"))
    (pad "1" smd rect (at 1 0 90) (size 1 1) (layers "B.Cu") (net 1 "SIG")))
  (footprint "Lib:B" (layer "F.Cu") (at 30 10)
    (property "Reference" "J2" (at 0 0 0) (layer "F.SilkS"))
    (pad "1" thru_hole circle (at 0 -1) (size 1.5 1.5) (drill 0.8) (layers "*.Cu" "*.Mask")
      (net 1 "SIG")))
  (segment (start 20 9) (end 30 9) (width 0.25) (layer "B.Cu") (net 1))
  (arc (start 30 9) (mid 31 8) (end 32 9) (width 0.2) (layer "F.Cu") (net 1))
  (via (at 25 9) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
  (via blind (at 26 9) (size 0.5) (drill 0.2) (layers "F.Cu" "B.Cu") (net 2))
  (zone (net 2) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.25))
    (min_thickness 0.2) (polygon (pts (xy 0 0) (xy 40 0) (xy 40 20) (xy 0 20))))
  (gr_rect (start 0 0) (end 40 20) (stroke (width 0.1)) (layer "Edge.Cuts"))
  (gr_circle (center 35 15) (end 36 15) (stroke (width 0.1)) (layer "Edge.Cuts"))
  (gr_text "TEST" (at 5 5) (layer "F.SilkS") (effects (font (size 1 1)))))"#;

fn converted() -> Converted {
    convert_pcb(BOARD, None).expect("converts")
}

#[test]
fn a_board_converts_into_signex_layers_and_nets() {
    let c = converted();
    let b = &c.board;
    assert_eq!(b.thickness, 1.2);
    let tokens: Vec<&str> = b.layers.iter().map(|l| l.token.as_str()).collect();
    assert_eq!(tokens, ["TopCopper", "BottomCopper", "TopSilkscreen", "EdgeCuts"]);
    assert_eq!(b.nets.len(), 2);
    assert_eq!(b.footprints[0].layer, "BottomCopper");
    assert_eq!(b.footprints[0].reference, "J1");
    assert_eq!(b.tracks[0].layer, "BottomCopper");
    assert_eq!(b.arcs.len(), 1);
    assert_eq!(b.vias[1].via_type, ViaType::Blind);
    assert_eq!(b.vias[0].layers, ["TopCopper", "BottomCopper"]);
    assert_eq!(b.zones.len(), 1);
    assert_eq!(b.texts[0].text, "TEST");
}

#[test]
fn the_largest_edge_ring_is_the_outline_and_cutouts_stay_drawings() {
    let b = converted().board;
    assert_eq!(b.outline.len(), 4);
    let edges: Vec<&BoardGraphic> = b.graphics.iter().filter(|g| g.layer == "EdgeCuts").collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].graphic_type, "circle");
}

#[test]
fn track_ends_land_on_their_own_pads_once_placed() {
    let tally = tally_track_ends(&converted().board);
    assert_eq!(tally.total, 4);
    assert_eq!(tally.own_net, 3);
    assert_eq!(tally.other_net, 0);
}

#[test]
fn board_classes_put_the_default_last() {
    let c = converted();
    let names: Vec<&str> = c.classes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Power", "Default"]);
    assert_eq!(c.classes[1].patterns, ["*"]);
}

#[test]
fn the_written_board_is_valid_toml_with_no_source_generator() {
    let text = snx_writer::write_board(&converted().board);
    assert!(!text.contains("pcbnew"));
    let value: toml::Value = toml::from_str(&text).expect("valid TOML");
    assert_eq!(value["format"].as_str(), Some("snxpcb/1.4"));
}

#[test]
fn the_same_input_writes_the_same_file() {
    let a = snx_writer::write_board(&converted().board);
    let b = snx_writer::write_board(&converted().board);
    assert_eq!(a, b);
}

#[test]
fn the_board_origin_is_the_aux_origin_else_the_grid_origin() {
    let origin = |setup: &str| {
        let text = format!(r#"(kicad_pcb (layers (0 "F.Cu" signal) (31 "B.Cu" signal)) {setup})"#);
        convert_pcb(&text, None).expect("converts").board.origin
    };
    assert_eq!(
        origin("(setup (aux_axis_origin 10 20) (grid_origin 1 2))"),
        Some(Point::new(10.0, 20.0))
    );
    assert_eq!(
        origin("(setup (aux_axis_origin 0 0) (grid_origin 1 2))"),
        Some(Point::new(1.0, 2.0))
    );
    assert_eq!(origin("(setup (pad_to_mask_clearance 0))"), None);
}

#[test]
fn a_file_that_is_not_a_board_is_refused() {
    assert!(convert_pcb("(kicad_sch (version 1))", None).is_err());
}
