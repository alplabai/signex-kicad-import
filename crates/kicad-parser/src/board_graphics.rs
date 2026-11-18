// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Board-level drawings and texts. Edge-cut drawings also become pieces of
//! the board edge; a filled copper polygon becomes solid copper.

use snx_writer::model::{BoardGraphic, BoardText, Point, Zone, ZoneKind};

use crate::geometry::{
    arc_polyline, at_of, circle_polygon, circumcenter, normalize_deg, number_of, point_args,
    point_of, stroke_width,
};
use crate::ids::Ids;
use crate::layers::{Stack, is_copper};
use crate::nets::NetTable;
use crate::outline::EdgePiece;
use crate::report::Report;
use crate::sexpr::SExpr;

/// Degrees per chord for an edge arc in the outline ring.
const EDGE_ARC_STEP_DEG: f64 = 5.0;
/// Corners of an edge circle in the outline ring.
const EDGE_CIRCLE_CORNERS: usize = 72;

/// What the board-level drawings give.
#[derive(Default)]
pub(crate) struct Drawings {
    /// Drawings on layers other than the edge and copper.
    pub(crate) graphics: Vec<BoardGraphic>,
    /// Edge drawings, parallel to `edge_pieces`.
    pub(crate) edge_graphics: Vec<BoardGraphic>,
    pub(crate) edge_pieces: Vec<EdgePiece>,
    /// Filled copper polygons.
    pub(crate) solids: Vec<Zone>,
    pub(crate) texts: Vec<BoardText>,
}

pub(crate) fn read(
    root: &SExpr,
    stack: &Stack,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
) -> Drawings {
    let mut out = Drawings::default();
    for node in root.children() {
        match node.keyword() {
            Some("gr_line" | "gr_rect" | "gr_circle" | "gr_arc" | "gr_poly") => {
                drawing(node, stack, nets, ids, report, &mut out);
            }
            Some("gr_text") => {
                if let Some(text) = text(node, stack, ids, report) {
                    out.texts.push(text);
                }
            }
            Some(kind @ ("gr_text_box" | "gr_curve" | "gr_bbox" | "image" | "dimension")) => {
                report.note(format!("board {kind} dropped"));
            }
            _ => {}
        }
    }
    out
}

fn drawing(
    node: &SExpr,
    stack: &Stack,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
    out: &mut Drawings,
) {
    let layer_name = node.find("layer").and_then(SExpr::first_arg).unwrap_or("");
    let Some(layer) = stack.token(layer_name) else {
        report.note(format!("board drawing on layer {layer_name} dropped"));
        return;
    };
    let Some(graphic) = shape(node, layer.clone(), ids) else {
        report.note("board drawing without its points dropped");
        return;
    };
    if is_copper(&layer) {
        match solid(node, &graphic, nets) {
            Some(zone) => out.solids.push(zone),
            None => report.note("unfilled copper drawing dropped"),
        }
        return;
    }
    if layer == "EdgeCuts" {
        if let Some(piece) = edge_piece(node, &graphic) {
            out.edge_pieces.push(piece);
            out.edge_graphics.push(graphic);
        }
        return;
    }
    out.graphics.push(graphic);
}

/// The drawing in the model's terms; an arc runs from `start` to `end` the
/// way of increasing angle, so it is swapped when the source runs it the
/// other way.
fn shape(node: &SExpr, layer: String, ids: &mut Ids) -> Option<BoardGraphic> {
    let kind = node.keyword()?;
    let mut g = BoardGraphic {
        uuid: ids.fresh(),
        layer,
        width: stroke_width(node),
        ..BoardGraphic::default()
    };
    match kind {
        "gr_line" | "gr_rect" => {
            g.graphic_type = if kind == "gr_line" { "line" } else { "rect" }.to_string();
            g.start = Some(point_of(node, "start")?);
            g.end = Some(point_of(node, "end")?);
        }
        "gr_circle" => {
            let (center, end) = (point_of(node, "center")?, point_of(node, "end")?);
            g.graphic_type = "circle".to_string();
            g.radius = (end.x - center.x).hypot(end.y - center.y);
            g.center = Some(center);
            g.end = Some(end);
        }
        "gr_arc" => {
            let start = point_of(node, "start")?;
            let mid = point_of(node, "mid")?;
            let end = point_of(node, "end")?;
            let Some(center) = circumcenter(start, mid, end) else {
                g.graphic_type = "line".to_string();
                g.start = Some(start);
                g.end = Some(end);
                return Some(g);
            };
            let increasing = sweeps_up(center, start, mid, end);
            let (from, to) = if increasing { (start, end) } else { (end, start) };
            g.graphic_type = "arc".to_string();
            g.radius = (from.x - center.x).hypot(from.y - center.y);
            g.center = Some(center);
            g.start = Some(from);
            g.end = Some(to);
        }
        _ => {
            g.graphic_type = "poly".to_string();
            g.points = poly_points(node);
            if g.points.len() < 2 {
                return None;
            }
        }
    }
    Some(g)
}

/// Whether the arc from `start` through `mid` to `end` runs the way of
/// increasing angle.
fn sweeps_up(center: Point, start: Point, mid: Point, end: Point) -> bool {
    let angle = |p: Point| (p.y - center.y).atan2(p.x - center.x);
    let tau = std::f64::consts::TAU;
    let forward = (angle(end) - angle(start)).rem_euclid(tau);
    (angle(mid) - angle(start)).rem_euclid(tau) <= forward
}

