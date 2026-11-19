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
    let fp = convert(&node, &mut cx).expect("a footprint");
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
  (pad "3" smd custom (at 2 1 200) (size 0.2 0.2) (layers "B.Cu")
    (options (anchor rect))
    (primitives (gr_poly (pts (xy -0.5 -0.5) (xy 1 -0.5) (xy 1 2)) (width 0) (fill yes)))))"#;

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
    assert_eq!(
        pad.layers,
        [
            "TopCopper",
            "BottomCopper",
            "TopSolderMask",
            "BottomSolderMask"
        ]
    );
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
    // The anchor lies inside the polygon, so the copper is the polygon; the
    // merge may start the ring at another corner.
    let source_corners = [(-0.5, -0.5), (1.0, -0.5), (1.0, 2.0)];
    assert_eq!(corners.len(), source_corners.len());
    let placed: Vec<Point> = corners
        .iter()
        .map(|corner| {
            let turned = Point::new(
                pad.position.x + corner.x * c - corner.y * s,
                pad.position.y + corner.x * s + corner.y * c,
            );
            signex_world(&fp, turned)
        })
        .collect();
    for (x, y) in source_corners {
        let want = source_world(world_pad, 200.0, Point::new(x, y));
        assert!(
            placed
                .iter()
                .any(|p| (p.x - want.x).abs() < 1e-6 && (p.y - want.y).abs() < 1e-6),
            "{want:?} not in {placed:?}"
        );
    }
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
fn a_custom_pad_keeps_its_anchor_copper_beside_its_polygon() {
    // The anchor rectangle reaches past the polygon: the copper is both.
    let (fp, report) = convert_text(
        r#"(footprint "Lib:J" (layer "F.Cu") (at 0 0)
  (pad "1" smd custom (at 0 0) (size 1 2) (layers "F.Cu")
    (options (anchor rect))
    (primitives
      (gr_poly (pts (xy -0.5 -0.8) (xy 3 -0.8) (xy 3 0.8) (xy -0.5 0.8)) (width 0) (fill yes))
      (gr_circle (center 3 0) (end 3.5 0) (width 0) (fill yes))
      (gr_line (start 0 0) (end 1 1) (width 0.2)))))"#,
    );
    let PadShape::Custom(ring) = &fp.pads[0].shape else {
        panic!("a custom pad: {:?}", fp.pads[0].shape);
    };
    for p in [
        (0.0, 0.0),
        (0.0, 0.95),
        (0.0, -0.95),
        (2.0, 0.0),
        (3.4, 0.0),
    ] {
        assert!(inside(ring, Point::new(p.0, p.1)), "{p:?} not copper");
    }
    assert!(!inside(ring, Point::new(2.0, 0.95)));
    assert!(
        report
            .summary()
            .contains("custom pad primitive gr_line dropped")
    );
}

#[test]
fn a_custom_pad_with_a_round_anchor_and_polygon_arcs() {
    let (fp, _) = convert_text(
        r#"(footprint "Lib:J" (layer "F.Cu") (at 0 0)
  (pad "1" smd custom (at 0 0) (size 1 1) (layers "F.Cu")
    (options (anchor circle))
    (primitives
      (gr_poly (pts (xy 0 -0.25) (xy 2 -0.25) (arc (start 2 -0.25) (mid 2.25 0) (end 2 0.25))
        (xy 0 0.25)) (width 0) (fill yes)))))"#,
    );
    let PadShape::Custom(ring) = &fp.pads[0].shape else {
        panic!("a custom pad: {:?}", fp.pads[0].shape);
    };
    assert!(inside(ring, Point::new(-0.45, 0.0)), "the round anchor");
    assert!(inside(ring, Point::new(2.2, 0.0)), "the arc's bulge");
}

/// The corners Signex draws a `trapezoid` pad of `size` and `delta` with
/// (`docs/SNX_FORMAT_NOTES.md`, "Trapezoid pads").
fn signex_trapezoid(size: Point, delta: Point) -> [Point; 4] {
    let (hw, hh) = (size.x / 2.0, size.y / 2.0);
    let (qx, qy) = (delta.x / 4.0, delta.y / 4.0);
    [
        Point::new(-(hw - qx), -(hh - qy)),
        Point::new(hw - qx, -(hh + qy)),
        Point::new(hw + qx, hh + qy),
        Point::new(-(hw + qx), hh - qy),
    ]
}

