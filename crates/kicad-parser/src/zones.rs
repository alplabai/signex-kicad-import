// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Zones: a copper zone becomes a pour on each copper layer it names, a
//! rule area becomes a keepout on each. The computed fill is not carried;
//! Signex fills a pour itself.

use snx_writer::model::{Point, Zone, ZoneKind};

use crate::geometry::{number_of, ring_points, twice_area};
use crate::ids::Ids;
use crate::layers::{Stack, is_copper};
use crate::nets::NetTable;
use crate::report::Report;
use crate::sexpr::{Atom, SExpr};

/// Degrees per chord when an arc in a zone outline becomes corners.
const ARC_STEP_DEG: f64 = 10.0;

pub(crate) fn convert(
    node: &SExpr,
    stack: &Stack,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
) -> Vec<Zone> {
    let kind = if node.find("keepout").is_some() {
        ZoneKind::Keepout
    } else {
        ZoneKind::Pour
    };
    if let Some(rules) = node.find("keepout")
        && rules
            .children()
            .iter()
            .any(|c| c.first_arg() == Some("allowed"))
    {
        report.note("rule area that allows some items written as a full copper keepout");
    }
    let shapes = shapes_of(node, report);
    let Some((first, _)) = shapes.first() else {
        report.note("zone without an outline dropped");
        return Vec::new();
    };
    let outline = first.clone();
    let net_name = node
        .find("net_name")
        .and_then(SExpr::first_arg)
        .unwrap_or("")
        .to_string();
    let net = match nets.of(node) {
        0 => nets.named(&net_name),
        n => n,
    };
    let connect = node.find("connect_pads");
    let mode = connect.and_then(|c| match c.children().first() {
        Some(SExpr::Atom(Atom::Raw(m))) => Some(m.as_str()),
        _ => None,
    });
    if mode == Some("no") {
        report.note("zone that leaves pads unconnected written with direct pad connections");
    }
    let fill = node.find("fill");
    if fill.and_then(|f| f.find("mode")).and_then(SExpr::first_arg) == Some("hatch") {
        report.note("hatched zone written as a solid pour");
    }
    let template = Zone {
        uuid: ids.fresh(),
        kind,
        net,
        net_name,
        layer: String::new(),
        outline,
        holes: Vec::new(),
        priority: node
            .find("priority")
            .and_then(|p| p.arg_f64(0))
            .map_or(0, |p| p.max(0.0) as u32),
        thermal_relief: !matches!(mode, Some("yes" | "no")),
        thermal_gap: fill
            .and_then(|f| number_of(f, "thermal_gap"))
            .unwrap_or(0.5),
        thermal_width: fill
            .and_then(|f| number_of(f, "thermal_bridge_width"))
            .unwrap_or(0.5),
        clearance: connect
            .and_then(|c| number_of(c, "clearance"))
            .unwrap_or(0.0),
        min_thickness: number_of(node, "min_thickness").unwrap_or(0.25),
        remove_dead_islands: number_of(node, "island_removal_mode").is_none_or(|m| m != 1.0),
        min_island_area: number_of(node, "island_area_min").unwrap_or(0.0),
    };
    let mut zones = Vec::new();
    for layer in zone_layers(node, stack, report) {
        for (outline, holes) in &shapes {
            let mut zone = template.clone();
            zone.layer = layer.clone();
            zone.outline = outline.clone();
            zone.holes = holes.clone();
            zone.uuid = if zones.is_empty() {
                ids.of(node, report)
            } else {
                ids.fresh()
            };
            zones.push(zone);
        }
    }
    zones
}

/// A zone's areas: each outline with the cut-outs inside it.
type Shape = (Vec<Point>, Vec<Vec<Point>>);

/// The zone's `(polygon ..)` rings sorted into outlines and their cut-outs:
/// a ring that lies inside a larger one is a cut-out of it, any other ring
/// is an outline of its own (reported, since it becomes its own zone).
fn shapes_of(node: &SExpr, report: &mut Report) -> Vec<Shape> {
    let mut rings: Vec<Vec<Point>> = node
        .find_all("polygon")
        .into_iter()
        .map(|polygon| ring_of(polygon, report))
        .filter(|r| r.len() >= 3)
        .collect();
    // Largest first, so a cut-out always comes after the ring holding it.
    rings.sort_by(|a, b| twice_area(b).abs().total_cmp(&twice_area(a).abs()));
    let mut shapes: Vec<Shape> = Vec::new();
    for ring in rings {
        let holder = shapes
            .iter()
            .position(|(outline, _)| ring.iter().all(|&p| inside(outline, p)));
        match holder {
            Some(i) => shapes[i].1.push(ring),
            None => {
                if !shapes.is_empty() {
                    report.note("zone area apart from its first outline written as its own zone");
                }
                shapes.push((ring, Vec::new()));
            }
        }
    }
    shapes
}

/// Whether `p` lies inside the ring (even-odd rule).
fn inside(ring: &[Point], p: Point) -> bool {
    let mut crossings = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                crossings = !crossings;
            }
        }
    }
    crossings
}

