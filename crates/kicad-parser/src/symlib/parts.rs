// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The small pieces of a symbol: points, strokes, fills, text effects,
//! properties, pins and graphic items. Each reader returns `None` for an
//! element it cannot read; the caller reports that.

use super::model::{
    Fill, Graphic, Pin, PinAlternate, PinStyle, PinType, Point, Property, Stroke, TextEffects,
};
use crate::sexpr::{Atom, SExpr};

/// `(key X Y)` as a point.
pub fn point(node: &SExpr, key: &str) -> Option<Point> {
    let n = node.find(key)?;
    Some(Point {
        x: n.arg_f64(0)?,
        y: n.arg_f64(1)?,
    })
}

/// The points of `(pts (xy X Y) ...)`.
pub fn points(node: &SExpr) -> Option<Vec<Point>> {
    node.find("pts")?
        .find_all("xy")
        .into_iter()
        .map(|xy| {
            Some(Point {
                x: xy.arg_f64(0)?,
                y: xy.arg_f64(1)?,
            })
        })
        .collect()
}

/// A yes/no or presence flag: the bare word `word`, or `(word yes|no)`.
/// `None` when neither is there.
pub fn flag(node: &SExpr, word: &str) -> Option<bool> {
    for child in node.children() {
        match child {
            SExpr::Atom(Atom::Raw(w)) if w == word => return Some(true),
            SExpr::List(_) if child.keyword() == Some(word) => {
                return Some(!matches!(child.first_arg(), Some("no" | "false")));
            }
            _ => {}
        }
    }
    None
}

/// `(at X Y [ANGLE])`.
pub fn at(node: &SExpr) -> Option<(Point, f64)> {
    let n = node.find("at")?;
    Some((
        Point {
            x: n.arg_f64(0)?,
            y: n.arg_f64(1)?,
        },
        n.arg_f64(2).unwrap_or(0.0),
    ))
}

