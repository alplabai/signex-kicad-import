// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Standalone KiCad footprints (`.kicad_mod`, `*.pretty`) as Signex library
//! footprints (`.snxfpt`).
//!
//! The footprint is converted by the board converter's own footprint code
//! (`footprint::convert`) in a library context: no board, so no nets, a
//! default layer stack (`Stack::library`) and a footprint placed at the
//! origin, top side, angle 0. That keeps one rule set for pads, shapes and
//! angles. The frame needs no flip: a KiCad footprint and a Signex footprint
//! are both +y down (`LIBRARY_FORMAT.md` section 4), and an angle in the
//! source turns counter-clockwise on screen while Signex turns from +x
//! towards +y, which the converter's `own_turn` already negates.

use std::path::Path;

use serde_json::json;
use snx_writer::canon::cell_number;
use snx_writer::footprint_file::{self as out, FootprintFile};
use snx_writer::model::{
    Footprint as BoardFootprint, FootprintGraphic, Pad as BoardPad, PadShape, PadType,
};

use crate::error::ParseError;
use crate::footprint::{Context, convert};
use crate::geometry::number_of;
use crate::ids::Ids;
use crate::items::{Item, ItemKind, Severity};
use crate::layers::Stack;
use crate::models::ModelCx;
use crate::nets::NetTable;
use crate::report::Report;
use crate::sexpr::{self, SExpr};
use crate::symlib::to_snx::EPOCH;

/// Footprints read from a library, as envelopes, and the report items.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FootprintLibRead {
    pub files: Vec<FootprintFile>,
    /// Footprints found in the source, written or not.
    pub found: usize,
    pub items: Vec<Item>,
}

/// The uuid of a footprint (`false`) or of its envelope (`true`) from its name.
pub type IdOf<'a> = &'a dyn Fn(bool, &str) -> String;

/// Read every `*.kicad_mod` of a `.pretty` folder, by file name.
pub fn read_pretty_dir(
    dir: &Path,
    ids: IdOf<'_>,
    models: &ModelCx,
) -> Result<FootprintLibRead, ParseError> {
    let library = dir
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("footprints")
        .to_string();
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut read = FootprintLibRead::default();
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("kicad_mod"))
        {
            read.items.push(Item::new(
                ItemKind::Library,
                &library,
                "footprint.file_ignored",
                Severity::Info,
                format!("{name} is not a .kicad_mod file; ignored"),
            ));
            continue;
        }
        read_file_into(&path, &library, ids, models, &mut read);
    }
    Ok(read)
}

/// Read one `.kicad_mod`; `library` names it in report items.
pub fn read_footprint_file(
    path: &Path,
    library: &str,
    ids: IdOf<'_>,
    models: &ModelCx,
) -> FootprintLibRead {
    let mut read = FootprintLibRead::default();
    read_file_into(path, library, ids, models, &mut read);
    read
}

fn read_file_into(
    path: &Path,
    library: &str,
    ids: IdOf<'_>,
    models: &ModelCx,
    read: &mut FootprintLibRead,
) {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let text = match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.contains('\u{fffd}') && std::str::from_utf8(&bytes).is_err() {
                read.items.push(Item::new(
                    ItemKind::Footprint,
                    format!("{library}:{name}"),
                    "footprint.invalid_utf8",
                    Severity::Approximated,
                    format!("{name} is not valid UTF-8; unreadable bytes replaced"),
                ));
            }
            text
        }
        Err(e) => {
            read.found += 1;
            read.items.push(unreadable(library, &name, &e.to_string()));
            return;
        }
    };
    read.found += 1;
    let dir = path.parent().unwrap_or(Path::new("."));
    match convert_footprint(&text, library, ids, &mut read.items, models, dir) {
        Ok(file) => {
            if read
                .files
                .iter()
                .any(|f| f.footprint.name == file.footprint.name)
            {
                read.items.push(Item::new(
                    ItemKind::Footprint,
                    format!("{library}:{}", file.footprint.name),
                    "footprint.duplicate_name",
                    Severity::Error,
                    format!(
                        "footprint {} appears again ({name}); the later one is not read",
                        file.footprint.name
                    ),
                ));
            } else {
                read.files.push(file);
            }
        }
        Err(e) => read.items.push(unreadable(library, &name, &e.to_string())),
    }
}