fn poly_points(node: &SExpr) -> Vec<Point> {
    node.find("pts")
        .map(|pts| pts.find_all("xy").into_iter().filter_map(point_args).collect())
        .unwrap_or_default()
}

fn is_filled(node: &SExpr) -> bool {
    matches!(
        node.find("fill").and_then(SExpr::first_arg),
        Some("yes" | "solid")
    )
}

/// A filled copper polygon or rectangle as solid copper; `None` otherwise.
fn solid(node: &SExpr, g: &BoardGraphic, nets: &mut NetTable) -> Option<Zone> {
    if !is_filled(node) {
        return None;
    }
    let outline = match g.graphic_type.as_str() {
        "poly" => g.points.clone(),
        "rect" => {
            let (s, e) = (g.start?, g.end?);
            vec![s, Point::new(e.x, s.y), e, Point::new(s.x, e.y)]
        }
        _ => return None,
    };
    Some(Zone {
        uuid: g.uuid,
        kind: ZoneKind::Solid,
        net: nets.of(node),
        net_name: String::new(),
        layer: g.layer.clone(),
        outline,
        priority: 0,
        thermal_relief: false,
        thermal_gap: 0.0,
        thermal_width: 0.0,
        clearance: 0.0,
        min_thickness: 0.0,
        remove_dead_islands: false,
        min_island_area: 0.0,
    })
}

/// The edge drawing as a polyline for the outline ring.
fn edge_piece(node: &SExpr, g: &BoardGraphic) -> Option<EdgePiece> {
    let open = |points: Vec<Point>| EdgePiece {
        points,
        closed: false,
    };
    let closed = |points: Vec<Point>| EdgePiece {
        points,
        closed: true,
    };
    Some(match g.graphic_type.as_str() {
        "line" => open(vec![g.start?, g.end?]),
        "rect" => {
            let (s, e) = (g.start?, g.end?);
            closed(vec![s, Point::new(e.x, s.y), e, Point::new(s.x, e.y)])
        }
        "circle" => closed(circle_polygon(g.center?, g.radius, EDGE_CIRCLE_CORNERS)),
        "arc" => {
            // From the source points, so the chords keep the source's ends.
            let start = point_of(node, "start")?;
            let mid = point_of(node, "mid")?;
            let end = point_of(node, "end")?;
            open(arc_polyline(start, mid, end, EDGE_ARC_STEP_DEG))
        }
        _ => closed(g.points.clone()),
    })
}

fn text(node: &SExpr, stack: &Stack, ids: &mut Ids, report: &mut Report) -> Option<BoardText> {
    let layer_name = node.find("layer").and_then(SExpr::first_arg).unwrap_or("");
    let Some(layer) = stack.token(layer_name) else {
        report.note(format!("board text on layer {layer_name} dropped"));
        return None;
    };
    let (position, angle) = at_of(node);
    let font_size = node
        .find("effects")
        .and_then(|e| e.find("font"))
        .and_then(|f| number_of(f, "size"))
        .unwrap_or(1.0);
    Some(BoardText {
        uuid: ids.of(node, report),
        text: node.first_arg().unwrap_or("").to_string(),
        position,
        layer,
        font_size,
        rotation: normalize_deg(-angle),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    fn read_text(text: &str) -> (Drawings, Report) {
        let root = sexpr::parse(text).expect("parses");
        let mut nets = NetTable::read(&root);
        let mut ids = Ids::seeded(text);
        let mut report = Report::default();
        let stack = Stack::read(&root, &mut report);
        let drawings = read(&root, &stack, &mut nets, &mut ids, &mut report);
        (drawings, report)
    }

    #[test]
    fn edge_drawings_become_pieces_and_others_stay_drawings() {
        let (d, _) = read_text(
            r#"(kicad_pcb (net 1 "GND")
                 (gr_line (start 0 0) (end 10 0) (layer "Edge.Cuts") (stroke (width 0.05)))
                 (gr_arc (start 10 0) (mid 12 2) (end 10 4) (layer "Edge.Cuts"))
                 (gr_line (start 0 0) (end 5 5) (layer "F.SilkS"))
                 (gr_poly (pts (xy 0 0) (xy 1 0) (xy 1 1)) (fill yes) (layer "F.Cu") (net 1))
                 (gr_text "REV A" (at 3 4 90) (layer "F.SilkS")
                   (effects (font (size 1.5 1.5)))))"#,
        );
        assert_eq!(d.edge_pieces.len(), 2);
        assert_eq!(d.edge_graphics.len(), 2);
        assert_eq!(d.graphics.len(), 1);
        assert_eq!(d.graphics[0].layer, "TopSilkscreen");
        assert_eq!(d.solids.len(), 1);
        assert_eq!((d.solids[0].kind, d.solids[0].net), (ZoneKind::Solid, 1));
        assert_eq!(d.texts[0].rotation, 270.0);
        assert_eq!(d.texts[0].font_size, 1.5);
    }

    #[test]
    fn an_arc_runs_the_way_of_increasing_angle() {
        let (d, _) = read_text(
            r#"(kicad_pcb (gr_arc (start 1 0) (mid 0 -1) (end -1 0) (layer "F.SilkS")))"#,
        );
        let arc = &d.graphics[0];
        assert_eq!(arc.graphic_type, "arc");
        // Through y = -1 the way from (1, 0) to (-1, 0) turns down in angle,
        // so the ends swap.
        assert_eq!(arc.start, Some(Point::new(-1.0, 0.0)));
        assert_eq!(arc.end, Some(Point::new(1.0, 0.0)));
    }
}
