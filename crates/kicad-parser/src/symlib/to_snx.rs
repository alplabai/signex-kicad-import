// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! A read symbol library as Signex symbols (`snx_writer::symbol_file`).
//!
//! Frames: a KiCad symbol library and a Signex symbol are both +y up
//! (`LIBRARY_FORMAT.md` section 3), so coordinates and angles carry over
//! unchanged; only the footprint frame differs (see `footprint_lib.rs`).
//! Unit numbers carry over (0 stays "every unit"); body styles 1 and 2 map
//! to 1 and 2, style 0 to "both bodies", any other style is dropped.

use serde_json::json;
use snx_writer::symbol_file::{
    Graphic as G, GraphicKind as K, Orientation, Pin as SPin, Symbol as SSym, SymbolFile,
};

use super::model::{Fill, Graphic, Pin, PinStyle, PinType, Point, Symbol, SymbolLib, Unit};
use crate::items::{Item, ItemKind, Severity};

/// The fixed envelope time: a conversion has no clock, so the same input
/// gives the same bytes (the format asks for a time, not for a real one).
pub const EPOCH: &str = "1970-01-01T00:00:00Z";
/// Signex's own default fill, used for a KiCad fill the file does not give a colour.
const BACKGROUND_FILL: [u8; 4] = [255, 255, 194, 255];
const OUTLINE_FILL: [u8; 4] = [0, 0, 0, 255];

struct Notes<'a> {
    library: &'a str,
    symbol: &'a str,
    items: &'a mut Vec<Item>,
    counts: Vec<(&'static str, Severity, &'static str, usize)>,
}

impl Notes<'_> {
    fn note(&mut self, code: &'static str, severity: Severity, message: &'static str) {
        match self.counts.iter_mut().find(|c| c.0 == code) {
            Some(c) => c.3 += 1,
            None => self.counts.push((code, severity, message, 1)),
        }
    }

    fn flush(self) {
        for (code, severity, message, count) in self.counts {
            self.items.push(
                Item::new(
                    ItemKind::Symbol,
                    format!("{}:{}", self.library, self.symbol),
                    code,
                    severity,
                    message,
                )
                .with_detail(json!({ "count": count })),
            );
        }
    }
}

/// Signex symbols for every readable symbol of `lib`, with the report items
/// for what the mapping approximated or dropped. `ids` gives the uuid of the
/// symbol of that name and of the envelope.
pub fn to_symbol_file(
    lib: &SymbolLib,
    library: &str,
    file_uuid: String,
    symbol_id: &dyn Fn(&str) -> String,
    items: &mut Vec<Item>,
) -> SymbolFile {
    let mut symbols = Vec::new();
    for s in &lib.symbols {
        // A derived symbol is a part row on its parent's symbol, not a symbol.
        if s.extends.is_some() {
            continue;
        }
        let Some(units) = lib.effective_units(s) else {
            continue; // reported by the reader (extends_missing / cycle)
        };
        let mut notes = Notes {
            library,
            symbol: &s.name,
            items,
            counts: Vec::new(),
        };
        symbols.push(symbol(s, units, symbol_id(&s.name), &mut notes));
        notes.flush();
    }
    SymbolFile {
        file_uuid,
        display_name: library.to_string(),
        created: EPOCH.to_string(),
        updated: EPOCH.to_string(),
        symbols,
    }
}

