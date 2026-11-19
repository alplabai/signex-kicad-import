// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Board-level drawings and texts. Edge-cut drawings also become pieces of
//! the board edge; a filled copper shape becomes solid copper, an unfilled
//! one tracks and copper arcs along its stroke.

use snx_writer::model::{BoardGraphic, BoardText, Point, Track, TrackArc, Zone, ZoneKind};

use crate::geometry::{
    arc_polyline, at_of, circle_polygon, circumcenter, normalize_deg, number_of, point_of,
    ring_points, stroke_width,
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
    /// The strokes of unfilled copper drawings.
    pub(crate) tracks: Vec<Track>,
    pub(crate) arcs: Vec<TrackArc>,
    pub(crate) texts: Vec<BoardText>,
}

/// Root items that are not drawings and are read elsewhere, or carry
/// nothing for the board (a group only lists its members).
const READ_ELSEWHERE: [&str; 21] = [
    "version",
    "generator",
    "generator_version",
    "host",
    "general",
    "paper",
    "page",
    "title_block",
    "layers",
    "setup",
    "property",
    "net",
    "net_class",
    "footprint",
    "module",
    "segment",
    "arc",
    "via",
    "zone",
    "group",
    "embedded_fonts",
];

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
            Some(kind) if !READ_ELSEWHERE.contains(&kind) => {
                // gr_text_box, gr_curve, gr_bbox, image, dimension, target,
                // table, generated and anything newer.
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
    let Some(graphic) = shape(node, layer.clone(), ids, report) else {
        report.note("board drawing without its points dropped");
        return;
    };
    if is_copper(&layer) {
        match solid(node, &graphic, nets) {
            Some(zone) => out.solids.push(zone),
            None => strokes(node, &graphic, nets, ids, report, out),
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
fn shape(node: &SExpr, layer: String, ids: &mut Ids, report: &mut Report) -> Option<BoardGraphic> {
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
            let (from, to) = if increasing {
                (start, end)
            } else {
                (end, start)
            };
            g.graphic_type = "arc".to_string();
            g.radius = (from.x - center.x).hypot(from.y - center.y);
            g.center = Some(center);
            g.start = Some(from);
            g.end = Some(to);
        }
        _ => {
            g.graphic_type = "poly".to_string();
            g.points = poly_points(node, report);
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

/// A polygon's corners, arcs in it as chords (as fine as an edge arc, since
/// an edge polygon becomes part of the outline ring).
fn poly_points(node: &SExpr, report: &mut Report) -> Vec<Point> {
    let mut unread = 0;
    let points = node
        .find("pts")
        .map(|pts| ring_points(pts, EDGE_ARC_STEP_DEG, &mut unread))
        .unwrap_or_default();
    for _ in 0..unread {
        report.note("board polygon corner that does not read dropped");
    }
    points
}

fn is_filled(node: &SExpr) -> bool {
    matches!(
        node.find("fill").and_then(SExpr::first_arg),
        Some("yes" | "solid")
    )
}

/// A filled copper polygon, rectangle or circle as solid copper; `None`
/// otherwise.
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
        "circle" if g.radius > 0.0 => circle_polygon(g.center?, g.radius, EDGE_CIRCLE_CORNERS),
        _ => return None,
    };
    Some(Zone {
        uuid: g.uuid,
        kind: ZoneKind::Solid,
        net: nets.of(node),
        net_name: String::new(),
        layer: g.layer.clone(),
        outline,
        holes: Vec::new(),
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

/// The stroke of an unfilled copper drawing as tracks (lines, rectangle
/// and polygon sides) and copper arcs (an arc; a circle as two halves), on
/// the drawing's layer and net. A stroke without a width is reported and
/// dropped.
fn strokes(
    node: &SExpr,
    g: &BoardGraphic,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
    out: &mut Drawings,
) {
    if g.width <= 0.0 {
        report.note("copper drawing without a positive width dropped");
        return;
    }
    let net = nets.of(node);
    let mut segments: Vec<(Point, Point)> = Vec::new();
    let mut arcs: Vec<[Point; 3]> = Vec::new();
    match g.graphic_type.as_str() {
        "line" => segments.extend(g.start.zip(g.end)),
        "rect" => {
            if let (Some(s), Some(e)) = (g.start, g.end) {
                let corners = [s, Point::new(e.x, s.y), e, Point::new(s.x, e.y)];
                segments.extend((0..4).map(|i| (corners[i], corners[(i + 1) % 4])));
            }
        }
        "poly" => {
            let n = g.points.len();
            segments.extend((0..n).map(|i| (g.points[i], g.points[(i + 1) % n])));
        }
        "arc" => {
            // From the source points: the model's arc may have its ends
            // swapped.
            if let (Some(s), Some(m), Some(e)) = (
                point_of(node, "start"),
                point_of(node, "mid"),
                point_of(node, "end"),
            ) {
                arcs.push([s, m, e]);
            }
        }
        "circle" => {
            if let Some(c) = g.center.filter(|_| g.radius > 0.0) {
                let r = g.radius;
                let (east, west) = (Point::new(c.x + r, c.y), Point::new(c.x - r, c.y));
                arcs.push([east, Point::new(c.x, c.y + r), west]);
                arcs.push([west, Point::new(c.x, c.y - r), east]);
            }
        }
        _ => {}
    }
    if segments.is_empty() && arcs.is_empty() {
        report.note("copper drawing without its points dropped");
        return;
    }
    report.note("unfilled copper drawing written as tracks");
    let mut first = Some(g.uuid);
    let mut uuid = |ids: &mut Ids| first.take().unwrap_or_else(|| ids.fresh());
    for (start, end) in segments.into_iter().filter(|(s, e)| s != e) {
        out.tracks.push(Track {
            uuid: uuid(ids),
            net,
            layer: g.layer.clone(),
            width: g.width,
            start,
            end,
        });
    }
    for [start, mid, end] in arcs {
        out.arcs.push(TrackArc {
            uuid: uuid(ids),
            net,
            layer: g.layer.clone(),
            width: g.width,
            start,
            mid,
            end,
        });
    }
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
    let Some((position, angle)) = at_of(node) else {
        report.note("board text without a readable position dropped");
        return None;
    };
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

    #[test]
    fn unfilled_copper_drawings_become_tracks_and_arcs() {
        let (d, report) = read_text(
            r#"(kicad_pcb (net 1 "GND")
                 (gr_line (start 0 0) (end 5 0) (stroke (width 0.3)) (layer "F.Cu") (net 1))
                 (gr_arc (start 0 0) (mid 1 -1) (end 2 0) (stroke (width 0.2)) (layer "B.Cu"))
                 (gr_rect (start 0 0) (end 2 1) (stroke (width 0.2)) (fill none) (layer "F.Cu"))
                 (gr_circle (center 0 0) (end 1 0) (stroke (width 0.2)) (layer "B.Cu"))
                 (gr_line (start 0 0) (end 1 0) (stroke (width 0)) (layer "F.Cu")))"#,
        );
        assert_eq!(d.tracks.len(), 5);
        assert_eq!(d.tracks[0].layer, "TopCopper");
        assert_eq!((d.tracks[0].net, d.tracks[0].width), (1, 0.3));
        assert_eq!(d.arcs.len(), 3);
        let arc = &d.arcs[0];
        assert_eq!(arc.layer, "BottomCopper");
        assert_eq!(
            (arc.start, arc.mid, arc.end),
            (
                Point::new(0.0, 0.0),
                Point::new(1.0, -1.0),
                Point::new(2.0, 0.0)
            )
        );
        assert!(d.graphics.is_empty() && d.solids.is_empty());
        let summary = report.summary();
        assert!(
            summary.contains("copper drawing without a positive width dropped"),
            "{summary}"
        );
    }

    #[test]
    fn polygon_arcs_become_chords_and_the_edge_closes_through_them() {
        let (d, _) = read_text(
            r#"(kicad_pcb (gr_poly (pts (xy 0 0) (xy 10 0)
                   (arc (start 10 0) (mid 15 5) (end 10 10)) (xy 0 10))
                 (layer "Edge.Cuts")))"#,
        );
        let ring = &d.edge_pieces[0].points;
        assert!(ring.len() > 10, "{ring:?}");
        assert!(ring.iter().any(|p| p.x > 14.9), "{ring:?}");
    }

    #[test]
    fn root_items_with_no_signex_form_are_reported() {
        let (_, report) = read_text(
            r#"(kicad_pcb (version 20240108) (generator "pcbnew") (group "" (members "x"))
                 (target plus (at 1 1) (size 5) (layer "Edge.Cuts"))
                 (table (column_count 2))
                 (dimension (type aligned) (layer "F.SilkS")))"#,
        );
        let summary = report.summary();
        assert!(summary.contains("board target dropped"), "{summary}");
        assert!(summary.contains("board table dropped"), "{summary}");
        assert!(summary.contains("board dimension dropped"), "{summary}");
        assert!(!summary.contains("group"), "{summary}");
        assert!(!summary.contains("version"), "{summary}");
    }
}