fn unreadable(library: &str, file: &str, why: &str) -> Item {
    Item::new(
        ItemKind::Footprint,
        format!("{library}:{file}"),
        "footprint.unreadable",
        Severity::Error,
        format!("{file}: {why}; footprint not read"),
    )
    .with_detail(json!({ "file": file }))
}

/// Convert the text of one `.kicad_mod`.
pub fn convert_footprint(
    text: &str,
    library: &str,
    ids: IdOf<'_>,
    items: &mut Vec<Item>,
    models: &ModelCx,
    fp_dir: &Path,
) -> Result<FootprintFile, ParseError> {
    let mut root = sexpr::parse(text)?;
    if !matches!(root.keyword(), Some("footprint" | "module")) {
        return Err(ParseError::InvalidValue(format!(
            "not a footprint (starts with {})",
            root.keyword().unwrap_or("a value")
        )));
    }
    // A library footprint has no `(at ..)`; the board converter wants one.
    if root.find("at").is_none()
        && let SExpr::List(children) = &mut root
    {
        children.push(sexpr::parse("(at 0 0)")?);
    }
    // A footprint drawn on the bottom side is stored flipped; turning it back
    // to a library-side-up top footprint needs the pad and text turns of a
    // mirrored footprint, which the format's frame rules do not pin down for
    // a standalone file. It is refused rather than guessed.
    if root
        .find("layer")
        .and_then(SExpr::first_arg)
        .is_some_and(|l| matches!(l, "B.Cu" | "B.Cu "))
    {
        return Err(ParseError::InvalidValue(
            "footprint on the bottom side (B.Cu) is not converted; save it on F.Cu".into(),
        ));
    }
    let name = root.first_arg().unwrap_or("").to_string();
    if name.is_empty() {
        return Err(ParseError::MissingField("footprint name".into()));
    }
    let source = format!("{library}:{name}");
    let mut report = Report::default();
    let stack = Stack::library(&mut report);
    let mut nets = NetTable::default();
    let mut id_seq = Ids::seeded(text);
    let converted = {
        let mut cx = Context {
            stack: &stack,
            nets: &mut nets,
            ids: &mut id_seq,
            report: &mut report,
        };
        convert(&root, &mut cx)
    };
    let board =
        converted.ok_or_else(|| ParseError::InvalidValue("footprint does not read".into()))?;
    let mut sink = Sink {
        items,
        counts: Vec::new(),
    };
    // Board messages become items; the model note is replaced by the deferred one.
    for item in report.items(ItemKind::Footprint, &source) {
        if !matches!(
            item.code.as_str(),
            "footprint.model_dropped" | "pad.slot_approximated"
        ) {
            sink.items.push(item);
        }
    }
    let embedded =
        crate::models::attach(&root.find_all("model"), fp_dir, models, &source, sink.items);
    if !root.find_all("zone").is_empty() {
        sink.note(
            "footprint.zone_dropped",
            Severity::Dropped,
            "footprint zone dropped; the library footprint holds no copper pours",
        );
    }
    let margins = Margins {
        mask: number_of(&root, "solder_mask_margin"),
        paste: number_of(&root, "solder_paste_margin"),
    };
    let ratio_pads = root
        .find_all("pad")
        .into_iter()
        .filter(|p| {
            p.find("solder_paste_margin_ratio").is_some()
                || root.find("solder_paste_ratio").is_some()
        })
        .count();
    for _ in 0..ratio_pads {
        sink.note(
            "pad.paste_ratio_not_carried",
            Severity::Approximated,
            "paste ratio has no Signex field; the paste aperture is the pad size",
        );
    }
    let mut f = adapt(&board, margins, &mut sink);
    let (step, model_list) = match embedded {
        Some((a, m)) => (Some(a), vec![m]),
        None => (None, Vec::new()),
    };
    f.step = step;
    f.uuid = ids(false, &name);
    f.name = name.clone();
    let descr = root.find("descr").and_then(SExpr::first_arg).unwrap_or("");
    let tags = root.find("tags").and_then(SExpr::first_arg).unwrap_or("");
    f.description = match (descr.is_empty(), tags.is_empty()) {
        (_, true) => descr.to_string(),
        (true, false) => format!("(tags: {tags})"),
        (false, false) => format!("{descr} (tags: {tags})"),
    };
    if !tags.is_empty() {
        sink.note(
            "footprint.tags_in_description",
            Severity::Approximated,
            "tags have no field; written at the end of the description",
        );
    }
    let reference = root
        .find_all("property")
        .into_iter()
        .find(|p| p.first_arg() == Some("Reference"))
        .and_then(|p| p.arg(1))
        .or_else(|| {
            root.find_all("fp_text")
                .into_iter()
                .find(|t| t.first_arg() == Some("reference"))
                .and_then(|t| t.arg(1))
        })
        .unwrap_or("");
    let prefix: String = reference
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if !prefix.is_empty() && !prefix.eq_ignore_ascii_case("ref") {
        f.default_designator = format!("{prefix}?");
    }
    apply_attr(&root, &mut f, &mut sink);
    sink.flush(&source);
    Ok(FootprintFile {
        file_uuid: ids(true, &name),
        display_name: name,
        created: EPOCH.to_string(),
        updated: EPOCH.to_string(),
        footprint: f,
        models: model_list,
    })
}

