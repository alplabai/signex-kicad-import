// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `.snxsym` symbol envelope, written from `docs/LIBRARY_FORMAT.md`
//! sections 3 and 5 (the public Signex library format). One envelope per
//! source library; the model here is plain data in Signex's own terms.

use std::fmt::Write as _;

use crate::canon::{
    cell_number, float_array, int_array, point_array, toml_float, toml_key, toml_string,
};

pub type P = [f64; 2];

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolFile {
    pub file_uuid: String,
    pub display_name: String,
    pub created: String,
    pub updated: String,
    pub symbols: Vec<Symbol>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub uuid: String,
    pub name: String,
    pub anchor: P,
    pub pins: Vec<Pin>,
    pub designator: String,
    pub comment: String,
    pub designator_pos: Option<P>,
    pub comment_pos: Option<P>,
    pub description: String,
    /// `Standard`, `Mechanical`, `Graphical`, `NetTie`, `StandardNoBom`, `Jumper`.
    pub component_type: String,
    pub mirrored: bool,
    pub local_fill_color: Option<[u8; 4]>,
    pub local_line_color: Option<[u8; 4]>,
    pub local_pin_color: Option<[u8; 4]>,
    pub version: String,
    pub released: bool,
    /// The number of units; Signex stores it as one byte.
    pub part_count: u8,
    pub created: String,
    pub updated: String,
    pub graphics: Vec<Graphic>,
    /// `schematic_params` of kind `Text`, in key order.
    pub params: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Right,
    Up,
    Left,
    Down,
}

