// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! In-memory model of a KiCad symbol library, as the file states it.
//!
//! Coordinates and lengths are millimetres and keep the file's axes (a
//! symbol library's y axis points up); nothing is converted here.

/// A point in the file's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SymbolLib {
    /// The `(version YYYYMMDD)` token, as written.
    pub version: Option<String>,
    pub generator: Option<String>,
    /// Top-level symbols in file order (several files in directory order).
    pub symbols: Vec<Symbol>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    /// The parent's name when this symbol is `(extends "PARENT")`.
    pub extends: Option<String>,
    /// `(power)` was present; `power_scope` is its `global` / `local` word.
    pub power: bool,
    pub power_scope: Option<String>,
    /// `(in_bom ...)`; true when the token is absent.
    pub in_bom: bool,
    /// `(on_board ...)`; true when the token is absent.
    pub on_board: bool,
    /// `(exclude_from_sim ...)`, `None` when the token is absent.
    pub exclude_from_sim: Option<bool>,
    pub pin_numbers_hidden: bool,
    pub pin_names_hidden: bool,
    /// `(pin_names (offset N))`, `None` when not stated.
    pub pin_names_offset: Option<f64>,
    pub properties: Vec<Property>,
    /// The symbol's own units, in file order; empty for a derived symbol.
    pub units: Vec<Unit>,
}

impl Symbol {
    pub fn new(name: impl Into<String>) -> Self {
        Symbol {
            name: name.into(),
            extends: None,
            power: false,
            power_scope: None,
            in_bom: true,
            on_board: true,
            exclude_from_sim: None,
            pin_numbers_hidden: false,
            pin_names_hidden: false,
            pin_names_offset: None,
            properties: Vec::new(),
            units: Vec::new(),
        }
    }

    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.value.as_str())
    }

    /// `Description`, or the older reserved key `ki_description`.
    pub fn description(&self) -> Option<&str> {
        self.property("Description")
            .or_else(|| self.property("ki_description"))
    }

    /// `ki_keywords`: space separated words, as written.
    pub fn keywords(&self) -> Option<&str> {
        self.property("ki_keywords")
    }

    /// `ki_fp_filters`: space separated footprint name patterns, as written.
    pub fn footprint_filters(&self) -> Option<&str> {
        self.property("ki_fp_filters")
    }

    pub fn pin_count(&self) -> usize {
        self.units.iter().map(|u| u.pins.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: String,
    pub value: String,
    /// `(at X Y ANGLE)`.
    pub position: Option<(Point, f64)>,
    /// Hidden by `(effects ... hide)` or a `(hide yes)` of its own.
    pub hidden: bool,
    pub effects: TextEffects,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextEffects {
    /// `(font (size H W))`.
    pub size: Option<(f64, f64)>,
    pub hidden: bool,
    pub bold: bool,
    pub italic: bool,
    /// The `(justify ...)` words, as written.
    pub justify: Vec<String>,
}

/// One `NAME_UNIT_STYLE` child of a symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    /// 0 is common to every unit; 1.. are the units.
    pub unit: u32,
    /// The body style: 1 normal, 2 De Morgan alternate. 0 only for graphics
    /// written straight in the symbol, outside any child.
    pub style: u32,
    /// `(unit_name "...")`.
    pub name: Option<String>,
    pub graphics: Vec<Graphic>,
    pub pins: Vec<Pin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinType {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
    Free,
    Unspecified,
    PowerIn,
    PowerOut,
    OpenCollector,
    OpenEmitter,
    NoConnect,
}

impl PinType {
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "input" => PinType::Input,
            "output" => PinType::Output,
            "bidirectional" => PinType::Bidirectional,
            "tri_state" => PinType::TriState,
            "passive" => PinType::Passive,
            "free" => PinType::Free,
            "unspecified" => PinType::Unspecified,
            "power_in" => PinType::PowerIn,
            "power_out" => PinType::PowerOut,
            "open_collector" => PinType::OpenCollector,
            "open_emitter" => PinType::OpenEmitter,
            "no_connect" => PinType::NoConnect,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinStyle {
    Line,
    Inverted,
    Clock,
    InvertedClock,
    InputLow,
    ClockLow,
    OutputLow,
    EdgeClockHigh,
    NonLogic,
}

impl PinStyle {
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "line" => PinStyle::Line,
            "inverted" => PinStyle::Inverted,
            "clock" => PinStyle::Clock,
            "inverted_clock" => PinStyle::InvertedClock,
            "input_low" => PinStyle::InputLow,
            "clock_low" => PinStyle::ClockLow,
            "output_low" => PinStyle::OutputLow,
            "edge_clock_high" => PinStyle::EdgeClockHigh,
            "non_logic" => PinStyle::NonLogic,
            _ => return None,
        })
    }
}

/// `(alternate "NAME" TYPE STYLE)`: another function the pin can take.
#[derive(Debug, Clone, PartialEq)]
pub struct PinAlternate {
    pub name: String,
    pub electrical: PinType,
    pub style: PinStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    pub electrical: PinType,
    pub style: PinStyle,
    /// The pin's connection point.
    pub position: Point,
    /// 0, 90, 180 or 270 for a well-formed pin.
    pub angle: f64,
    pub length: f64,
    pub name: String,
    pub number: String,
    pub name_hidden: bool,
    pub number_hidden: bool,
    /// The pin itself is hidden (`hide` or `(hide yes)`).
    pub hidden: bool,
    pub alternates: Vec<PinAlternate>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stroke {
    pub width: f64,
    /// `(type ...)`: `default`, `solid`, `dash`, ... as written.
    pub kind: Option<String>,
    /// `(color R G B A)` as written (components 0-255, alpha 0-1).
    pub color: Option<[f64; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Fill {
    #[default]
    None,
    Outline,
    Background,
    /// `(type color)` with its `(color R G B A)`.
    Color([f64; 4]),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Graphic {
    Rectangle {
        start: Point,
        end: Point,
        stroke: Stroke,
        fill: Fill,
    },
    Circle {
        center: Point,
        radius: f64,
        stroke: Stroke,
        fill: Fill,
    },
    Arc {
        start: Point,
        mid: Point,
        end: Point,
        stroke: Stroke,
        fill: Fill,
    },
    Polyline {
        points: Vec<Point>,
        stroke: Stroke,
        fill: Fill,
    },
    Bezier {
        points: Vec<Point>,
        stroke: Stroke,
        fill: Fill,
    },
    Text {
        text: String,
        position: Point,
        angle: f64,
        effects: TextEffects,
    },
}

impl SymbolLib {
    /// The top-level symbol called `name`.
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// The units that draw `symbol`: its own, or those of the nearest
    /// ancestor along `extends` that has any. `None` when the chain breaks
    /// (a missing parent or a cycle).
    pub fn effective_units<'a>(&'a self, symbol: &'a Symbol) -> Option<&'a [Unit]> {
        let mut current = symbol;
        for _ in 0..=self.symbols.len() {
            if !current.units.is_empty() {
                return Some(&current.units);
            }
            match &current.extends {
                None => return Some(&current.units),
                Some(parent) => current = self.symbol(parent)?,
            }
        }
        None
    }
}