/// `(attr ..)` words: `exclude_from_bom` makes the part a no-BOM component;
/// the placement and board flags have no field.
fn apply_attr(root: &SExpr, f: &mut out::Footprint, sink: &mut Sink<'_>) {
    let Some(attr) = root.find("attr") else {
        return;
    };
    for word in attr.children().iter().filter_map(|c| match c {
        SExpr::Atom(a) => Some(a.as_str()),
        SExpr::List(_) => None,
    }) {
        match word {
            "smd" | "through_hole" => {}
            "exclude_from_bom" => f.component_type = Some("StandardNoBom".to_string()),
            _ => sink.note(
                "footprint.attr_not_carried",
                Severity::Approximated,
                "footprint attribute has no Signex field",
            ),
        }
    }
}

/// Items of one footprint, counted by code.
struct Sink<'a> {
    items: &'a mut Vec<Item>,
    counts: Vec<(&'static str, Severity, &'static str, usize)>,
}

impl Sink<'_> {
    fn note(&mut self, code: &'static str, severity: Severity, message: &'static str) {
        match self.counts.iter_mut().find(|c| c.0 == code) {
            Some(c) => c.3 += 1,
            None => self.counts.push((code, severity, message, 1)),
        }
    }

    /// Like `note`, but one item per footprint however many pads.
    fn note_once(&mut self, code: &'static str, severity: Severity, message: &'static str) {
        if !self.counts.iter().any(|c| c.0 == code) {
            self.counts.push((code, severity, message, 1));
        }
    }

    fn flush(&mut self, source: &str) {
        for (code, severity, message, count) in self.counts.drain(..) {
            self.items.push(
                Item::new(ItemKind::Footprint, source, code, severity, message)
                    .with_detail(json!({ "count": count })),
            );
        }
    }
}

/// The footprint-level margins every pad without its own one inherits.
#[derive(Clone, Copy)]
struct Margins {
    mask: Option<f64>,
    paste: Option<f64>,
}

fn adapt(b: &BoardFootprint, margins: Margins, sink: &mut Sink<'_>) -> out::Footprint {
    let mut f = out::Footprint {
        uuid: String::new(),
        name: String::new(),
        pads: b
            .pads
            .iter()
            .filter_map(|p| pad(p, margins, sink))
            .collect(),
        silk_f: vec![],
        silk_b: vec![],
        fab_f: vec![],
        fab_b: vec![],
        version: "1.0.0".to_string(),
        released: false,
        created: EPOCH.to_string(),
        updated: EPOCH.to_string(),
        description: String::new(),
        default_designator: String::new(),
        component_type: None,
        height_mm: None,
        courtyard: None,
        body: out::Body3d::default(),
        step: None,
    };
    let mut courtyard: Vec<&FootprintGraphic> = Vec::new();
    for g in &b.graphics {
        let list = match g.layer.as_str() {
            "TopSilkscreen" => &mut f.silk_f,
            "BottomSilkscreen" => &mut f.silk_b,
            "TopFab" => &mut f.fab_f,
            "BottomFab" => &mut f.fab_b,
            "TopCourtyard" => {
                courtyard.push(g);
                continue;
            }
            _ => {
                sink.note(
                    "footprint.graphic_layer_dropped",
                    Severity::Dropped,
                    "drawing on a layer the library footprint does not keep dropped",
                );
                continue;
            }
        };
        match graphic(g, sink) {
            Some(g) => list.push(g),
            None => sink.note(
                "footprint.graphic_unreadable",
                Severity::Dropped,
                "drawing without its geometry dropped",
            ),
        }
    }
    if !courtyard.is_empty() {
        f.courtyard = Some(courtyard_polygon(&courtyard, sink));
    }
    f
}

