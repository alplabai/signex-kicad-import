// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! A `.kicad_pcb` into the `snx-writer` board model.

use snx_writer::model::{
    Board, BoardGraphic, NetClass, Point, Setup, Track, TrackArc, Via, ViaType,
};

use crate::board_graphics;
use crate::error::ParseError;
use crate::footprint::{self, Context};
use crate::geometry::{at_of, number_of, point_of};
use crate::ids::Ids;
use crate::layers::{Stack, is_copper};
use crate::nets::NetTable;
use crate::outline::{self, EdgePiece};
use crate::project::ProjectRules;
use crate::report::Report;
use crate::sexpr::{self, Atom, SExpr};
use crate::zones;

/// A converted board with the net classes for its project sidecar.
#[derive(Debug, Clone)]
pub struct Converted {
    pub board: Board,
    pub classes: Vec<NetClass>,
    pub report: Report,
}

/// Convert `.kicad_pcb` text; `rules` are the project file's net classes
/// and board rules when there is one.
pub fn convert_pcb(text: &str, rules: Option<&ProjectRules>) -> Result<Converted, ParseError> {
    let root = sexpr::parse(text)?;
    if root.keyword() != Some("kicad_pcb") {
        return Err(ParseError::InvalidSExpr(
            "not a KiCad board file".to_string(),
        ));
    }
    let mut report = Report::default();
    let mut ids = Ids::seeded(text);
    let mut nets = NetTable::read(&root);
    let stack = Stack::read(&root, &mut report);

    let footprints: Vec<_> = {
        let mut cx = Context {
            stack: &stack,
            nets: &mut nets,
            ids: &mut ids,
            report: &mut report,
        };
        root.children()
            .iter()
            .filter(|n| matches!(n.keyword(), Some("footprint" | "module")))
            .map(|n| footprint::convert(n, &mut cx))
            .collect()
    };
    let tracks = tracks(&root, &stack, &mut nets, &mut ids, &mut report);
    let arcs = arcs(&root, &stack, &mut nets, &mut ids, &mut report);
    let vias = vias(&root, &stack, &mut nets, &mut ids, &mut report);
    let mut zones: Vec<_> = root
        .find_all("zone")
        .into_iter()
        .flat_map(|z| zones::convert(z, &stack, &mut nets, &mut ids, &mut report))
        .collect();
    let drawings = board_graphics::read(&root, &stack, &mut nets, &mut ids, &mut report);
    zones.extend(drawings.solids);
    let (ring, edge_graphics) = edge(&drawings.edge_pieces, drawings.edge_graphics, &mut report);
    let mut graphics = drawings.graphics;
    graphics.extend(edge_graphics);

    let thickness = root
        .find("general")
        .and_then(|g| number_of(g, "thickness"))
        .unwrap_or(1.6);
    let classes = match rules {
        Some(r) => r.classes.clone(),
        None => board_classes(&root),
    };
    let board = Board {
        uuid: ids.fresh(),
        thickness,
        layers: stack.layers.clone(),
        setup: rules.map_or_else(Setup::default, |r| r.setup),
        nets: nets.nets(),
        footprints,
        tracks,
        arcs,
        vias,
        zones,
        outline: ring,
        graphics,
        texts: drawings.texts,
        origin: origin_of(&root),
    };
    Ok(Converted {
        board,
        classes,
        report,
    })
}

/// The board's own origin: the auxiliary (drill and place) origin when the
/// board sets one, else the grid origin; `None` when neither is set.
fn origin_of(root: &SExpr) -> Option<Point> {
    let setup = root.find("setup")?;
    ["aux_axis_origin", "grid_origin"]
        .iter()
        .filter_map(|key| point_of(setup, key))
        .find(|p| p.x != 0.0 || p.y != 0.0)
}

/// The outline ring and the edge drawings that are not part of it.
fn edge(
    pieces: &[EdgePiece],
    graphics: Vec<BoardGraphic>,
    report: &mut Report,
) -> (Vec<Point>, Vec<BoardGraphic>) {
    let outline = outline::build(pieces);
    if outline.ring.is_empty() {
        report.note("board edge does not close; no outline written");
    }
    let rest = graphics
        .into_iter()
        .zip(outline.used)
        .filter(|(_, used)| !used)
        .map(|(g, _)| g)
        .collect();
    (outline.ring, rest)
}

