// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

use super::*;
use crate::sexpr;

fn stack() -> Stack {
    let root = sexpr::parse(r#"(kicad_pcb (layers (0 "F.Cu" signal) (31 "B.Cu" signal)))"#)
        .expect("parses");
    Stack::read(&root, &mut Report::default())
}

fn convert_text(text: &str) -> (Footprint, Report) {
    let node = sexpr::parse(text).expect("parses");
    let stack = stack();
    let mut nets = NetTable::default();
    let mut ids = Ids::seeded(text);
    let mut report = Report::default();
    let mut cx = Context {
        stack: &stack,
        nets: &mut nets,
        ids: &mut ids,
        report: &mut report,
    };
    let fp = convert(&node, &mut cx);
    (fp, report)
}

/// Where Signex puts a footprint-local point: mirror x on the bottom, then
/// the plain rotation matrix, then the footprint position.
fn signex_world(fp: &Footprint, local: Point) -> Point {
    let p = if fp.layer == "BottomCopper" {
        Point::new(-local.x, local.y)
    } else {
        local
    };
    let (s, c) = fp.rotation.to_radians().sin_cos();
    Point::new(
        fp.position.x + p.x * c - p.y * s,
        fp.position.y + p.x * s + p.y * c,
    )
}

/// Where the source puts a placed local point: counter-clockwise on screen
/// with y down.
fn source_world(origin: Point, angle: f64, local: Point) -> Point {
    let (s, c) = angle.to_radians().sin_cos();
    Point::new(
        origin.x + local.x * c + local.y * s,
        origin.y - local.x * s + local.y * c,
    )
}

fn assert_near(a: Point, b: Point) {
    assert!(
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9,
        "{a:?} != {b:?}"
    );
}

/// The angle Signex draws a pad's axes at, in Signex sense.
fn signex_pad_angle(fp: &Footprint, pad: &Pad) -> f64 {
    let own = if fp.layer == "BottomCopper" {
        -pad.rotation
    } else {
        pad.rotation
    };
    normalize_deg(fp.rotation + own)
}

const TOP: &str = r#"(footprint "Lib:R" (layer "F.Cu") (at 10 20 30)
  (property "Reference" "R1" (at 0 -1.5 30) (layer "F.SilkS"))
  (pad "1" smd roundrect (at -0.8 0.4 75) (size 0.9 0.6) (layers "F.Cu" "F.Paste" "F.Mask")
    (roundrect_rratio 0.2) (net 1 "GND"))
  (fp_line (start -1 -0.5) (end 1 -0.5) (stroke (width 0.12)) (layer "F.SilkS")))"#;

const BOTTOM: &str = r#"(footprint "Lib:U" (layer "B.Cu") (at 5 6 120) (locked yes)
  (pad "2" thru_hole circle (at 1.27 -2.54 120) (size 1.7 1.7) (drill 1.0)
    (layers "*.Cu" "*.Mask") (net 2 "VCC"))
  (pad "3" smd custom (at 2 1 200) (size 0.5 0.5) (layers "B.Cu")
    (options (anchor rect))
    (primitives (gr_poly (pts (xy 0 0) (xy 1 0) (xy 1 2)) (width 0) (fill yes)))))"#;

#[test]
fn a_top_pad_lands_where_the_source_puts_it() {
    let (fp, _) = convert_text(TOP);
    assert_eq!(fp.layer, "TopCopper");
    assert_eq!(fp.rotation, 330.0);
    assert_eq!(fp.reference, "R1");
    let pad = &fp.pads[0];
    let expected = source_world(Point::new(10.0, 20.0), 30.0, Point::new(-0.8, 0.4));
    assert_near(signex_world(&fp, pad.position), expected);
    assert!((signex_pad_angle(&fp, pad) - normalize_deg(-75.0)).abs() < 1e-9);
    assert_eq!(pad.shape, PadShape::RoundRect(0.2));
    assert_eq!(pad.layers, ["TopCopper", "TopPaste", "TopSolderMask"]);
}

#[test]
fn a_bottom_pad_lands_where_the_source_puts_it() {
    let (fp, _) = convert_text(BOTTOM);
    assert_eq!(fp.layer, "BottomCopper");
    assert!(fp.locked);
    let pad = &fp.pads[0];
    let expected = source_world(Point::new(5.0, 6.0), 120.0, Point::new(1.27, -2.54));
    assert_near(signex_world(&fp, pad.position), expected);
    assert_eq!(pad.drill, Some(1.0));
    assert_eq!(pad.layers, ["TopCopper", "BottomCopper", "TopSolderMask", "BottomSolderMask"]);
    let custom = &fp.pads[1];
    assert!((signex_pad_angle(&fp, custom) - normalize_deg(-200.0)).abs() < 1e-9);
}

#[test]
fn a_bottom_custom_outline_keeps_its_board_corners() {
    let (fp, _) = convert_text(BOTTOM);
    let pad = &fp.pads[1];
    let PadShape::Custom(corners) = &pad.shape else {
        panic!("a custom pad: {:?}", pad.shape);
    };
    // Signex turns each corner by the pad's own turn about the pad, then
    // places it like any footprint point.
    let (s, c) = pad.rotation.to_radians().sin_cos();
    // The source turns a primitive by the pad's board angle about the pad.
    let world_pad = source_world(Point::new(5.0, 6.0), 120.0, Point::new(2.0, 1.0));
    let source_corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 2.0)];
    assert_eq!(corners.len(), source_corners.len());
    for (corner, (x, y)) in corners.iter().zip(source_corners) {
        let turned = Point::new(
            pad.position.x + corner.x * c - corner.y * s,
            pad.position.y + corner.x * s + corner.y * c,
        );
        let want = source_world(world_pad, 200.0, Point::new(x, y));
        assert_near(signex_world(&fp, turned), want);
    }
}

#[test]
fn a_self_crossing_outline_is_not_simple() {
    let bowtie = [
        Point::new(0.0, 0.0),
        Point::new(2.0, 2.0),
        Point::new(2.0, 0.0),
        Point::new(0.0, 1.0),
    ];
    assert!(!is_simple(&bowtie));
    let square = [
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ];
    assert!(is_simple(&square));
    let folded = [Point::new(0.0, 0.0), Point::new(2.0, 0.0), Point::new(1.0, 0.0)];
    assert!(!is_simple(&folded));
}

#[test]
fn source_tool_bookkeeping_fields_are_dropped() {
    let (fp, report) = convert_text(
        r#"(footprint "Lib:R" (layer "F.Cu") (at 0 0)
  (property "Reference" "R1" (at 0 0 0) (layer "F.SilkS"))
  (property "Sheetfile" "power.kicad_sch")
  (property "Sheetname" "Power")
  (property "ki_fp_filters" "R_*")
  (property "MPN" "RC0603"))"#,
    );
    let keys: Vec<&str> = fp.properties.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, ["Reference", "MPN"]);
    assert!(report.summary().contains("3 x source-tool metadata field dropped"));
}