pub fn effects(node: &SExpr) -> TextEffects {
    let Some(e) = node.find("effects") else {
        return TextEffects::default();
    };
    let font = e.find("font");
    TextEffects {
        size: font
            .and_then(|f| f.find("size"))
            .and_then(|s| Some((s.arg_f64(0)?, s.arg_f64(1)?))),
        hidden: flag(e, "hide").unwrap_or(false),
        bold: font.and_then(|f| flag(f, "bold")).unwrap_or(false),
        italic: font.and_then(|f| flag(f, "italic")).unwrap_or(false),
        justify: e
            .find("justify")
            .map(|j| {
                j.children()
                    .iter()
                    .filter_map(|c| match c {
                        SExpr::Atom(a) => Some(a.as_str().to_string()),
                        SExpr::List(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn color(node: &SExpr) -> Option<[f64; 4]> {
    let c = node.find("color")?;
    Some([c.arg_f64(0)?, c.arg_f64(1)?, c.arg_f64(2)?, c.arg_f64(3)?])
}

pub fn stroke(node: &SExpr) -> Stroke {
    let Some(s) = node.find("stroke") else {
        return Stroke::default();
    };
    Stroke {
        width: s.find("width").and_then(|w| w.arg_f64(0)).unwrap_or(0.0),
        kind: s
            .find("type")
            .and_then(SExpr::first_arg)
            .map(str::to_string),
        color: color(s),
    }
}

/// The fill and whether its type word was not one the format lists.
pub fn fill(node: &SExpr) -> (Fill, Option<String>) {
    let Some(f) = node.find("fill") else {
        return (Fill::None, None);
    };
    match f.find("type").and_then(SExpr::first_arg) {
        None | Some("none") => (Fill::None, None),
        Some("outline") => (Fill::Outline, None),
        Some("background") => (Fill::Background, None),
        Some("color") => match color(f) {
            Some(c) => (Fill::Color(c), None),
            None => (Fill::None, Some("color".to_string())),
        },
        Some(other) => (Fill::None, Some(other.to_string())),
    }
}

pub fn property(node: &SExpr) -> Option<Property> {
    let key = node.arg(0)?.to_string();
    let value = node.arg(1).unwrap_or("").to_string();
    let effects = effects(node);
    Some(Property {
        key,
        value,
        position: at(node),
        hidden: effects.hidden || flag(node, "hide").unwrap_or(false),
        effects,
    })
}

/// Words of a pin a reader did not recognise, for the report.
#[derive(Default)]
pub struct PinNotes {
    pub unknown_type: Option<String>,
    pub unknown_style: Option<String>,
    pub odd_angle: bool,
    pub no_length: bool,
}

/// A `(pin TYPE STYLE (at ..) (length ..) (name ..) (number ..) ...)`; `None`
/// when it has no position.
pub fn pin(node: &SExpr) -> Option<(Pin, PinNotes)> {
    let mut notes = PinNotes::default();
    let (position, angle) = at(node)?;
    let (electrical, style) =
        type_and_style(node, 0, &mut notes.unknown_type, &mut notes.unknown_style);
    let text = |key: &str| {
        node.find(key)
            .map(|n| (n.arg(0).unwrap_or("").to_string(), effects(n).hidden))
            .unwrap_or_default()
    };
    let (name, name_hidden) = text("name");
    let (number, number_hidden) = text("number");
    let length = node.find("length").and_then(|l| l.arg_f64(0));
    notes.no_length = length.is_none();
    notes.odd_angle = ![0.0, 90.0, 180.0, 270.0].contains(&angle.rem_euclid(360.0));
    let alternates = node
        .find_all("alternate")
        .into_iter()
        .filter_map(|a| {
            let (mut bad_type, mut bad_style) = (None, None);
            let (electrical, style) = type_and_style(a, 1, &mut bad_type, &mut bad_style);
            Some(PinAlternate {
                name: a.arg(0)?.to_string(),
                electrical,
                style,
            })
        })
        .collect();
    let pin = Pin {
        electrical,
        style,
        position,
        angle: angle.rem_euclid(360.0),
        length: length.unwrap_or(0.0),
        name,
        number,
        name_hidden,
        number_hidden,
        hidden: flag(node, "hide").unwrap_or(false),
        alternates,
    };
    Some((pin, notes))
}

/// The two bare words at argument `first` and `first + 1`: electrical type
/// then graphic style. An unlisted word is returned in `bad_*` and replaced
/// by `unspecified` / `line`.
fn type_and_style(
    node: &SExpr,
    first: usize,
    bad_type: &mut Option<String>,
    bad_style: &mut Option<String>,
) -> (PinType, PinStyle) {
    let electrical = match node.arg(first) {
        Some(w) => PinType::from_token(w).unwrap_or_else(|| {
            *bad_type = Some(w.to_string());
            PinType::Unspecified
        }),
        None => {
            *bad_type = Some(String::new());
            PinType::Unspecified
        }
    };
    let style = match node.arg(first + 1) {
        Some(w) => PinStyle::from_token(w).unwrap_or_else(|| {
            *bad_style = Some(w.to_string());
            PinStyle::Line
        }),
        None => PinStyle::Line,
    };
    (electrical, style)
}

/// A graphic item of a symbol: the model value, plus the fill type word the
/// reader did not recognise. `None` for an unknown keyword (the caller
/// reports it) or an unreadable item (`Some(Err(keyword))`).
pub fn graphic(node: &SExpr) -> Option<Result<(Graphic, Option<String>), String>> {
    let kind = node.keyword()?;
    let unreadable = || Err(kind.to_string());
    let build = |g: Option<(Graphic, Option<String>)>| g.map_or_else(unreadable, Ok);
    Some(match kind {
        "rectangle" => build(
            point(node, "start")
                .zip(point(node, "end"))
                .map(|(start, end)| {
                    let (fill, bad) = fill(node);
                    (
                        Graphic::Rectangle {
                            start,
                            end,
                            stroke: stroke(node),
                            fill,
                        },
                        bad,
                    )
                }),
        ),
        "circle" => build(
            point(node, "center")
                .zip(node.find("radius").and_then(|r| r.arg_f64(0)))
                .map(|(center, radius)| {
                    let (fill, bad) = fill(node);
                    (
                        Graphic::Circle {
                            center,
                            radius,
                            stroke: stroke(node),
                            fill,
                        },
                        bad,
                    )
                }),
        ),
        "arc" => build(
            point(node, "start")
                .zip(point(node, "mid"))
                .zip(point(node, "end"))
                .map(|((start, mid), end)| {
                    let (fill, bad) = fill(node);
                    (
                        Graphic::Arc {
                            start,
                            mid,
                            end,
                            stroke: stroke(node),
                            fill,
                        },
                        bad,
                    )
                }),
        ),
        "polyline" => build(points(node).filter(|p| p.len() >= 2).map(|points| {
            let (fill, bad) = fill(node);
            (
                Graphic::Polyline {
                    points,
                    stroke: stroke(node),
                    fill,
                },
                bad,
            )
        })),
        "bezier" => build(points(node).filter(|p| p.len() == 4).map(|points| {
            let (fill, bad) = fill(node);
            (
                Graphic::Bezier {
                    points,
                    stroke: stroke(node),
                    fill,
                },
                bad,
            )
        })),
        "text" => build(node.arg(0).zip(at(node)).map(|(text, (position, angle))| {
            (
                Graphic::Text {
                    text: text.to_string(),
                    position,
                    angle,
                    effects: effects(node),
                },
                None,
            )
        })),
        _ => return None,
    })
}