/// The copper layer an item names, or `None` (reported) when it is not one.
fn copper_layer(node: &SExpr, stack: &Stack, what: &str, report: &mut Report) -> Option<String> {
    let name = node.find("layer").and_then(SExpr::first_arg).unwrap_or("");
    match stack.token(name) {
        Some(t) if is_copper(&t) => Some(t),
        _ => {
            report.note(format!("{what} on non-copper layer {name} dropped"));
            None
        }
    }
}

fn tracks(
    root: &SExpr,
    stack: &Stack,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
) -> Vec<Track> {
    let mut out = Vec::new();
    for node in root.find_all("segment") {
        let (Some(start), Some(end)) = (point_of(node, "start"), point_of(node, "end")) else {
            report.note("track without its ends dropped");
            continue;
        };
        let Some(layer) = copper_layer(node, stack, "track", report) else {
            continue;
        };
        out.push(Track {
            uuid: ids.of(node, report),
            net: nets.of(node),
            layer,
            width: number_of(node, "width").unwrap_or(0.25),
            start,
            end,
        });
    }
    out
}

fn arcs(
    root: &SExpr,
    stack: &Stack,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
) -> Vec<TrackArc> {
    let mut out = Vec::new();
    for node in root.find_all("arc") {
        let (Some(start), Some(mid), Some(end)) = (
            point_of(node, "start"),
            point_of(node, "mid"),
            point_of(node, "end"),
        ) else {
            report.note("copper arc without its three points dropped");
            continue;
        };
        let Some(layer) = copper_layer(node, stack, "copper arc", report) else {
            continue;
        };
        out.push(TrackArc {
            uuid: ids.of(node, report),
            net: nets.of(node),
            layer,
            width: number_of(node, "width").unwrap_or(0.25),
            start,
            mid,
            end,
        });
    }
    out
}

fn vias(
    root: &SExpr,
    stack: &Stack,
    nets: &mut NetTable,
    ids: &mut Ids,
    report: &mut Report,
) -> Vec<Via> {
    let copper = stack.copper();
    let outer = vec![
        copper.first().cloned().unwrap_or_default(),
        copper.last().cloned().unwrap_or_default(),
    ];
    let mut out = Vec::new();
    for node in root.find_all("via") {
        let flag = |f: &str| {
            node.children()
                .iter()
                .any(|c| matches!(c, SExpr::Atom(Atom::Raw(a)) if a == f))
        };
        let via_type = if flag("micro") {
            ViaType::Micro
        } else if flag("blind") {
            ViaType::Blind
        } else {
            ViaType::Through
        };
        let named: Vec<String> = node
            .find("layers")
            .map(|l| {
                l.children()
                    .iter()
                    .filter_map(|c| match c {
                        SExpr::Atom(a) => stack.token(a.as_str()).filter(|t| is_copper(t)),
                        SExpr::List(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let layers = if via_type == ViaType::Through || named.len() < 2 {
            outer.clone()
        } else {
            named
        };
        let (position, _) = at_of(node);
        out.push(Via {
            uuid: ids.of(node, report),
            net: nets.of(node),
            position,
            drill: number_of(node, "drill").unwrap_or(0.3),
            diameter: number_of(node, "size").unwrap_or(0.6),
            layers,
            via_type,
        });
    }
    out
}

/// Net classes kept in the board file itself (older files):
/// `(net_class "<name>" "<description>" (clearance ..) (trace_width ..)
/// (via_dia ..) (via_drill ..) (add_net "<net>") ..)`.
fn board_classes(root: &SExpr) -> Vec<NetClass> {
    let mut classes: Vec<NetClass> = root
        .find_all("net_class")
        .into_iter()
        .filter_map(|node| {
            let value = |key: &str| number_of(node, key).filter(|v| *v > 0.0).unwrap_or(0.0);
            Some(NetClass {
                name: node.first_arg()?.to_string(),
                nets: node
                    .find_all("add_net")
                    .into_iter()
                    .filter_map(SExpr::first_arg)
                    .map(str::to_string)
                    .collect(),
                patterns: Vec::new(),
                clearance: value("clearance"),
                trace_width: value("trace_width"),
                via_diameter: value("via_dia"),
                via_drill: value("via_drill"),
                diff_pair_gap: value("diff_pair_gap"),
                diff_pair_width: value("diff_pair_width"),
            })
        })
        .collect();
    if let Some(i) = classes.iter().position(|c| c.name == "Default") {
        let mut default = classes.remove(i);
        default.nets.clear();
        default.patterns = vec!["*".to_string()];
        classes.push(default);
    }
    classes
}

#[cfg(test)]
#[path = "pcb_tests.rs"]
mod tests;