impl Orientation {
    fn token(self) -> &'static str {
        match self {
            Self::Right => "Right",
            Self::Up => "Up",
            Self::Left => "Left",
            Self::Down => "Down",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    pub number: String,
    pub name: String,
    /// One of the electrical tokens of section 3.2.
    pub electrical: String,
    pub pos: P,
    pub orientation: Orientation,
    pub length: f64,
    pub description: String,
    /// Alternative names; written joined with `|`.
    pub function: Vec<String>,
    pub package_length: Option<f64>,
    pub propagation_delay_ns: Option<f64>,
    pub designator_visible: bool,
    pub name_visible: bool,
    /// The four symbol decorations: inside, inside edge, outside edge,
    /// outside, as tokens (`None` for none).
    pub decorations: [String; 4],
    pub hidden: bool,
    pub locked: bool,
    /// The unit, 0 for every unit.
    pub part_number: u8,
    /// 0 both bodies, 1 normal, 2 alternate.
    pub body_style: u8,
}

impl Pin {
    /// A pin with the plain defaults of the format.
    pub fn new(
        number: &str,
        name: &str,
        electrical: &str,
        pos: P,
        orientation: Orientation,
        length: f64,
    ) -> Self {
        Pin {
            number: number.to_string(),
            name: name.to_string(),
            electrical: electrical.to_string(),
            pos,
            orientation,
            length,
            description: String::new(),
            function: Vec::new(),
            package_length: None,
            propagation_delay_ns: None,
            designator_visible: true,
            name_visible: true,
            decorations: std::array::from_fn(|_| "None".to_string()),
            hidden: false,
            locked: false,
            part_number: 1,
            body_style: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Graphic {
    pub stroke_width: f64,
    pub fill: Option<[u8; 4]>,
    pub part_number: u8,
    pub body_style: u8,
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
    Bezier {
        points: Vec<P>,
    },
    RoundRectangle {
        from: P,
        to: P,
        corner_radius: f64,
    },
}

impl GraphicKind {
    fn needs_v4(&self) -> bool {
        matches!(self, Self::Bezier { .. } | Self::RoundRectangle { .. })
    }
}

/// The lowest token the whole file needs (section 3.5).
fn token(file: &SymbolFile) -> &'static str {
    let graphics = || file.symbols.iter().flat_map(|s| &s.graphics);
    let pins = || file.symbols.iter().flat_map(|s| &s.pins);
    if graphics().any(|g| g.kind.needs_v4()) {
        "snxsym/v4"
    } else if pins().any(|p| p.body_style == 0 || p.body_style == 2)
        || graphics().any(|g| g.body_style == 0 || g.body_style == 2)
    {
        "snxsym/v3"
    } else if graphics().any(|g| matches!(g.kind, GraphicKind::Polygon { .. })) {
        "snxsym/v2"
    } else {
        "snxsym/v1"
    }
}

const PIN_COLUMNS: [&str; 20] = [
    "number",
    "name",
    "electrical",
    "pos_x",
    "pos_y",
    "orientation",
    "length",
    "description",
    "function",
    "pin_package_length",
    "propagation_delay_ns",
    "designator_visible",
    "name_visible",
    "inside_symbol",
    "inside_edge_symbol",
    "outside_edge_symbol",
    "outside_symbol",
    "hidden",
    "locked",
    "part_number",
];

fn opt(v: Option<f64>) -> String {
    v.map_or_else(String::new, cell_number)
}

fn pins_tsv(pins: &[Pin]) -> String {
    let with_style = pins.iter().any(|p| p.body_style != 1);
    let mut out = PIN_COLUMNS.join("\t");
    if with_style {
        out.push_str("\tbody_style");
    }
    out.push('\n');
    for p in pins {
        let mut cells = vec![
            p.number.clone(),
            p.name.clone(),
            p.electrical.clone(),
            cell_number(p.pos[0]),
            cell_number(p.pos[1]),
            p.orientation.token().to_string(),
            cell_number(p.length),
            p.description.clone(),
            p.function.join("|"),
            opt(p.package_length),
            opt(p.propagation_delay_ns),
            p.designator_visible.to_string(),
            p.name_visible.to_string(),
        ];
        cells.extend(p.decorations.iter().cloned());
        cells.push(p.hidden.to_string());
        cells.push(p.locked.to_string());
        cells.push(p.part_number.to_string());
        if with_style {
            cells.push(p.body_style.to_string());
        }
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// The canonical bytes of one envelope.
pub fn write_symbol_file(file: &SymbolFile) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "format = {}", toml_string(token(file)));
    let _ = writeln!(o, "file_uuid = {}", toml_string(&file.file_uuid));
    let _ = writeln!(o, "display_name = {}", toml_string(&file.display_name));
    let _ = writeln!(o, "created = {}", toml_string(&file.created));
    let _ = writeln!(o, "updated = {}", toml_string(&file.updated));
    for s in &file.symbols {
        write_symbol(&mut o, s);
    }
    o
}

fn color(key: &str, c: Option<[u8; 4]>) -> String {
    c.map_or_else(String::new, |c| int_array(key, &c))
}

fn write_symbol(o: &mut String, s: &Symbol) {
    o.push_str("\n[[symbols]]\n");
    let _ = writeln!(o, "uuid = {}", toml_string(&s.uuid));
    let _ = writeln!(o, "name = {}", toml_string(&s.name));
    o.push_str(&float_array("anchor", &s.anchor));
    let _ = writeln!(o, "pins_tsv = '''\n{}'''", pins_tsv(&s.pins));
    if s.graphics.is_empty() {
        o.push_str("graphics = []\n");
    }
    let _ = writeln!(o, "designator = {}", toml_string(&s.designator));
    let _ = writeln!(o, "comment = {}", toml_string(&s.comment));
    if let Some(p) = s.designator_pos {
        o.push_str(&float_array("designator_pos", &p));
    }
    if let Some(p) = s.comment_pos {
        o.push_str(&float_array("comment_pos", &p));
    }
    let _ = writeln!(o, "description = {}", toml_string(&s.description));
    let _ = writeln!(o, "component_type = {}", toml_string(&s.component_type));
    let _ = writeln!(o, "mirrored = {}", s.mirrored);
    o.push_str(&color("local_fill_color", s.local_fill_color));
    o.push_str(&color("local_line_color", s.local_line_color));
    o.push_str(&color("local_pin_color", s.local_pin_color));
    let _ = writeln!(o, "version = {}", toml_string(&s.version));
    let _ = writeln!(o, "released = {}", s.released);
    let _ = writeln!(o, "part_count = {}", s.part_count);
    let _ = writeln!(o, "created = {}", toml_string(&s.created));
    let _ = writeln!(o, "updated = {}", toml_string(&s.updated));
    for g in &s.graphics {
        write_graphic(o, g);
    }
    if s.params.is_empty() {
        o.push_str("\n[symbols.schematic_params]\n");
    }
    for (key, value) in &s.params {
        let _ = write!(
            o,
            "\n[symbols.schematic_params.{}]\nkind = \"Text\"\nvalue = {}\n",
            toml_key(key),
            toml_string(value)
        );
    }
}

fn write_graphic(o: &mut String, g: &Graphic) {
    o.push_str("\n[[symbols.graphics]]\n");
    let _ = writeln!(o, "stroke_width = {}", toml_float(g.stroke_width));
    o.push_str(&color("fill", g.fill));
    let _ = writeln!(o, "part_number = {}", g.part_number);
    if g.body_style != 1 {
        let _ = writeln!(o, "body_style = {}", g.body_style);
    }
    o.push_str("\n[symbols.graphics.kind]\n");
    let kind = |o: &mut String, k: &str| {
        let _ = writeln!(o, "kind = \"{k}\"");
    };
    match &g.kind {
        GraphicKind::Line { from, to } => {
            kind(o, "line");
            o.push_str(&float_array("from", from));
            o.push_str(&float_array("to", to));
        }
        GraphicKind::Rectangle { from, to } => {
            kind(o, "rectangle");
            o.push_str(&float_array("from", from));
            o.push_str(&float_array("to", to));
        }
        GraphicKind::Circle { center, radius } => {
            kind(o, "circle");
            o.push_str(&float_array("center", center));
            let _ = writeln!(o, "radius = {}", toml_float(*radius));
        }
        GraphicKind::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            kind(o, "arc");
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
            kind(o, "text");
            o.push_str(&float_array("position", position));
            let _ = writeln!(o, "content = {}", toml_string(content));
            let _ = writeln!(o, "size = {}", toml_float(*size));
        }
        GraphicKind::Polygon { vertices } => {
            kind(o, "polygon");
            o.push_str(&point_array("vertices", vertices));
        }
        GraphicKind::Bezier { points } => {
            kind(o, "bezier");
            o.push_str(&point_array("points", points));
        }
        GraphicKind::RoundRectangle {
            from,
            to,
            corner_radius,
        } => {
            kind(o, "round_rectangle");
            o.push_str(&float_array("from", from));
            o.push_str(&float_array("to", to));
            let _ = writeln!(o, "corner_radius = {}", toml_float(*corner_radius));
        }
    }
}