fn p2(p: snx_writer::model::Point) -> [f64; 2] {
    [p.x, p.y]
}

fn shape_token(shape: &PadShape) -> String {
    match shape {
        PadShape::Circle => "round".to_string(),
        PadShape::Rect => "rect".to_string(),
        PadShape::Oval => "oval".to_string(),
        PadShape::RoundRect(r) => format!("round_rect:{}", cell_number(*r)),
        PadShape::Trapezoid(d) => format!("trapezoid:{},{}", cell_number(d.x), cell_number(d.y)),
        PadShape::Custom(points) => {
            let corners: Vec<String> = points
                .iter()
                .map(|p| format!("{},{}", cell_number(p.x), cell_number(p.y)))
                .collect();
            format!("custom:{}", corners.join("|"))
        }
    }
}

/// The pad's layer tokens in the library's named form and order (copper,
/// mask, paste: `LIBRARY_FORMAT.md` 4.2). A non-plated hole keeps no copper.
fn pad_layers(p: &BoardPad, sink: &mut Sink<'_>) -> Vec<String> {
    let has = |t: &str| p.layers.iter().any(|l| l == t);
    let npth = p.pad_type == PadType::NpThru;
    let mut out: Vec<String> = Vec::new();
    let wanted = [
        ("TopCopper", "Top Layer", true),
        ("BottomCopper", "Bottom Layer", true),
    ];
    for (token, name, copper) in wanted {
        if has(token) && !(copper && npth) {
            out.push(name.to_string());
        }
    }
    if !npth {
        out.extend(
            p.layers
                .iter()
                .filter(|l| l.starts_with("Inner") && l.ends_with("Copper"))
                .cloned(),
        );
    }
    let rest = [
        ("TopSolderMask", "Top Solder", false),
        ("BottomSolderMask", "Bottom Solder", false),
        ("TopPaste", "Top Paste", true),
        ("BottomPaste", "Bottom Paste", true),
        ("TopSilkscreen", "Top Overlay", false),
        ("BottomSilkscreen", "Bottom Overlay", false),
    ];
    for (token, name, drops_on_npth) in rest {
        if has(token) && !(drops_on_npth && npth) {
            out.push(name.to_string());
        }
    }
    if npth && (has("TopCopper") || has("BottomCopper")) {
        sink.note(
            "pad.npth_copper_dropped",
            Severity::Approximated,
            "copper on a non-plated hole dropped; the format keeps mask layers only",
        );
    }
    let known = [
        "TopCopper",
        "BottomCopper",
        "TopSolderMask",
        "BottomSolderMask",
        "TopPaste",
        "BottomPaste",
        "TopSilkscreen",
        "BottomSilkscreen",
    ];
    if p.layers
        .iter()
        .any(|l| !known.contains(&l.as_str()) && !(l.starts_with("Inner") && l.ends_with("Copper")))
    {
        sink.note(
            "pad.layer_dropped",
            Severity::Dropped,
            "pad on a layer the library footprint has no token for dropped from that layer",
        );
    }
    out
}