fn symbol(s: &Symbol, units: &[Unit], uuid: String, n: &mut Notes<'_>) -> SSym {
    let reference = s.property("Reference").unwrap_or("U");
    let designator = if reference.contains('?') {
        reference.to_string()
    } else {
        format!("{reference}?")
    };
    let at = |key: &str| {
        s.properties
            .iter()
            .find(|p| p.key == key && !p.hidden)
            .and_then(|p| p.position)
            .map(|(p, _)| [p.x, p.y])
    };
    if s.power {
        n.note(
            "symbol.power_not_carried",
            Severity::Approximated,
            "power symbol flag has no Signex field; written as an ordinary symbol",
        );
    }
    if !s.on_board {
        n.note(
            "symbol.on_board_not_carried",
            Severity::Approximated,
            "on_board no has no symbol field; the part still goes to the board",
        );
    }
    if s.exclude_from_sim == Some(true) {
        n.note(
            "symbol.exclude_from_sim_not_carried",
            Severity::Approximated,
            "exclude_from_sim has no symbol field",
        );
    }
    let mut pins = Vec::new();
    let mut graphics = Vec::new();
    let mut part_count: u8 = 1;
    for u in units {
        let Some(style) = body_style(u.style) else {
            n.note(
                "symbol.body_style_dropped",
                Severity::Dropped,
                "body style above 2 dropped",
            );
            continue;
        };
        // Signex stores the unit as one byte: a higher unit cannot be written.
        let Ok(unit) = u8::try_from(u.unit) else {
            n.note(
                "symbol.unit_dropped",
                Severity::Dropped,
                "unit above 255 dropped; Signex symbols hold at most 255 units",
            );
            continue;
        };
        part_count = part_count.max(unit);
        for p in &u.pins {
            pins.push(pin(s, p, unit, style, n));
        }
        for g in &u.graphics {
            graphics.extend(graphic(g, unit, style, n));
        }
    }
    SSym {
        uuid,
        name: s.name.clone(),
        anchor: [0.0, 0.0],
        pins,
        designator,
        // Placement shows the part value from the symbol's comment when the
        // part has no MPN; the source Value is the closest text.
        comment: s
            .property("Value")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("*")
            .to_string(),
        designator_pos: at("Reference"),
        comment_pos: at("Value"),
        description: s.description().unwrap_or("").to_string(),
        component_type: if s.in_bom {
            "Standard"
        } else {
            "StandardNoBom"
        }
        .to_string(),
        mirrored: false,
        local_fill_color: None,
        local_line_color: None,
        local_pin_color: None,
        version: "1.0.0".to_string(),
        released: false,
        part_count,
        created: EPOCH.to_string(),
        updated: EPOCH.to_string(),
        graphics,
        params: Vec::new(),
    }
}

fn body_style(style: u32) -> Option<u8> {
    match style {
        0 => Some(0),
        1 => Some(1),
        2 => Some(2),
        _ => None,
    }
}

fn clean(text: &str, n: &mut Notes<'_>) -> String {
    let (out, changed) = snx_writer::canon::clean_cell(text);
    if changed {
        n.note(
            "symbol.text_sanitized",
            Severity::Approximated,
            "control characters in a pin text replaced by spaces",
        );
    }
    out
}