#[test]
fn trapezoid_pads_keep_their_corners_in_the_native_shape() {
    for (layer, delta) in [
        ("F.Cu", "0 0.4"),
        ("F.Cu", "0.3 0"),
        ("B.Cu", "0.3 0"),
        ("B.Cu", "0 -0.4"),
    ] {
        let (fp, _) = convert_text(&format!(
            r#"(footprint "Lib:T" (layer "{layer}") (at 0 0)
  (pad "1" smd trapezoid (at 0 0) (size 1 2) (rect_delta {delta}) (layers "{layer}")))"#
        ));
        let pad = &fp.pads[0];
        let &PadShape::Trapezoid(signex_delta) = &pad.shape else {
            panic!("{layer} {delta}: {:?}", pad.shape);
        };
        let source: Vec<f64> = delta.split(' ').map(|v| v.parse().unwrap()).collect();
        let bottom = layer == "B.Cu";
        let want: Vec<Point> = trapezoid_corners(pad.size, Point::new(source[0], source[1]))
            .into_iter()
            .map(|c| mirror_x(c, bottom))
            .collect();
        for corner in signex_trapezoid(pad.size, signex_delta) {
            assert!(
                want.iter()
                    .any(|w| (w.x - corner.x).abs() < 1e-9 && (w.y - corner.y).abs() < 1e-9),
                "{layer} {delta}: {corner:?} not in {want:?}"
            );
        }
    }
}

#[test]
fn a_trapezoid_without_a_delta_is_a_rectangle() {
    let (fp, _) = convert_text(
        r#"(footprint "Lib:T" (layer "F.Cu") (at 0 0)
  (pad "1" smd trapezoid (at 0 0) (size 1 2) (layers "F.Cu")))"#,
    );
    assert_eq!(fp.pads[0].shape, PadShape::Rect);
}

#[test]
fn unreadable_positions_and_sizes_are_reported_and_dropped() {
    let (fp, report) = convert_text(
        r#"(footprint "Lib:R" (layer "F.Cu") (at 0 0)
  (pad "1" smd rect (at x 0) (size 1 1) (layers "F.Cu"))
  (pad "2" smd rect (at 1 0) (size -1 1) (layers "F.Cu"))
  (pad "3" smd rect (at 2 0) (size 1 1) (layers "F.Cu")))"#,
    );
    let numbers: Vec<&str> = fp.pads.iter().map(|p| p.number.as_str()).collect();
    assert_eq!(numbers, ["3"]);
    let summary = report.summary();
    assert!(
        summary.contains("pad without a readable position dropped"),
        "{summary}"
    );
    assert!(
        summary.contains("pad without a positive size dropped"),
        "{summary}"
    );

    let node = sexpr::parse(r#"(footprint "Lib:R" (layer "F.Cu") (at 1 nan))"#).expect("parses");
    let stack = stack();
    let (mut nets, mut ids, mut report) =
        (NetTable::default(), Ids::seeded("x"), Report::default());
    let mut cx = Context {
        stack: &stack,
        nets: &mut nets,
        ids: &mut ids,
        report: &mut report,
    };
    assert!(convert(&node, &mut cx).is_none());
    assert!(
        report
            .summary()
            .contains("footprint without a readable position dropped")
    );
}

#[test]
fn footprint_items_with_no_signex_form_are_reported() {
    let (fp, report) = convert_text(
        r#"(footprint "Lib:U" (layer "F.Cu") (at 0 0)
  (fp_text user "hidden note" (at 0 3) (layer "F.Fab") hide)
  (fp_text_box "box" (start 0 0) (end 1 1) (layer "F.Fab"))
  (fp_curve (pts (xy 0 0) (xy 1 1) (xy 2 1) (xy 3 0)) (layer "F.SilkS"))
  (fp_poly (pts (xy 0 0) (xy 2 0) (arc (start 2 0) (mid 3 1) (end 2 2)) (xy 0 2))
    (layer "F.SilkS"))
  (model "${KIPRJMOD}/part.step" (offset (xyz 0 0 0))))"#,
    );
    let summary = report.summary();
    assert!(
        summary.contains("hidden footprint text dropped"),
        "{summary}"
    );
    assert!(
        summary.contains("footprint fp_text_box dropped"),
        "{summary}"
    );
    assert!(summary.contains("footprint fp_curve dropped"), "{summary}");
    assert!(summary.contains("footprint 3D model dropped"), "{summary}");
    let poly = fp
        .graphics
        .iter()
        .find(|g| g.graphic_type == "poly")
        .expect("the polygon");
    assert!(poly.points.len() > 6, "{:?}", poly.points);
    assert!(poly.points.iter().any(|p| p.x > 2.9));
}

