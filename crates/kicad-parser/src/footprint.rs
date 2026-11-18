// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Footprints and their pads.
//!
//! The source stores a footprint's children in its local frame as placed,
//! and a pad's or text's angle is its angle on the board. Flipping a
//! footprint to the bottom negates local y and turns it by a half turn
//! (angle `a` becomes `180 - a`). Signex keeps library-side-up children and
//! mirrors local x itself for a bottom-side footprint, and turns the other
//! way (the source turns counter-clockwise on screen with y down; Signex
//! applies the plain rotation matrix to y-down coordinates). So, with
//! footprint angle `a` and a pad or text angle `p`:
//!
//! * top: local `(x, y)`, Signex angle `-a`, own turn `a - p`;
//! * bottom: local `(x, -y)` (the library side up again), Signex angle
//!   `180 - a`, own turn `180 - a + p`;
//! * a corner in a pad's own frame (a custom or trapezoid outline) keeps
//!   its coordinates on top and has x negated on the bottom.
//!
//! Each puts every point at the board position the source gives it.

use snx_writer::model::{Footprint, FootprintGraphic, Pad, PadShape, PadType, Point, Property};

use crate::geometry::{
    at_of, circumcenter, mirror_x, normalize_deg, number_of, point_args, point_of, stroke_width,
};
use crate::ids::Ids;
use crate::layers::Stack;
use crate::nets::NetTable;
use crate::report::Report;
use crate::ring::{is_simple_ring, to_nm};
use crate::sexpr::{Atom, SExpr};

/// The shared state footprint conversion reads and updates.
pub(crate) struct Context<'a> {
    pub(crate) stack: &'a Stack,
    pub(crate) nets: &'a mut NetTable,
    pub(crate) ids: &'a mut Ids,
    pub(crate) report: &'a mut Report,
}

/// How a footprint sits: its source angle and whether it is on the bottom.
#[derive(Clone, Copy)]
struct Frame {
    angle: f64,
    bottom: bool,
}

impl Frame {
    /// A footprint-local point, library side up.
    fn local(self, p: Point) -> Point {
        if self.bottom {
            Point::new(p.x, -p.y)
        } else {
            p
        }
    }

    /// A corner in a pad's own frame.
    fn pad_corner(self, c: Point) -> Point {
        mirror_x(c, self.bottom)
    }

    /// The footprint's Signex angle.
    fn rotation(self) -> f64 {
        if self.bottom {
            normalize_deg(180.0 - self.angle)
        } else {
            normalize_deg(-self.angle)
        }
    }

    /// A pad's or text's own turn in the Signex footprint frame from its
    /// angle on the board.
    fn own_turn(self, angle: f64) -> f64 {
        if self.bottom {
            normalize_deg(180.0 - self.angle + angle)
        } else {
            normalize_deg(self.angle - angle)
        }
    }
}

fn has_flag(node: &SExpr, flag: &str) -> bool {
    node.children().iter().any(|c| match c {
        SExpr::Atom(Atom::Raw(a)) => a == flag,
        list => list.keyword() == Some(flag) && list.first_arg() != Some("no"),
    })
}

/// The text of a footprint field: `(property "Reference" "R1")` in newer
/// files, `(fp_text reference "R1")` in older ones.
fn field(fp: &SExpr, property: &str, text_kind: &str) -> String {
    fp.find_all("property")
        .into_iter()
        .find(|p| p.first_arg() == Some(property))
        .and_then(|p| p.arg(1))
        .or_else(|| {
            fp.find_all("fp_text")
                .into_iter()
                .find(|t| t.first_arg() == Some(text_kind))
                .and_then(|t| t.arg(1))
        })
        .unwrap_or("")
        .to_string()
}