fn electrical(t: PinType, n: &mut Notes<'_>) -> &'static str {
    match t {
        PinType::Input => "Input",
        PinType::Output => "Output",
        PinType::Bidirectional => "Bidirectional",
        PinType::TriState => "Tristate",
        PinType::Passive => "Passive",
        PinType::Unspecified => "Unspecified",
        PinType::PowerIn => "Power",
        PinType::PowerOut => "PowerOutput",
        PinType::OpenCollector => "OpenCollector",
        PinType::OpenEmitter => "OpenEmitter",
        PinType::NoConnect => "NotConnected",
        PinType::Free => {
            n.note(
                "symbol.pin_type_approximated",
                Severity::Approximated,
                "pin type free written as unspecified",
            );
            "Unspecified"
        }
    }
}

/// The pin's decoration. Signex reads one column, the outside edge, for the
/// symbols a schematic draws (the other three columns are written as `None`),
/// so a KiCad style that combines a marker with a clock wedge keeps the
/// marker and reports the loss.
fn decorations(style: PinStyle, n: &mut Notes<'_>) -> [&'static str; 4] {
    let outside_edge = match style {
        PinStyle::Line => "None",
        PinStyle::Inverted => "Dot",
        PinStyle::Clock => "ClockEdge",
        PinStyle::InputLow => "ActiveLowInput",
        PinStyle::OutputLow => "ActiveLowOutput",
        PinStyle::InvertedClock => {
            n.note(
                "symbol.pin_style_approximated",
                Severity::Approximated,
                "pin style inverted_clock written as an inverted pin; the clock wedge is lost",
            );
            "Dot"
        }
        PinStyle::ClockLow => {
            n.note(
                "symbol.pin_style_approximated",
                Severity::Approximated,
                "pin style clock_low written as an active-low input; the clock wedge is lost",
            );
            "ActiveLowInput"
        }
        PinStyle::EdgeClockHigh => {
            n.note(
                "symbol.pin_style_approximated",
                Severity::Approximated,
                "pin style edge_clock_high written as a plain clock",
            );
            "ClockEdge"
        }
        PinStyle::NonLogic => {
            n.note(
                "symbol.pin_style_approximated",
                Severity::Approximated,
                "pin style non_logic written as a plain pin",
            );
            "None"
        }
    };
    ["None", "None", outside_edge, "None"]
}

fn orientation(angle: f64, n: &mut Notes<'_>) -> Orientation {
    let a = angle.rem_euclid(360.0);
    if ![0.0, 90.0, 180.0, 270.0].contains(&a) {
        n.note(
            "symbol.pin_angle_approximated",
            Severity::Approximated,
            "pin angle not a multiple of 90 written as the nearest direction",
        );
    }
    match ((a / 90.0).round() as i64).rem_euclid(4) {
        0 => Orientation::Right,
        1 => Orientation::Up,
        2 => Orientation::Left,
        _ => Orientation::Down,
    }
}

fn pin(s: &Symbol, p: &Pin, unit: u8, style: u8, n: &mut Notes<'_>) -> SPin {
    let nameless = p.name == "~" || p.name.is_empty();
    let name = if p.name == "~" {
        String::new()
    } else {
        clean(&p.name, n)
    };
    let mut out = SPin::new(
        &clean(&p.number, n),
        &name,
        electrical(p.electrical, n),
        [p.position.x, p.position.y],
        orientation(p.angle, n),
        p.length,
    );
    out.designator_visible = !s.pin_numbers_hidden && !p.number_hidden;
    out.name_visible = !(s.pin_names_hidden || p.name_hidden || nameless);
    out.hidden = p.hidden;
    out.part_number = unit;
    out.body_style = style;
    let d = decorations(p.style, n);
    out.decorations = d.map(str::to_string);
    if !p.alternates.is_empty() {
        n.note(
            "symbol.pin_alternates_approximated",
            Severity::Approximated,
            "alternate pin functions written as names only; their types and styles are not kept",
        );
    }
    for a in &p.alternates {
        let (name, changed) = snx_writer::canon::clean_cell(&a.name);
        let name = name.replace('|', "/");
        if changed || a.name.contains('|') {
            n.note(
                "symbol.text_sanitized",
                Severity::Approximated,
                "control characters in a pin text replaced by spaces",
            );
        }
        out.function.push(name);
    }
    out
}

fn rgba(c: [f64; 4]) -> [u8; 4] {
    let byte = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    [
        byte(c[0]),
        byte(c[1]),
        byte(c[2]),
        byte((c[3] * 255.0).round()),
    ]
}

/// A fill on a shape that cannot take one is dropped and reported.
fn lost_fill(f: Fill, n: &mut Notes<'_>) {
    if f != Fill::None {
        n.note(
            "symbol.fill_dropped",
            Severity::Dropped,
            "fill on a shape Signex cannot fill (open polyline, arc, bezier) dropped",
        );
    }
}

fn fill(f: Fill, n: &mut Notes<'_>) -> Option<[u8; 4]> {
    match f {
        Fill::None => None,
        Fill::Color(c) => Some(rgba(c)),
        Fill::Background => {
            n.note(
                "symbol.fill_approximated",
                Severity::Approximated,
                "theme fill written as Signex's default fill colour",
            );
            Some(BACKGROUND_FILL)
        }
        Fill::Outline => {
            n.note(
                "symbol.fill_approximated",
                Severity::Approximated,
                "outline fill written as a black fill",
            );
            Some(OUTLINE_FILL)
        }
    }
}

fn pt(p: Point) -> [f64; 2] {
    [p.x, p.y]
}

fn graphic(g: &Graphic, unit: u8, style: u8, n: &mut Notes<'_>) -> Vec<G> {
    let make = |stroke_width: f64, fill: Option<[u8; 4]>, kind: K| G {
        stroke_width,
        fill,
        part_number: unit,
        body_style: style,
        kind,
    };
    match g {
        Graphic::Rectangle {
            start,
            end,
            stroke,
            fill: f,
        } => {
            vec![make(
                stroke.width,
                fill(*f, n),
                K::Rectangle {
                    from: pt(*start),
                    to: pt(*end),
                },
            )]
        }
        Graphic::Circle {
            center,
            radius,
            stroke,
            fill: f,
        } => {
            vec![make(
                stroke.width,
                fill(*f, n),
                K::Circle {
                    center: pt(*center),
                    radius: *radius,
                },
            )]
        }
        Graphic::Arc {
            start,
            mid,
            end,
            stroke,
            fill: f,
        } => match arc(*start, *mid, *end) {
            Some(kind) => {
                // An arc is not a closed shape: a fill on it cannot be written.
                lost_fill(*f, n);
                vec![make(stroke.width, None, kind)]
            }
            None => {
                n.note(
                    "symbol.arc_degenerate",
                    Severity::Approximated,
                    "arc through collinear points written as a line",
                );
                lost_fill(*f, n);
                vec![make(
                    stroke.width,
                    None,
                    K::Line {
                        from: pt(*start),
                        to: pt(*end),
                    },
                )]
            }
        },
        Graphic::Polyline {
            points,
            stroke,
            fill: f,
        } => {
            let closed = points.len() >= 4 && points.first() == points.last();
            if closed {
                let vertices = points[..points.len() - 1].iter().map(|p| pt(*p)).collect();
                vec![make(stroke.width, fill(*f, n), K::Polygon { vertices })]
            } else {
                lost_fill(*f, n);
                points
                    .windows(2)
                    .map(|w| {
                        make(
                            stroke.width,
                            None,
                            K::Line {
                                from: pt(w[0]),
                                to: pt(w[1]),
                            },
                        )
                    })
                    .collect()
            }
        }
        Graphic::Bezier {
            points,
            stroke,
            fill: f,
        } => {
            // Signex never fills a bezier.
            lost_fill(*f, n);
            vec![make(
                stroke.width,
                None,
                K::Bezier {
                    points: points.iter().map(|p| pt(*p)).collect(),
                },
            )]
        }
        Graphic::Text {
            text,
            position,
            angle,
            effects,
        } => {
            if angle.rem_euclid(360.0) != 0.0 {
                n.note(
                    "symbol.text_rotation_dropped",
                    Severity::Approximated,
                    "graphic text rotation not carried; the text is written upright",
                );
            }
            let content = clean_text(text, n);
            vec![make(
                0.0,
                None,
                K::Text {
                    position: pt(*position),
                    content,
                    size: effects.size.map_or(1.27, |s| s.0),
                },
            )]
        }
    }
}

fn clean_text(text: &str, n: &mut Notes<'_>) -> String {
    let (out, changed) = snx_writer::canon::clean_cell(text);
    if changed {
        n.note(
            "symbol.text_sanitized",
            Severity::Approximated,
            "control characters in a pin text replaced by spaces",
        );
    }
    out
}

/// The arc from `start` through `mid` to `end` as centre, radius and the
/// counter-clockwise sweep `start_deg` to `end_deg`, angles in `[0, 360)`.
/// `None` for collinear points.
fn arc(start: Point, mid: Point, end: Point) -> Option<K> {
    let d =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let sq = |p: Point| p.x * p.x + p.y * p.y;
    let cx =
        (sq(start) * (mid.y - end.y) + sq(mid) * (end.y - start.y) + sq(end) * (start.y - mid.y))
            / d;
    let cy =
        (sq(start) * (end.x - mid.x) + sq(mid) * (start.x - end.x) + sq(end) * (mid.x - start.x))
            / d;
    // Rounded to 1e-9 degrees so float noise does not show (359.99999999999994).
    let angle = |p: Point| {
        let deg = (p.y - cy).atan2(p.x - cx).to_degrees().rem_euclid(360.0);
        ((deg * 1e9).round() / 1e9).rem_euclid(360.0)
    };
    let (a_start, a_mid, a_end) = (angle(start), angle(mid), angle(end));
    let ccw = (a_mid - a_start).rem_euclid(360.0) <= (a_end - a_start).rem_euclid(360.0);
    let (start_deg, end_deg) = if ccw {
        (a_start, a_end)
    } else {
        (a_end, a_start)
    };
    Some(K::Arc {
        center: [cx, cy],
        radius: (start.x - cx).hypot(start.y - cy),
        start_deg,
        end_deg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arc_keeps_its_sweep_whichever_way_it_was_drawn() {
        let p = |x, y| Point { x, y };
        // Quarter circle of radius 1 about the origin, from (1,0) over (.707,.707) to (0,1): CCW.
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let Some(K::Arc {
            start_deg,
            end_deg,
            radius,
            ..
        }) = arc(p(1.0, 0.0), p(h, h), p(0.0, 1.0))
        else {
            panic!()
        };
        assert!(
            (start_deg - 0.0).abs() < 1e-9
                && (end_deg - 90.0).abs() < 1e-9
                && (radius - 1.0).abs() < 1e-9
        );
        // The same arc drawn from (0,1) to (1,0) is clockwise: start and end swap.
        let Some(K::Arc {
            start_deg, end_deg, ..
        }) = arc(p(0.0, 1.0), p(h, h), p(1.0, 0.0))
        else {
            panic!()
        };
        assert!((start_deg - 0.0).abs() < 1e-9 && (end_deg - 90.0).abs() < 1e-9);
        assert!(arc(p(0.0, 0.0), p(1.0, 1.0), p(2.0, 2.0)).is_none());
    }
}
