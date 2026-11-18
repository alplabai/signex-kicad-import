// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `.snxpcb` text: a TOML header, the stackup and nets, TSV tables for
//! footprints, pads, tracks and vias, `[[zones]]`, copper arcs under
//! `[free]`, and the `[extras]` tables for what the rows do not carry.
//! Lengths in TSV cells and under `[free]` are whole nanometres; lengths
//! in TOML tables are millimetres.

use crate::model::{
    Board, BoardGraphic, BoardText, Footprint, FootprintGraphic, Pad, PadShape, Point, Property,
    TrackArc, Zone,
};
use crate::text::{
    nm, number_cell, toml_float, toml_point, toml_points, toml_string, tsv_cell, tsv_section,
};

/// The format token this writer produces.
pub const FORMAT_TOKEN: &str = "snxpcb/1.4";

/// The `generator` header value.
pub const GENERATOR: &str = "signex-import";

/// The board as `.snxpcb` text.
pub fn write_board(board: &Board) -> String {
    let mut out = header(board);
    out.push_str(&stackup(board));
    out.push_str(&nets(board));
    out.push_str(&footprint_table(&board.footprints));
    out.push_str(&pad_table(&board.footprints));
    out.push_str(&track_table(board));
    out.push_str(&via_table(board));
    for zone in &board.zones {
        out.push_str(&zone_table(zone));
    }
    for arc in &board.arcs {
        out.push_str(&arc_table(arc));
    }
    out.push_str(&footprint_extras(&board.footprints));
    out.push_str(&pad_extras(&board.footprints));
    out.push_str(&board_extras(board));
    out
}

fn header(board: &Board) -> String {
    format!(
        "format = {}\npcb_id = {}\nversion = 1\ngenerator = {}\nthickness = {}\n",
        toml_string(FORMAT_TOKEN),
        toml_string(&board.uuid.to_string()),
        toml_string(GENERATOR),
        toml_float(board.thickness),
    )
}

fn string_array(items: impl Iterator<Item = String>) -> String {
    let quoted: Vec<String> = items.map(|s| toml_string(&s)).collect();
    format!("[{}]", quoted.join(", "))
}

fn stackup(board: &Board) -> String {
    let mut out = String::from("\n[stackup]\n");
    out.push_str(&format!(
        "layers = {}\n",
        string_array(board.layers.iter().map(|l| l.token.clone()))
    ));
    if board.layers.iter().any(|l| !l.layer_type.is_empty()) {
        out.push_str(&format!(
            "layer_types = {}\n",
            string_array(board.layers.iter().map(|l| l.layer_type.clone()))
        ));
    }
    let s = &board.setup;
    out.push_str("\n[stackup.setup]\n");
    for (key, value) in [
        ("grid_size", s.grid_size),
        ("trace_width", s.trace_width),
        ("via_diameter", s.via_diameter),
        ("via_drill", s.via_drill),
        ("clearance", s.clearance),
        ("track_min_width", s.track_min_width),
        ("via_min_diameter", s.via_min_diameter),
        ("via_min_drill", s.via_min_drill),
    ] {
        out.push_str(&format!("{key} = {}\n", toml_float(value)));
    }
    out
}