pub(crate) fn convert(fp: &SExpr, cx: &mut Context<'_>) -> Footprint {
    let (position, angle) = at_of(fp);
    let side = fp
        .find("layer")
        .and_then(SExpr::first_arg)
        .unwrap_or("F.Cu");
    let bottom = cx.stack.token(side).as_deref() == Some("BottomCopper");
    let frame = Frame { angle, bottom };
    let uuid = cx.ids.of(fp, cx.report);
    let pads = fp
        .find_all("pad")
        .into_iter()
        .map(|p| pad(p, frame, cx))
        .collect();
    let mut graphics: Vec<FootprintGraphic> = Vec::new();
    for child in fp.children() {
        if let Some(g) = graphic(child, frame, cx.stack, cx.report) {
            graphics.push(g);
        }
    }
    let mut value = field(fp, "Value", "value");
    let mut properties = Vec::new();
    for node in fp.find_all("property") {
        let key = node.first_arg().unwrap_or("");
        if key.eq_ignore_ascii_case(IMPORTED_VALUE_KEY) {
            // A value a part brought from another tool's library: it fills an
            // empty Value, and the field itself is dropped.
            let imported = node.arg(1).unwrap_or("");
            if value.is_empty() && !imported.is_empty() {
                value = imported.to_string();
                cx.report
                    .note("imported-library value field used as the empty Value");
            } else {
                cx.report.note("imported-library value field dropped");
            }
        } else if is_source_metadata(key) {
            cx.report.note("source-tool metadata field dropped");
        } else {
            properties.push(property(node, frame, cx.stack));
        }
    }
    // Older files keep the designator and value as `fp_text reference` /
    // `fp_text value`; they become the Reference and Value fields.
    for (kind, key) in [("reference", "Reference"), ("value", "Value")] {
        if properties.iter().any(|p| p.key == key) {
            continue;
        }
        if let Some(text) = fp
            .find_all("fp_text")
            .into_iter()
            .find(|t| t.first_arg() == Some(kind))
        {
            let mut prop = property(text, frame, cx.stack);
            prop.key = key.to_string();
            prop.value = text.arg(1).unwrap_or("").to_string();
            properties.push(prop);
        }
    }
    Footprint {
        uuid,
        reference: field(fp, "Reference", "reference"),
        value,
        footprint_id: fp.first_arg().unwrap_or("").to_string(),
        position,
        rotation: frame.rotation(),
        layer: if bottom { "BottomCopper" } else { "TopCopper" }.to_string(),
        locked: has_flag(fp, "locked"),
        pads,
        graphics,
        properties,
    }
}

/// The field a part imported from a binary-layout tool's library carries
/// its value in.
const IMPORTED_VALUE_KEY: &str = "ALTIUM_VALUE";

/// A footprint field that records the source tool's own bookkeeping (its
/// schematic sheet files and its `ki_` keys), not the part: dropped, since
/// Signex keeps its own schematic link. A part's Description stays.
fn is_source_metadata(key: &str) -> bool {
    matches!(key, "Sheetfile" | "Sheetname") || key.starts_with("ki_")
}

fn pad_type(token: &str) -> PadType {
    match token {
        "thru_hole" => PadType::Thru,
        "np_thru_hole" => PadType::NpThru,
        "connect" => PadType::Connect,
        _ => PadType::Smd,
    }
}