/// The copper layers a zone is on: `(layer ..)` or `(layers ..)`, with the
/// copper wildcards expanded.
fn zone_layers(node: &SExpr, stack: &Stack, report: &mut Report) -> Vec<String> {
    let names: Vec<&str> = ["layer", "layers"]
        .iter()
        .filter_map(|key| node.find(key))
        .flat_map(|n| n.children())
        .filter_map(|c| match c {
            SExpr::Atom(a) => Some(a.as_str()),
            SExpr::List(_) => None,
        })
        .collect();
    let mut layers = stack.pad_layers(&names, report);
    let before = layers.len();
    layers.retain(|l| is_copper(l));
    if layers.len() < before || (layers.is_empty() && !names.is_empty()) {
        let dropped: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| stack.token(n).is_none_or(|t| !is_copper(&t)))
            .collect();
        if !dropped.is_empty() {
            report.note("zone on a non-copper layer dropped from that layer");
        }
    }
    layers
}

/// One `(polygon (pts ..))` as a ring, arcs in it as chords; a corner that
/// does not read is reported and left out.
fn ring_of(polygon: &SExpr, report: &mut Report) -> Vec<Point> {
    let Some(pts) = polygon.find("pts") else {
        return Vec::new();
    };
    let mut unread = 0;
    let ring = ring_points(pts, ARC_STEP_DEG, &mut unread);
    for _ in 0..unread {
        report.note("zone corner that does not read dropped");
    }
    ring
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    fn convert_text(text: &str) -> (Vec<Zone>, Report) {
        let root = sexpr::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (31 "B.Cu" signal)) (net 1 "GND"))"#,
        )
        .expect("parses");
        let mut report = Report::default();
        let stack = Stack::read(&root, &mut report);
        let mut nets = NetTable::read(&root);
        let mut ids = Ids::seeded(text);
        let node = sexpr::parse(text).expect("parses");
        let zones = convert(&node, &stack, &mut nets, &mut ids, &mut report);
        (zones, report)
    }

    #[test]
    fn a_two_layer_pour_becomes_one_zone_per_layer() {
        let (zones, _) = convert_text(
            r#"(zone (net 1) (net_name "GND") (layers "F&B.Cu") (priority 2)
                 (connect_pads (clearance 0.3)) (min_thickness 0.2)
                 (fill yes (thermal_gap 0.4) (thermal_bridge_width 0.35))
                 (polygon (pts (xy 0 0) (xy 10 0) (xy 10 10) (xy 0 10))))"#,
        );
        assert_eq!(zones.len(), 2);
        assert_eq!(zones[0].layer, "TopCopper");
        assert_eq!(zones[1].layer, "BottomCopper");
        assert_ne!(zones[0].uuid, zones[1].uuid);
        let z = &zones[0];
        assert_eq!((z.kind, z.net, z.priority), (ZoneKind::Pour, 1, 2));
        assert_eq!(
            (z.clearance, z.thermal_gap, z.thermal_width),
            (0.3, 0.4, 0.35)
        );
        assert!(z.thermal_relief);
        assert_eq!(z.outline.len(), 4);
    }

    #[test]
    fn a_rule_area_is_a_keepout() {
        let (zones, _) = convert_text(
            r#"(zone (net 0) (net_name "") (layer "F.Cu")
                 (keepout (tracks not_allowed) (vias not_allowed))
                 (polygon (pts (xy 0 0) (xy 1 0) (xy 1 1))))"#,
        );
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].kind, ZoneKind::Keepout);
    }

    #[test]
    fn a_zone_with_a_solid_connection_has_no_thermal_relief() {
        let (zones, _) = convert_text(
            r#"(zone (net 1) (layer "B.Cu") (connect_pads yes (clearance 0.2))
                 (polygon (pts (xy 0 0) (xy 1 0) (xy 1 1))))"#,
        );
        assert!(!zones[0].thermal_relief);
    }

    #[test]
    fn rings_inside_the_outline_are_holes_and_rings_apart_are_zones() {
        let (zones, report) = convert_text(
            r#"(zone (net 1) (layer "F.Cu") (connect_pads no (clearance 0.2))
                 (fill yes (mode hatch))
                 (polygon (pts (xy 2 2) (xy 4 2) (xy 4 4) (xy 2 4)))
                 (polygon (pts (xy 0 0) (xy 10 0) (xy 10 10) (xy 0 10)))
                 (polygon (pts (xy 20 0) (xy 25 0) (xy 25 5))))"#,
        );
        assert_eq!(zones.len(), 2);
        assert_eq!(zones[0].outline.len(), 4);
        assert_eq!(zones[0].outline[1], Point::new(10.0, 0.0));
        assert_eq!(zones[0].holes.len(), 1);
        assert_eq!(zones[0].holes[0][0], Point::new(2.0, 2.0));
        assert!(zones[1].holes.is_empty());
        assert_ne!(zones[0].uuid, zones[1].uuid);
        let summary = report.summary();
        assert!(summary.contains("written as its own zone"), "{summary}");
        assert!(summary.contains("leaves pads unconnected"), "{summary}");
        assert!(
            summary.contains("hatched zone written as a solid pour"),
            "{summary}"
        );
    }
}
