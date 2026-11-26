// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! One top-level `(symbol ...)` into a `Symbol`, reporting what it skips.

use serde_json::json;

use super::model::{Symbol, Unit};
use super::parts;
use crate::items::{Item, ItemKind, Severity};
use crate::sexpr::SExpr;

/// Where report items of one library come from.
pub struct Sink<'a> {
    pub library: &'a str,
    pub items: &'a mut Vec<Item>,
}

impl Sink<'_> {
    pub fn add(
        &mut self,
        symbol: &str,
        code: &str,
        severity: Severity,
        message: String,
        detail: serde_json::Value,
    ) {
        let source = if symbol.is_empty() {
            self.library.to_string()
        } else {
            format!("{}:{symbol}", self.library)
        };
        let kind = if symbol.is_empty() {
            ItemKind::Library
        } else {
            ItemKind::Symbol
        };
        self.items
            .push(Item::new(kind, source, code, severity, message).with_detail(detail));
    }
}

/// Tokens of a symbol that are read (or knowingly ignored) elsewhere.
const KNOWN_SYMBOL_TOKENS: &[&str] = &[
    "extends",
    "power",
    "in_bom",
    "on_board",
    "exclude_from_sim",
    "pin_numbers",
    "pin_names",
    "property",
    "symbol",
    "unit_name",
    "embedded_fonts",
];

pub fn parse_symbol(node: &SExpr, sink: &mut Sink<'_>) -> Option<Symbol> {
    let Some(name) = node.first_arg().map(str::to_string) else {
        sink.add(
            "",
            "symbol.name_missing",
            Severity::Error,
            "symbol without a name".into(),
            json!({}),
        );
        return None;
    };
    let mut symbol = Symbol::new(&name);
    symbol.extends = node
        .find("extends")
        .and_then(SExpr::first_arg)
        .map(str::to_string);
    if let Some(p) = node.find("power") {
        symbol.power = true;
        symbol.power_scope = p.first_arg().map(str::to_string);
    }
    symbol.in_bom = parts::flag(node, "in_bom").unwrap_or(true);
    symbol.on_board = parts::flag(node, "on_board").unwrap_or(true);
    symbol.exclude_from_sim = parts::flag(node, "exclude_from_sim");
    if let Some(n) = node.find("pin_numbers") {
        symbol.pin_numbers_hidden = parts::flag(n, "hide").unwrap_or(false);
    }
    if let Some(n) = node.find("pin_names") {
        symbol.pin_names_hidden = parts::flag(n, "hide").unwrap_or(false);
        symbol.pin_names_offset = n.find("offset").and_then(|o| o.arg_f64(0));
    }
    for p in node.find_all("property") {
        match parts::property(p) {
            Some(prop) => symbol.properties.push(prop),
            None => sink.add(
                &name,
                "symbol.property_unreadable",
                Severity::Dropped,
                "property without a name dropped".into(),
                json!({}),
            ),
        }
    }
    // Items written straight in the symbol, outside any child unit.
    let mut own = Unit {
        unit: 0,
        style: 0,
        name: None,
        graphics: Vec::new(),
        pins: Vec::new(),
    };
    read_items(node, &name, &mut own, sink, false);
    if !own.graphics.is_empty() || !own.pins.is_empty() {
        sink.add(
            &name,
            "symbol.items_outside_unit",
            Severity::Info,
            "graphics or pins written outside any unit kept as unit 0 style 0".into(),
            json!({}),
        );
        symbol.units.push(own);
    }
    for child in node.find_all("symbol") {
        if let Some(unit) = parse_unit(child, &name, sink) {
            symbol.units.push(unit);
        }
    }
    for child in node.children() {
        if let Some(k) = child.keyword()
            && !KNOWN_SYMBOL_TOKENS.contains(&k)
            && parts::graphic(child).is_none()
            && k != "pin"
        {
            sink.add(
                &name,
                "symbol.token_unknown",
                Severity::Info,
                format!("unknown symbol token {k} ignored"),
                json!({ "token": k }),
            );
        }
    }
    Some(symbol)
}