fn pad(node: &SExpr, frame: Frame, cx: &mut Context<'_>) -> Pad {
    let (at, pad_angle) = at_of(node);
    let size = node
        .find("size")
        .and_then(point_args)
        .unwrap_or(Point::new(1.0, 1.0));
    let kind = pad_type(node.arg(1).unwrap_or("smd"));
    let shape = pad_shape(node, size, frame, cx.report);
    let (drill, drill_shape) = drill_of(node, kind, cx.report);
    let names: Vec<&str> = node
        .find("layers")
        .map(|l| {
            l.children()
                .iter()
                .filter_map(|c| match c {
                    SExpr::Atom(a) => Some(a.as_str()),
                    SExpr::List(_) => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let layers = cx.stack.pad_layers(&names, cx.report);
    Pad {
        uuid: cx.ids.of(node, cx.report),
        number: node.first_arg().unwrap_or("").to_string(),
        pad_type: kind,
        shape,
        position: frame.local(at),
        size,
        rotation: frame.own_turn(pad_angle),
        drill,
        drill_shape,
        layers,
        net: cx.nets.net_of(node),
    }
}

/// `(drill d)`, `(drill oval w h)`, `(drill d (offset x y))`. A slot keeps
/// its width as the diameter and is marked `oval`.
fn drill_of(node: &SExpr, kind: PadType, report: &mut Report) -> (Option<f64>, String) {
    let Some(drill) = node.find("drill") else {
        return (None, String::new());
    };
    if matches!(kind, PadType::Smd | PadType::Connect) {
        return (None, String::new());
    }
    if drill.find("offset").is_some() {
        report.note("drill offset from the pad centre ignored");
    }
    let numbers: Vec<f64> = drill
        .children()
        .iter()
        .filter_map(|c| match c {
            SExpr::Atom(a) => a.as_str().parse::<f64>().ok().filter(|v| v.is_finite()),
            SExpr::List(_) => None,
        })
        .collect();
    let oval = has_flag(drill, "oval");
    if oval {
        report.note("slot drill kept as a round hole of its width, marked oval");
    }
    let diameter = numbers.first().copied().filter(|d| *d > 0.0);
    (diameter, if oval { "oval".into() } else { String::new() })
}

fn pad_shape(node: &SExpr, size: Point, frame: Frame, report: &mut Report) -> PadShape {
    match node.arg(2).unwrap_or("rect") {
        "circle" => PadShape::Circle,
        "oval" => PadShape::Oval,
        "roundrect" => {
            if node
                .find("chamfer_ratio")
                .is_some_and(|c| c.arg_f64(0) != Some(0.0))
            {
                report.note("chamfered pad corners written as round corners");
            }
            let ratio = number_of(node, "roundrect_rratio").unwrap_or(0.25);
            PadShape::RoundRect(ratio.clamp(0.0, 0.5))
        }
        "trapezoid" => {
            let delta = node
                .find("rect_delta")
                .and_then(point_args)
                .unwrap_or_default();
            let corners = trapezoid_corners(size, delta);
            custom_or(corners, frame, PadShape::Rect, report)
        }
        "custom" => custom_pad(node, frame, report),
        "rect" => PadShape::Rect,
        other => {
            report.note(format!("pad shape {other} written as a rectangle"));
            PadShape::Rect
        }
    }
}

/// The corners of a trapezoid pad of `size` with the source's `rect_delta`.
fn trapezoid_corners(size: Point, delta: Point) -> Vec<Point> {
    let (dx, dy) = (size.x / 2.0, size.y / 2.0);
    let (ddx, ddy) = (delta.x / 2.0, delta.y / 2.0);
    vec![
        Point::new(-dx - ddy, dy + ddx),
        Point::new(dx + ddy, dy - ddx),
        Point::new(dx - ddy, -dy + ddx),
        Point::new(-dx + ddy, -dy - ddx),
    ]
}

/// A custom pad: its first polygon primitive as the outline; the anchor
/// shape when there is none or it is not a simple polygon.
fn custom_pad(node: &SExpr, frame: Frame, report: &mut Report) -> PadShape {
    let anchor = match node
        .find("options")
        .and_then(|o| o.find("anchor"))
        .and_then(SExpr::first_arg)
    {
        Some("circle") => PadShape::Circle,
        _ => PadShape::Rect,
    };
    let primitives = node.find("primitives").map(SExpr::children).unwrap_or(&[]);
    let mut polygon: Option<Vec<Point>> = None;
    for primitive in primitives {
        match primitive.keyword() {
            Some("gr_poly") if polygon.is_none() => {
                polygon = primitive.find("pts").map(|pts| {
                    pts.find_all("xy")
                        .into_iter()
                        .filter_map(point_args)
                        .collect()
                });
            }
            Some(kind) => report.note(format!("custom pad primitive {kind} dropped")),
            None => {}
        }
    }
    match polygon {
        Some(corners) => custom_or(corners, frame, anchor, report),
        None => anchor,
    }
}

/// `corners` (pad frame, source side) as a custom outline, or `fallback`
/// when they do not make a simple polygon.
fn custom_or(
    corners: Vec<Point>,
    frame: Frame,
    fallback: PadShape,
    report: &mut Report,
) -> PadShape {
    let mut ring: Vec<Point> = corners.into_iter().map(|c| frame.pad_corner(c)).collect();
    ring.dedup_by(|a, b| nm_eq(*a, *b));
    while ring.len() > 1 && nm_eq(ring[0], ring[ring.len() - 1]) {
        ring.pop();
    }
    if is_simple_ring(&ring) {
        PadShape::Custom(ring)
    } else {
        report.note("pad outline not a simple polygon; anchor shape kept");
        fallback
    }
}

fn nm_eq(a: Point, b: Point) -> bool {
    to_nm(a.x) == to_nm(b.x) && to_nm(a.y) == to_nm(b.y)
}

fn font_size(node: &SExpr) -> Option<f64> {
    node.find("effects")
        .and_then(|e| e.find("font"))
        .and_then(|f| f.find("size"))
        .and_then(|s| s.arg_f64(0))
}

fn is_hidden(node: &SExpr) -> bool {
    has_flag(node, "hide") || node.find("effects").is_some_and(|e| has_flag(e, "hide"))
}

fn property(node: &SExpr, frame: Frame, stack: &Stack) -> Property {
    let (at, angle) = at_of(node);
    Property {
        key: node.first_arg().unwrap_or("").to_string(),
        value: node.arg(1).unwrap_or("").to_string(),
        position: node.find("at").map(|_| frame.local(at)),
        rotation: frame.own_turn(angle),
        layer: node
            .find("layer")
            .and_then(SExpr::first_arg)
            .and_then(|name| stack.token(name)),
        font_size: font_size(node),
        hidden: is_hidden(node),
    }
}

/// One footprint drawing; `None` for a child that is not one, or one on a
/// layer Signex does not have.
fn graphic(
    node: &SExpr,
    frame: Frame,
    stack: &Stack,
    report: &mut Report,
) -> Option<FootprintGraphic> {
    let kind = node.keyword()?;
    let graphic_type = match kind {
        "fp_line" => "line",
        "fp_rect" => "rect",
        "fp_circle" => "circle",
        "fp_arc" => "arc",
        "fp_poly" => "poly",
        "fp_text" => "text",
        _ => return None,
    };
    let layer_name = node.find("layer").and_then(SExpr::first_arg).unwrap_or("");
    let Some(layer) = stack.token(layer_name) else {
        report.note(format!("footprint drawing on layer {layer_name} dropped"));
        return None;
    };
    let local = |key: &str| point_of(node, key).map(|p| frame.local(p));
    let mut g = FootprintGraphic {
        graphic_type: graphic_type.to_string(),
        layer,
        width: stroke_width(node),
        fill: fill_of(node),
        ..FootprintGraphic::default()
    };
    match graphic_type {
        "line" | "rect" => {
            g.start = local("start");
            g.end = local("end");
        }
        "circle" => {
            g.center = local("center");
            g.end = local("end");
        }
        "arc" => {
            let (start, end) = (local("start")?, local("end")?);
            let Some(mid) = local("mid") else {
                report.note("footprint arc in the old angle form dropped");
                return None;
            };
            g.center = circumcenter(start, mid, end);
            if g.center.is_none() {
                g.graphic_type = "line".to_string();
            } else {
                g.mid = Some(mid);
            }
            g.start = Some(start);
            g.end = Some(end);
        }
        "poly" => {
            g.points = node
                .find("pts")
                .map(|pts| {
                    pts.find_all("xy")
                        .into_iter()
                        .filter_map(point_args)
                        .map(|p| frame.local(p))
                        .collect()
                })
                .unwrap_or_default();
            if node
                .find("pts")
                .is_some_and(|pts| pts.find("arc").is_some())
            {
                report.note("arc inside a footprint polygon dropped");
            }
        }
        _ => {
            // The designator and value texts are the Reference and Value
            // fields (see `convert`), not drawings.
            if is_hidden(node) || matches!(node.first_arg(), Some("reference" | "value")) {
                return None;
            }
            let (at, angle) = at_of(node);
            g.text = node.arg(1).unwrap_or("").to_string();
            g.font_size = font_size(node).unwrap_or(1.0);
            g.position = Some(frame.local(at));
            g.rotation = frame.own_turn(angle);
        }
    }
    Some(g)
}

fn fill_of(node: &SExpr) -> String {
    match node.find("fill").and_then(SExpr::first_arg) {
        Some("yes" | "solid") => "solid".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
#[path = "footprint_tests.rs"]
mod tests;