/// A library footprint whose pad 1 sits at (1, 0.5) turned 15 degrees,
/// placed at angle `a`: on top as stored, or flipped to the bottom the way
/// the source flips it (local y negated, every angle `t` becomes `180 - t`).
fn placed_text(a: f64, bottom: bool) -> String {
    let (layer, fp_angle, y, pad_angle) = if bottom {
        ("B.Cu", 180.0 - a, -0.5, 180.0 - (a + 15.0))
    } else {
        ("F.Cu", a, 0.5, a + 15.0)
    };
    format!(
        r#"(footprint "Lib:Q" (layer "{layer}") (at 50 40 {fp_angle})
  (pad "1" smd rect (at 1 {y} {pad_angle}) (size 0.8 0.4) (layers "{layer}") (net 1 "N")))"#
    )
}

#[test]
fn a_flipped_footprint_keeps_its_library_pad_coordinates() {
    for a in [0.0, 30.0, 90.0, 200.0] {
        let (top, _) = convert_text(&placed_text(a, false));
        let (bottom, _) = convert_text(&placed_text(a, true));
        assert_eq!(bottom.layer, "BottomCopper");
        assert_near(top.pads[0].position, Point::new(1.0, 0.5));
        assert_near(bottom.pads[0].position, Point::new(1.0, 0.5));
        // Flipping in Signex turns a part the other way, as the source's
        // flip did: the bottom rotation is the top one negated.
        let expected = normalize_deg(-top.rotation);
        assert!(
            (bottom.rotation - expected).abs() < 1e-9,
            "{a}: {}",
            bottom.rotation
        );
        // A rectangle half a turn round is the same copper.
        let (t, b) = (
            top.pads[0].rotation % 180.0,
            bottom.pads[0].rotation % 180.0,
        );
        assert!((t - b).abs() < 1e-9, "{a}: own turns {t} and {b}");
        // And the flipped pad lands where the source puts it.
        let source = source_world(Point::new(50.0, 40.0), 180.0 - a, Point::new(1.0, -0.5));
        assert_near(signex_world(&bottom, bottom.pads[0].position), source);
    }
}

#[test]
fn designator_and_value_texts_become_fields() {
    let (fp, _) = convert_text(
        r#"(module "Lib:R" (layer "B.Cu") (at 10 10 90)
  (fp_text reference "R7" (at 0 -1.5 90) (layer "B.SilkS")
    (effects (font (size 1.2 1.2)) (justify mirror)))
  (fp_text value "4k7" (at 0 1.5 90) (layer "B.Fab") hide
    (effects (font (size 1 1))))
  (fp_text user "note" (at 0 3 90) (layer "B.Fab")))"#,
    );
    assert_eq!(fp.reference, "R7");
    assert_eq!(fp.value, "4k7");
    let reference = fp
        .properties
        .iter()
        .find(|p| p.key == "Reference")
        .expect("Reference");
    assert_eq!(reference.value, "R7");
    assert_eq!(reference.layer.as_deref(), Some("BottomSilkscreen"));
    assert_eq!(reference.font_size, Some(1.2));
    assert_near(
        reference.position.expect("a position"),
        Point::new(0.0, 1.5),
    );
    assert!(!reference.hidden);
    let value = fp
        .properties
        .iter()
        .find(|p| p.key == "Value")
        .expect("Value");
    assert!(value.hidden);
    let texts: Vec<&str> = fp
        .graphics
        .iter()
        .filter(|g| g.graphic_type == "text")
        .map(|g| g.text.as_str())
        .collect();
    assert_eq!(texts, ["note"]);
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
    assert!(
        report
            .summary()
            .contains("3 x source-tool metadata field dropped")
    );
}

#[test]
fn an_imported_library_value_fills_an_empty_value_and_is_dropped() {
    let (fp, report) = convert_text(
        r#"(footprint "Lib:C" (layer "F.Cu") (at 0 0)
  (property "Reference" "C1" (at 0 0 0) (layer "F.SilkS"))
  (property "Value" "")
  (property "ALTIUM_VALUE" "100nF"))"#,
    );
    assert_eq!(fp.value, "100nF");
    assert!(fp.properties.iter().all(|p| p.key != "ALTIUM_VALUE"));
    assert!(
        report
            .summary()
            .contains("imported-library value field used as the empty Value")
    );
    let (kept, _) = convert_text(
        r#"(footprint "Lib:C" (layer "F.Cu") (at 0 0)
  (property "Value" "10uF")
  (property "ALTIUM_VALUE" "100nF"))"#,
    );
    assert_eq!(kept.value, "10uF");
}