/// `(symbol "PARENT_U_S" ...)`: the unit and body style come from the
/// name's last two `_` separated numbers.
fn parse_unit(node: &SExpr, parent: &str, sink: &mut Sink<'_>) -> Option<Unit> {
    let id = node.first_arg().unwrap_or("");
    let mut parts_of = id.rsplitn(3, '_');
    let style = parts_of.next().and_then(|s| s.parse::<u32>().ok());
    let unit = parts_of.next().and_then(|s| s.parse::<u32>().ok());
    let prefix = parts_of.next();
    let (Some(unit), Some(style)) = (unit, style) else {
        sink.add(
            parent,
            "symbol.unit_unreadable",
            Severity::Dropped,
            format!("unit {id} does not read as NAME_UNIT_STYLE; dropped"),
            json!({ "unit_id": id }),
        );
        return None;
    };
    if prefix != Some(parent) {
        sink.add(
            parent,
            "symbol.unit_name_mismatch",
            Severity::Info,
            format!("unit {id} is not named after its symbol {parent}"),
            json!({ "unit_id": id }),
        );
    }
    if style > 2 {
        sink.add(
            parent,
            "symbol.body_style_beyond_two",
            Severity::Info,
            format!("unit {id} has body style {style}; KiCad defines styles 1 and 2"),
            json!({ "unit_id": id, "style": style }),
        );
    }
    let mut out = Unit {
        unit,
        style,
        name: node
            .find("unit_name")
            .and_then(SExpr::first_arg)
            .map(str::to_string),
        graphics: Vec::new(),
        pins: Vec::new(),
    };
    read_items(node, parent, &mut out, sink, true);
    Some(out)
}

fn read_items(node: &SExpr, symbol: &str, out: &mut Unit, sink: &mut Sink<'_>, in_unit: bool) {
    for child in node.children() {
        let Some(keyword) = child.keyword() else {
            continue;
        };
        if keyword == "pin" {
            match parts::pin(child) {
                Some((pin, notes)) => {
                    if let Some(w) = &notes.unknown_type {
                        sink.add(
                            symbol,
                            "symbol.pin_type_unknown",
                            Severity::Info,
                            format!("pin type {w:?} read as unspecified"),
                            json!({ "pin": pin.number, "word": w }),
                        );
                    }
                    if let Some(w) = &notes.unknown_style {
                        sink.add(
                            symbol,
                            "symbol.pin_style_unknown",
                            Severity::Info,
                            format!("pin style {w:?} read as line"),
                            json!({ "pin": pin.number, "word": w }),
                        );
                    }
                    if notes.odd_angle {
                        sink.add(
                            symbol,
                            "symbol.pin_angle_unusual",
                            Severity::Info,
                            "pin angle is not 0, 90, 180 or 270".into(),
                            json!({ "pin": pin.number, "angle": pin.angle }),
                        );
                    }
                    if notes.no_length {
                        sink.add(
                            symbol,
                            "symbol.pin_length_missing",
                            Severity::Info,
                            "pin without a length read as 0".into(),
                            json!({ "pin": pin.number }),
                        );
                    }
                    out.pins.push(pin);
                }
                None => sink.add(
                    symbol,
                    "symbol.pin_unreadable",
                    Severity::Dropped,
                    "pin without a position dropped".into(),
                    json!({}),
                ),
            }
            continue;
        }
        match parts::graphic(child) {
            Some(Ok((graphic, bad_fill))) => {
                if let Some(w) = bad_fill {
                    sink.add(
                        symbol,
                        "symbol.fill_unknown",
                        Severity::Info,
                        format!("fill type {w:?} read as none"),
                        json!({ "word": w }),
                    );
                }
                out.graphics.push(graphic);
            }
            Some(Err(kind)) => sink.add(
                symbol,
                "symbol.graphic_unreadable",
                Severity::Dropped,
                format!("{kind} without its geometry dropped"),
                json!({ "graphic": kind }),
            ),
            None if in_unit && keyword != "unit_name" => sink.add(
                symbol,
                "symbol.unit_token_unknown",
                Severity::Info,
                format!("unknown token {keyword} in a unit ignored"),
                json!({ "token": keyword }),
            ),
            None => {}
        }
    }
}