fn pad(p: &BoardPad, margins: Margins, sink: &mut Sink<'_>) -> Option<out::Pad> {
    let (text, changed) = snx_writer::canon::clean_cell(&p.number);
    if changed {
        sink.note(
            "pad.text_sanitized",
            Severity::Approximated,
            "control characters in a pad number replaced by spaces",
        );
    }
    let property = p.property.as_str();
    let fiducial = property.starts_with("pad_prop_fiducial");
    let castellated = property == "pad_prop_castellated";
    if matches!(
        property,
        "pad_prop_bga" | "pad_prop_testpoint" | "pad_prop_heatsink"
    ) {
        sink.note(
            "pad.property_not_carried",
            Severity::Approximated,
            "pad property has no Signex field",
        );
    }
    let kind = if fiducial {
        "Fiducial"
    } else if castellated {
        "Castellated"
    } else {
        match p.pad_type {
            PadType::Smd => "Smd",
            PadType::Thru => "Tht",
            PadType::NpThru => "NptHole",
            PadType::Connect => "ConnectorPad",
        }
    };
    // The format's table fixes the layer set of a fiducial and a castellated pad.
    let layers = if fiducial {
        let bottom = p.layers.iter().any(|l| l == "BottomCopper")
            && !p.layers.iter().any(|l| l == "TopCopper");
        if bottom {
            vec!["Bottom Layer".to_string(), "Bottom Solder".to_string()]
        } else {
            vec!["Top Layer".to_string(), "Top Solder".to_string()]
        }
    } else if castellated {
        ["Top Layer", "Bottom Layer", "Top Solder", "Bottom Solder"]
            .map(str::to_string)
            .to_vec()
    } else {
        pad_layers(p, sink)
    };
    // `(drill oval DIAMETER WIDTH)`: the smaller size is the hole's diameter,
    // the larger the slot length. A slot lies along the pad's x axis here; one
    // longer in y is made exact for the plain shapes by swapping the pad's
    // size and turning it a quarter (the slot and the copper turn together).
    let mut size = p2(p.size);
    let mut rotation = p.rotation;
    let (drill, slot_length) = match (p.drill_slot, p.drill) {
        (Some(s), Some(_)) => {
            sink.note_once(
                "pad.slot_not_placeable",
                Severity::Approximated,
                "Signex cannot place this footprint on a board until slotted drills are supported (signex #3128); the footprint is written exactly",
            );
            if s.y > s.x {
                if matches!(
                    p.shape,
                    PadShape::Circle | PadShape::Rect | PadShape::Oval | PadShape::RoundRect(_)
                ) {
                    size = [size[1], size[0]];
                    rotation = (rotation + 90.0).rem_euclid(360.0);
                } else {
                    sink.note(
                        "pad.slot_orientation_approximated",
                        Severity::Approximated,
                        "slot drill longer in y on a trapezoid or custom pad written with its length along the pad's x axis",
                    );
                }
            }
            (Some(s.x.min(s.y)), Some(s.x.max(s.y)))
        }
        (_, d) => (d, None),
    };
    Some(out::Pad {
        number: text,
        kind: kind.to_string(),
        shape: shape_token(&p.shape),
        size,
        pos: p2(p.position),
        rotation,
        layers,
        drill,
        slot_length,
        mask_margin: p.solder_mask_margin.or(margins.mask),
        paste_margin: p.paste_margin.or(margins.paste),
    })
}

/// Angle of `p` about `c` in degrees from +x towards +y, in `[0, 360)`,
/// rounded to 1e-9 so float noise does not show.
fn angle_deg(c: [f64; 2], p: [f64; 2]) -> f64 {
    let deg = (p[1] - c[1])
        .atan2(p[0] - c[0])
        .to_degrees()
        .rem_euclid(360.0);
    ((deg * 1e9).round() / 1e9).rem_euclid(360.0)
}

fn graphic(g: &FootprintGraphic, sink: &mut Sink<'_>) -> Option<out::Graphic> {
    let filled = g.fill == "solid";
    let kind = match g.graphic_type.as_str() {
        "line" => out::GraphicKind::Line {
            from: p2(g.start?),
            to: p2(g.end?),
        },
        "rect" => out::GraphicKind::Rectangle {
            from: p2(g.start?),
            to: p2(g.end?),
        },
        "circle" => {
            let (c, e) = (p2(g.center?), p2(g.end?));
            out::GraphicKind::Circle {
                center: c,
                radius: (e[0] - c[0]).hypot(e[1] - c[1]),
            }
        }
        "arc" => {
            let (c, s, m, e) = (p2(g.center?), p2(g.start?), p2(g.mid?), p2(g.end?));
            let (a_s, a_m, a_e) = (angle_deg(c, s), angle_deg(c, m), angle_deg(c, e));
            let ccw = (a_m - a_s).rem_euclid(360.0) <= (a_e - a_s).rem_euclid(360.0);
            let (start_deg, end_deg) = if ccw { (a_s, a_e) } else { (a_e, a_s) };
            out::GraphicKind::Arc {
                center: c,
                radius: (s[0] - c[0]).hypot(s[1] - c[1]),
                start_deg,
                end_deg,
            }
        }
        "poly" => {
            let mut v: Vec<[f64; 2]> = g.points.iter().map(|p| p2(*p)).collect();
            if v.len() > 1 && v.first() == v.last() {
                v.pop();
            }
            if v.len() < 3 {
                return None;
            }
            out::GraphicKind::Polygon { vertices: v }
        }
        "text" => {
            if g.rotation.rem_euclid(360.0) != 0.0 {
                sink.note(
                    "footprint.text_rotation_dropped",
                    Severity::Approximated,
                    "text rotation not carried; the text is written upright",
                );
            }
            let (content, changed) = snx_writer::canon::clean_cell(&g.text);
            if changed {
                sink.note(
                    "pad.text_sanitized",
                    Severity::Approximated,
                    "control characters in a pad number replaced by spaces",
                );
            }
            out::GraphicKind::Text {
                position: p2(g.position?),
                content,
                size: g.font_size,
            }
        }
        _ => return None,
    };
    Some(out::Graphic {
        stroke_width: g.width,
        filled,
        kind,
    })
}

