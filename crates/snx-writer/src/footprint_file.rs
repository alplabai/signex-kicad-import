// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `.snxfpt` footprint envelope (one footprint per file), written from
//! `docs/LIBRARY_FORMAT.md` sections 4 and 5. The footprint frame is the
//! board's: +y down, angles from +x toward +y.

use std::fmt::Write as _;

use crate::canon::{
    cell_number, clean_cell, f32_array, float_array, point_array, toml_f32, toml_float, toml_string,
};
use crate::symbol_file::P;

#[derive(Debug, Clone, PartialEq)]
pub struct FootprintFile {
    pub file_uuid: String,
    pub display_name: String,
    pub created: String,
    pub updated: String,
    pub footprint: Footprint,
    /// Embedded STEP models (section 8), one per distinct hash; the file's
    /// token is `snxfpt/1.4` when there is any.
    pub models: Vec<Model>,
}

/// A STEP model embedded in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Lower-case hex SHA-256 of the raw bytes.
    pub hash: String,
    pub filename: String,
    /// The raw (uncompressed) STEP bytes.
    pub bytes: std::sync::Arc<[u8]>,
}

/// The footprint's reference to an embedded model.
#[derive(Debug, Clone, PartialEq)]
pub struct StepAttachment {
    pub content_hash: String,
    pub filename: String,
    pub offset_xyz: [f64; 3],
    pub rotation_xyz: [f64; 3],
}

/// The most raw bytes a model may have (256 MiB).
pub const MAX_MODEL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub uuid: String,
    pub name: String,
    pub pads: Vec<Pad>,
    pub silk_f: Vec<Graphic>,
    pub silk_b: Vec<Graphic>,
    pub fab_f: Vec<Graphic>,
    pub fab_b: Vec<Graphic>,
    pub version: String,
    pub released: bool,
    pub created: String,
    pub updated: String,
    pub description: String,
    pub default_designator: String,
    /// `None` is `Standard`.
    pub component_type: Option<String>,
    pub height_mm: Option<f64>,
    pub courtyard: Option<Vec<P>>,
    pub body: Body3d,
    pub step: Option<StepAttachment>,
}

/// The extruded body; the values are 32-bit floats in the format.
#[derive(Debug, Clone, PartialEq)]
pub struct Body3d {
    pub height_mm: f32,
    pub offset_z_mm: f32,
    pub top_color: [f32; 4],
    pub side_color: [f32; 4],
}