fn nets(board: &Board) -> String {
    if board.nets.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n[nets]\nentries = [\n");
    for net in &board.nets {
        out.push_str(&format!(
            "    {{ number = {}, name = {} }},\n",
            net.number,
            toml_string(&net.name)
        ));
    }
    out.push_str("]\n");
    out
}

fn header_cells(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

fn footprint_table(footprints: &[Footprint]) -> String {
    let header = header_cells(&[
        "uuid", "ref", "library", "pos_x", "pos_y", "rotation", "layer", "value",
    ]);
    let rows: Vec<Vec<String>> = footprints
        .iter()
        .map(|fp| {
            vec![
                fp.uuid.to_string(),
                tsv_cell(&fp.reference),
                tsv_cell(&fp.footprint_id),
                nm(fp.position.x).to_string(),
                nm(fp.position.y).to_string(),
                number_cell(fp.rotation),
                tsv_cell(&fp.layer),
                tsv_cell(&fp.value),
            ]
        })
        .collect();
    tsv_section("footprints", &header, &rows)
}

/// The corners of a custom pad as one cell, `<x>,<y>;..` in whole nm.
fn outline_cell(corners: &[Point]) -> String {
    let parts: Vec<String> = corners
        .iter()
        .map(|p| format!("{},{}", nm(p.x), nm(p.y)))
        .collect();
    parts.join(";")
}

fn pad_table(footprints: &[Footprint]) -> String {
    let pads = || footprints.iter().flat_map(|fp| fp.pads.iter().map(move |p| (fp, p)));
    let has_rotation = pads().any(|(_, p)| p.rotation != 0.0);
    let has_outline = pads().any(|(_, p)| matches!(p.shape, PadShape::Custom(_)));
    let mut names = vec![
        "uuid",
        "footprint_ref",
        "pin",
        "pos_x",
        "pos_y",
        "size_x",
        "size_y",
        "pad_type",
        "shape",
        "layers",
        "drill",
        "net_number",
        "net_name",
        "roundrect_ratio",
    ];
    if has_rotation {
        names.push("rotation");
    }
    if has_outline {
        names.push("custom_outline");
    }
    let rows: Vec<Vec<String>> = pads()
        .map(|(fp, pad)| pad_row(fp, pad, has_rotation, has_outline))
        .collect();
    tsv_section("pads", &header_cells(&names), &rows)
}

fn pad_row(fp: &Footprint, pad: &Pad, has_rotation: bool, has_outline: bool) -> Vec<String> {
    let ratio = match pad.shape {
        PadShape::RoundRect(r) => r.clamp(0.0, 0.5),
        _ => 0.0,
    };
    let (net_number, net_name) = match &pad.net {
        Some(net) => (net.number, net.name.as_str()),
        None => (0, ""),
    };
    let mut row = vec![
        pad.uuid.to_string(),
        fp.uuid.to_string(),
        tsv_cell(&pad.number),
        nm(pad.position.x).to_string(),
        nm(pad.position.y).to_string(),
        nm(pad.size.x).to_string(),
        nm(pad.size.y).to_string(),
        pad.pad_type.token().to_string(),
        pad.shape.token().to_string(),
        tsv_cell(&pad.layers.join(",")),
        pad.drill.map_or(0, nm).to_string(),
        net_number.to_string(),
        tsv_cell(net_name),
        number_cell(ratio),
    ];
    if has_rotation {
        row.push(if pad.rotation == 0.0 {
            tsv_cell("")
        } else {
            number_cell(pad.rotation)
        });
    }
    if has_outline {
        row.push(match &pad.shape {
            PadShape::Custom(corners) => tsv_cell(&outline_cell(corners)),
            _ => tsv_cell(""),
        });
    }
    row
}

fn track_table(board: &Board) -> String {
    let header = header_cells(&[
        "uuid", "net", "layer", "width", "start_x", "start_y", "end_x", "end_y",
    ]);
    let rows: Vec<Vec<String>> = board
        .tracks
        .iter()
        .map(|t| {
            vec![
                t.uuid.to_string(),
                t.net.to_string(),
                tsv_cell(&t.layer),
                nm(t.width).to_string(),
                nm(t.start.x).to_string(),
                nm(t.start.y).to_string(),
                nm(t.end.x).to_string(),
                nm(t.end.y).to_string(),
            ]
        })
        .collect();
    tsv_section("tracks", &header, &rows)
}

fn via_table(board: &Board) -> String {
    let header = header_cells(&[
        "uuid", "net", "pos_x", "pos_y", "drill", "diameter", "layers", "via_type",
    ]);
    let rows: Vec<Vec<String>> = board
        .vias
        .iter()
        .map(|v| {
            vec![
                v.uuid.to_string(),
                v.net.to_string(),
                nm(v.position.x).to_string(),
                nm(v.position.y).to_string(),
                nm(v.drill).to_string(),
                nm(v.diameter).to_string(),
                tsv_cell(&v.layers.join(",")),
                v.via_type.token().to_string(),
            ]
        })
        .collect();
    tsv_section("vias", &header, &rows)
}

fn zone_table(zone: &Zone) -> String {
    let mut out = String::from("\n[[zones]]\n");
    out.push_str(&format!("uuid = {}\n", toml_string(&zone.uuid.to_string())));
    out.push_str(&format!("net = {}\n", zone.net));
    out.push_str(&format!("net_name = {}\n", toml_string(&zone.net_name)));
    out.push_str(&format!("layer = {}\n", toml_string(&zone.layer)));
    out.push_str(&format!("outline = {}\n", toml_points(&zone.outline)));
    out.push_str(&format!("priority = {}\n", zone.priority));
    out.push_str(&format!(
        "fill_type = {}\n",
        toml_string(zone.kind.fill_type())
    ));
    out.push_str(&format!("thermal_relief = {}\n", zone.thermal_relief));
    for (key, value) in [
        ("thermal_gap", zone.thermal_gap),
        ("thermal_width", zone.thermal_width),
        ("clearance", zone.clearance),
        ("min_thickness", zone.min_thickness),
    ] {
        out.push_str(&format!("{key} = {}\n", toml_float(value)));
    }
    if zone.min_island_area > 0.0 {
        out.push_str(&format!(
            "min_island_area = {}\n",
            toml_float(zone.min_island_area)
        ));
    }
    if zone.remove_dead_islands {
        out.push_str("remove_dead_islands = true\n");
    }
    out
}

fn arc_table(arc: &TrackArc) -> String {
    format!(
        "\n[[free.arcs]]\nuuid = {}\nstart_x = {}\nstart_y = {}\nmid_x = {}\nmid_y = {}\n\
         end_x = {}\nend_y = {}\nwidth = {}\nlayer = {}\nnet = {}\n",
        toml_string(&arc.uuid.to_string()),
        nm(arc.start.x),
        nm(arc.start.y),
        nm(arc.mid.x),
        nm(arc.mid.y),
        nm(arc.end.x),
        nm(arc.end.y),
        nm(arc.width),
        toml_string(&arc.layer),
        arc.net,
    )
}

/// `key = value` pairs as an inline table.
fn inline_table(pairs: &[(&str, String)]) -> String {
    let parts: Vec<String> = pairs.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    format!("{{ {} }}", parts.join(", "))
}

/// A multi-line array of inline tables, one per line.
fn table_array(key: &str, items: &[String]) -> String {
    let mut out = format!("{key} = [\n");
    for item in items {
        out.push_str(&format!("    {item},\n"));
    }
    out.push_str("]\n");
    out
}

fn optional_point(pairs: &mut Vec<(&str, String)>, key: &'static str, p: Option<Point>) {
    if let Some(p) = p {
        pairs.push((key, toml_point(p)));
    }
}

fn footprint_graphic(g: &FootprintGraphic) -> String {
    let mut pairs = vec![
        ("graphic_type", toml_string(&g.graphic_type)),
        ("layer", toml_string(&g.layer)),
        ("width", toml_float(g.width)),
    ];
    optional_point(&mut pairs, "start", g.start);
    optional_point(&mut pairs, "end", g.end);
    optional_point(&mut pairs, "center", g.center);
    optional_point(&mut pairs, "mid", g.mid);
    if g.radius != 0.0 {
        pairs.push(("radius", toml_float(g.radius)));
    }
    if !g.points.is_empty() {
        pairs.push(("points", toml_points(&g.points)));
    }
    if !g.text.is_empty() {
        pairs.push(("text", toml_string(&g.text)));
        pairs.push(("font_size", toml_float(g.font_size)));
    }
    optional_point(&mut pairs, "position", g.position);
    if g.rotation != 0.0 {
        pairs.push(("rotation", toml_float(g.rotation)));
    }
    if !g.fill.is_empty() {
        pairs.push(("fill", toml_string(&g.fill)));
    }
    inline_table(&pairs)
}

fn property(p: &Property) -> String {
    let mut pairs = vec![
        ("key", toml_string(&p.key)),
        ("value", toml_string(&p.value)),
    ];
    optional_point(&mut pairs, "position", p.position);
    pairs.push(("rotation", toml_float(p.rotation)));
    if let Some(layer) = &p.layer {
        pairs.push(("layer", toml_string(layer)));
    }
    if let Some(size) = p.font_size {
        pairs.push(("font_size", toml_float(size)));
    }
    pairs.push(("hidden", p.hidden.to_string()));
    inline_table(&pairs)
}

fn footprint_extras(footprints: &[Footprint]) -> String {
    let mut out = String::new();
    for fp in footprints {
        out.push_str(&format!("\n[extras.footprints.{}]\n", fp.uuid));
        out.push_str(&format!(
            "footprint_id = {}\n",
            toml_string(&fp.footprint_id)
        ));
        if fp.locked {
            out.push_str("locked = true\n");
        }
        if !fp.graphics.is_empty() {
            let items: Vec<String> = fp.graphics.iter().map(footprint_graphic).collect();
            out.push_str(&table_array("graphics", &items));
        }
        if !fp.properties.is_empty() {
            let items: Vec<String> = fp.properties.iter().map(property).collect();
            out.push_str(&table_array("properties", &items));
        }
    }
    out
}

fn pad_extras(footprints: &[Footprint]) -> String {
    let mut out = String::new();
    for pad in footprints.iter().flat_map(|fp| &fp.pads) {
        if !pad.drill_shape.is_empty() {
            out.push_str(&format!(
                "\n[extras.pads.{}]\ndrill_shape = {}\n",
                pad.uuid,
                toml_string(&pad.drill_shape)
            ));
        }
    }
    out
}

fn board_graphic(g: &BoardGraphic) -> String {
    let mut pairs = vec![
        ("uuid", toml_string(&g.uuid.to_string())),
        ("graphic_type", toml_string(&g.graphic_type)),
        ("layer", toml_string(&g.layer)),
        ("width", toml_float(g.width)),
    ];
    optional_point(&mut pairs, "start", g.start);
    optional_point(&mut pairs, "end", g.end);
    optional_point(&mut pairs, "center", g.center);
    pairs.push(("radius", toml_float(g.radius)));
    pairs.push(("points", toml_points(&g.points)));
    inline_table(&pairs)
}

fn board_text(t: &BoardText) -> String {
    inline_table(&[
        ("uuid", toml_string(&t.uuid.to_string())),
        ("text", toml_string(&t.text)),
        ("position", toml_point(t.position)),
        ("layer", toml_string(&t.layer)),
        ("font_size", toml_float(t.font_size)),
        ("rotation", toml_float(t.rotation)),
    ])
}

fn board_extras(board: &Board) -> String {
    if board.outline.is_empty() && board.graphics.is_empty() && board.texts.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n[extras.board]\n");
    out.push_str(&format!("outline = {}\n", toml_points(&board.outline)));
    let graphics: Vec<String> = board.graphics.iter().map(board_graphic).collect();
    out.push_str(&table_array("graphics", &graphics));
    let texts: Vec<String> = board.texts.iter().map(board_text).collect();
    out.push_str(&table_array("texts", &texts));
    out
}

#[cfg(test)]
#[path = "pcb_tests.rs"]
mod tests;