/// The courtyard polygon from the drawings on the courtyard layer: a
/// polygon or rectangle as it is, else the lines chained into a closed ring;
/// anything else falls back to the bounding box of everything drawn.
fn courtyard_polygon(gs: &[&FootprintGraphic], sink: &mut Sink<'_>) -> Vec<[f64; 2]> {
    for g in gs {
        match g.graphic_type.as_str() {
            "rect" => {
                if let (Some(a), Some(b)) = (g.start, g.end) {
                    return vec![[a.x, a.y], [b.x, a.y], [b.x, b.y], [a.x, b.y]];
                }
            }
            "poly" if g.points.len() >= 3 => {
                let mut v: Vec<[f64; 2]> = g.points.iter().map(|p| p2(*p)).collect();
                if v.first() == v.last() {
                    v.pop();
                }
                return v;
            }
            _ => {}
        }
    }
    let segments: Vec<([f64; 2], [f64; 2])> = gs
        .iter()
        .filter(|g| g.graphic_type == "line")
        .filter_map(|g| Some((p2(g.start?), p2(g.end?))))
        .collect();
    if segments.len() == gs.len()
        && let Some(ring) = chain(&segments)
    {
        return ring;
    }
    sink.note(
        "footprint.courtyard_approximated",
        Severity::Approximated,
        "courtyard that is not one polygon written as the bounding box of its drawings",
    );
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for g in gs {
        pts.extend(
            [g.start, g.end, g.center, g.position]
                .into_iter()
                .flatten()
                .map(p2),
        );
        pts.extend(g.points.iter().map(|p| p2(*p)));
        if g.graphic_type == "circle"
            && let (Some(c), Some(e)) = (g.center, g.end)
        {
            let r = (e.x - c.x).hypot(e.y - c.y);
            pts.extend([[c.x - r, c.y - r], [c.x + r, c.y + r]]);
        }
    }
    if pts.is_empty() {
        return Vec::new();
    }
    let min = |i: usize| pts.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min);
    let max = |i: usize| pts.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max);
    vec![
        [min(0), min(1)],
        [max(0), min(1)],
        [max(0), max(1)],
        [min(0), max(1)],
    ]
}

/// Lines end to end into one closed ring; `None` when they do not close or
/// leave some over.
fn chain(segments: &[([f64; 2], [f64; 2])]) -> Option<Vec<[f64; 2]>> {
    const TOL: f64 = 1e-4;
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL;
    let mut used = vec![false; segments.len()];
    used[0] = true;
    let mut ring = vec![segments[0].0];
    let mut at = segments[0].1;
    for _ in 1..segments.len() {
        let next = segments.iter().enumerate().find_map(|(i, &(a, b))| {
            if used[i] {
                None
            } else if near(a, at) {
                Some((i, b))
            } else if near(b, at) {
                Some((i, a))
            } else {
                None
            }
        })?;
        used[next.0] = true;
        ring.push(at);
        at = next.1;
    }
    (near(at, ring[0]) && ring.len() >= 3).then_some(ring)
}

#[cfg(test)]
#[path = "footprint_lib_tests.rs"]
mod tests;