impl Default for Body3d {
    fn default() -> Self {
        Body3d {
            height_mm: 1.0,
            offset_z_mm: 0.0,
            top_color: [0.2, 0.2, 0.2, 1.0],
            side_color: [0.3, 0.3, 0.3, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    pub number: String,
    /// `Smd`, `Tht`, `NptHole`, `ConnectorPad`, `Castellated`, `Fiducial`.
    pub kind: String,
    /// A shape token of section 4.2, already formatted.
    pub shape: String,
    pub size: P,
    pub pos: P,
    pub rotation: f64,
    pub layers: Vec<String>,
    pub drill: Option<f64>,
    pub slot_length: Option<f64>,
    pub mask_margin: Option<f64>,
    pub paste_margin: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Graphic {
    pub stroke_width: f64,
    pub filled: bool,
    pub kind: GraphicKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicKind {
    Line {
        from: P,
        to: P,
    },
    Rectangle {
        from: P,
        to: P,
    },
    Circle {
        center: P,
        radius: f64,
    },
    Arc {
        center: P,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
    Text {
        position: P,
        content: String,
        size: f64,
    },
    Polygon {
        vertices: Vec<P>,
    },
}

fn opt(v: Option<f64>) -> String {
    v.map_or_else(String::new, cell_number)
}

const PAD_COLUMNS: [&str; 13] = [
    "number",
    "kind",
    "shape",
    "size_x",
    "size_y",
    "pos_x",
    "pos_y",
    "rotation",
    "layers",
    "drill_diameter",
    "drill_slot_length",
    "solder_mask_margin",
    "paste_margin",
];

fn pads_tsv(pads: &[Pad]) -> String {
    let mut out = PAD_COLUMNS.join("\t");
    out.push('\n');
    for p in pads {
        let cells = [
            clean_cell(&p.number).0,
            clean_cell(&p.kind).0,
            clean_cell(&p.shape).0,
            cell_number(p.size[0]),
            cell_number(p.size[1]),
            cell_number(p.pos[0]),
            cell_number(p.pos[1]),
            cell_number(p.rotation),
            p.layers
                .iter()
                .map(|l| clean_cell(l).0.replace('|', "/"))
                .collect::<Vec<_>>()
                .join("|"),
            opt(p.drill),
            opt(p.slot_length),
            opt(p.mask_margin),
            opt(p.paste_margin),
        ];
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// The canonical bytes of one footprint envelope (`snxfpt/1.1`: no sketch,
/// no generated features).
pub fn write_footprint_file(file: &FootprintFile) -> String {
    let f = &file.footprint;
    let mut o = String::new();
    let token = if file.models.is_empty() {
        "snxfpt/1.1"
    } else {
        "snxfpt/1.4"
    };
    let _ = writeln!(o, "format = {}", toml_string(token));
    let _ = writeln!(o, "file_uuid = {}", toml_string(&file.file_uuid));
    let _ = writeln!(o, "display_name = {}", toml_string(&file.display_name));
    let _ = writeln!(o, "created = {}", toml_string(&file.created));
    let _ = writeln!(o, "updated = {}", toml_string(&file.updated));
    o.push_str("\n[[footprints]]\n");
    let _ = writeln!(o, "uuid = {}", toml_string(&f.uuid));
    let _ = writeln!(o, "name = {}", toml_string(&f.name));
    o.push_str(&float_array("anchor", &[0.0, 0.0]));
    let _ = writeln!(o, "pads_tsv = '''\n{}'''", pads_tsv(&f.pads));
    let lists = [
        ("silk_f", &f.silk_f),
        ("silk_b", &f.silk_b),
        ("fab_f", &f.fab_f),
        ("fab_b", &f.fab_b),
    ];
    for (key, list) in lists {
        if list.is_empty() {
            let _ = writeln!(o, "{key} = []");
        }
    }
    let _ = writeln!(o, "version = {}", toml_string(&f.version));
    let _ = writeln!(o, "released = {}", f.released);
    let _ = writeln!(o, "created = {}", toml_string(&f.created));
    let _ = writeln!(o, "updated = {}", toml_string(&f.updated));
    let _ = writeln!(o, "schema_version = 3");
    if !f.description.is_empty() {
        let _ = writeln!(o, "description = {}", toml_string(&f.description));
    }
    if !f.default_designator.is_empty() {
        let _ = writeln!(
            o,
            "default_designator = {}",
            toml_string(&f.default_designator)
        );
    }
    if let Some(t) = &f.component_type {
        let _ = writeln!(o, "component_type = {}", toml_string(t));
    }
    if let Some(h) = f.height_mm {
        let _ = writeln!(o, "height_mm = {}", toml_float(h));
    }
    if let Some(points) = &f.courtyard {
        o.push_str("\n[footprints.courtyard]\n");
        o.push_str(&point_array("points", points));
    }
    for (key, list) in lists {
        for g in list {
            write_graphic(&mut o, key, g);
        }
    }
    o.push_str("\n[footprints.body_3d]\nshape = \"Extrude\"\n");
    let _ = writeln!(o, "height_mm = {}", toml_f32(f.body.height_mm));
    let _ = writeln!(o, "offset_z_mm = {}", toml_f32(f.body.offset_z_mm));
    o.push_str(&f32_array("top_color", &f.body.top_color));
    o.push_str(&f32_array("side_color", &f.body.side_color));
    if let Some(s) = &f.step {
        o.push_str("\n[footprints.step_attachment]\n");
        let _ = writeln!(o, "content_hash = {}", toml_string(&s.content_hash));
        let _ = writeln!(o, "filename = {}", toml_string(&s.filename));
        o.push_str(&float_array("offset_xyz", &s.offset_xyz));
        o.push_str(&float_array("rotation_xyz", &s.rotation_xyz));
    }
    o.push_str("\n[footprints.pcb_params]\n");
    let mut models: Vec<&Model> = file.models.iter().collect();
    models.sort_by(|a, b| a.hash.cmp(&b.hash));
    for m in models {
        write_model(&mut o, m);
    }
    o
}

/// One `[[models]]` entry: raw deflate, base64 wrapped at 76 columns.
fn write_model(o: &mut String, m: &Model) {
    use base64::Engine as _;
    use std::io::Write as _;
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
    let packed = enc
        .write_all(&m.bytes)
        .and_then(|()| enc.finish())
        .unwrap_or_default();
    let b64 = base64::engine::general_purpose::STANDARD.encode(packed);
    o.push_str("\n[[models]]\n");
    let _ = writeln!(o, "hash = {}", toml_string(&m.hash));
    let _ = writeln!(o, "filename = {}", toml_string(&m.filename));
    o.push_str("encoding = \"deflate+base64\"\n");
    let _ = writeln!(o, "size = {}", m.bytes.len());
    o.push_str("data = '''\n");
    for line in b64.as_bytes().chunks(76) {
        o.push_str(std::str::from_utf8(line).unwrap_or(""));
        o.push('\n');
    }
    o.push_str("'''\n");
}

fn write_graphic(o: &mut String, list: &str, g: &Graphic) {
    let _ = write!(
        o,
        "\n[[footprints.{list}]]\nstroke_width = {}\nfilled = {}\n\n[footprints.{list}.kind]\n",
        toml_float(g.stroke_width),
        g.filled
    );
    match &g.kind {
        GraphicKind::Line { from, to } => {
            o.push_str("kind = \"line\"\n");
            o.push_str(&float_array("from", from));
            o.push_str(&float_array("to", to));
        }
        GraphicKind::Rectangle { from, to } => {
            o.push_str("kind = \"rectangle\"\n");
            o.push_str(&float_array("from", from));
            o.push_str(&float_array("to", to));
        }
        GraphicKind::Circle { center, radius } => {
            o.push_str("kind = \"circle\"\n");
            o.push_str(&float_array("center", center));
            let _ = writeln!(o, "radius = {}", toml_float(*radius));
        }
        GraphicKind::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            o.push_str("kind = \"arc\"\n");
            o.push_str(&float_array("center", center));
            let _ = writeln!(o, "radius = {}", toml_float(*radius));
            let _ = writeln!(o, "start_deg = {}", toml_float(*start_deg));
            let _ = writeln!(o, "end_deg = {}", toml_float(*end_deg));
        }
        GraphicKind::Text {
            position,
            content,
            size,
        } => {
            o.push_str("kind = \"text\"\n");
            o.push_str(&float_array("position", position));
            let _ = writeln!(o, "content = {}", toml_string(content));
            let _ = writeln!(o, "size = {}", toml_float(*size));
        }
        GraphicKind::Polygon { vertices } => {
            o.push_str("kind = \"polygon\"\n");
            o.push_str(&point_array("vertices", vertices));
        }
    }
}
