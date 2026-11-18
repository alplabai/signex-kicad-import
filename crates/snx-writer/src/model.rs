// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The board as the writer takes it. Lengths are millimetres, angles are
//! degrees in the Signex sense (counter-clockwise in model axes, y down),
//! layer names are Signex layer tokens.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// One layer of the `[stackup]` list.
#[derive(Debug, Clone, PartialEq)]
pub struct StackLayer {
    /// The layer token: `TopCopper`, `Inner1Copper`, `TopSilkscreen`, ...
    pub token: String,
    /// `copper`, `plane`, or empty for a non-copper layer.
    pub layer_type: String,
}

/// Board-wide defaults (`[stackup.setup]`), millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Setup {
    pub grid_size: f64,
    pub trace_width: f64,
    pub via_diameter: f64,
    pub via_drill: f64,
    pub clearance: f64,
    pub track_min_width: f64,
    pub via_min_diameter: f64,
    pub via_min_drill: f64,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            grid_size: 1.27,
            trace_width: 0.25,
            via_diameter: 0.6,
            via_drill: 0.3,
            clearance: 0.2,
            track_min_width: 0.1,
            via_min_diameter: 0.4,
            via_min_drill: 0.2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Net {
    pub number: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadType {
    Thru,
    Smd,
    Connect,
    NpThru,
}

impl PadType {
    pub fn token(self) -> &'static str {
        match self {
            Self::Thru => "thru",
            Self::Smd => "smd",
            Self::Connect => "connect",
            Self::NpThru => "np_thru",
        }
    }
}

/// A pad's copper outline. Custom corners are in the pad's own frame.
#[derive(Debug, Clone, PartialEq)]
pub enum PadShape {
    Circle,
    Rect,
    Oval,
    /// Corner radius as a fraction of the shorter side, `0.0..=0.5`.
    RoundRect(f64),
    Custom(Vec<Point>),
}

impl PadShape {
    pub fn token(&self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Rect => "rect",
            Self::Oval => "oval",
            Self::RoundRect(_) => "roundrect",
            Self::Custom(_) => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    pub uuid: Uuid,
    pub number: String,
    pub pad_type: PadType,
    pub shape: PadShape,
    /// Footprint-local, unmirrored.
    pub position: Point,
    pub size: Point,
    /// The pad's own turn in the footprint frame.
    pub rotation: f64,
    /// Hole diameter; `None` for no hole.
    pub drill: Option<f64>,
    /// `oval` for a slot; empty for a round hole.
    pub drill_shape: String,
    pub layers: Vec<String>,
    /// Net number and name; `None` for no net.
    pub net: Option<Net>,
}

/// A drawing inside a footprint, footprint-local.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FootprintGraphic {
    /// `line`, `rect`, `circle`, `arc`, `poly` or `text`.
    pub graphic_type: String,
    pub layer: String,
    pub width: f64,
    pub start: Option<Point>,
    pub end: Option<Point>,
    pub center: Option<Point>,
    pub mid: Option<Point>,
    pub radius: f64,
    pub points: Vec<Point>,
    pub text: String,
    pub font_size: f64,
    pub position: Option<Point>,
    pub rotation: f64,
    /// `solid` for a filled shape, empty otherwise.
    pub fill: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Property {
    pub key: String,
    pub value: String,
    pub position: Option<Point>,
    pub rotation: f64,
    pub layer: Option<String>,
    pub font_size: Option<f64>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub uuid: Uuid,
    pub reference: String,
    pub value: String,
    pub footprint_id: String,
    pub position: Point,
    pub rotation: f64,
    /// `TopCopper` or `BottomCopper`: the side.
    pub layer: String,
    pub locked: bool,
    pub pads: Vec<Pad>,
    pub graphics: Vec<FootprintGraphic>,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub uuid: Uuid,
    pub net: u32,
    pub layer: String,
    pub width: f64,
    pub start: Point,
    pub end: Point,
}

/// A copper arc through three points.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackArc {
    pub uuid: Uuid,
    pub net: u32,
    pub layer: String,
    pub width: f64,
    pub start: Point,
    pub mid: Point,
    pub end: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViaType {
    Through,
    Blind,
    Micro,
}

impl ViaType {
    pub fn token(self) -> &'static str {
        match self {
            Self::Through => "through",
            Self::Blind => "blind",
            Self::Micro => "micro",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Via {
    pub uuid: Uuid,
    pub net: u32,
    pub position: Point,
    pub drill: f64,
    pub diameter: f64,
    /// The copper layers it spans, outermost first and last.
    pub layers: Vec<String>,
    pub via_type: ViaType,
}

/// What a zone is on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    /// Copper poured inside the outline, clear of other nets.
    Pour,
    /// A rule area that keeps copper out.
    Keepout,
    /// Solid copper exactly the outline (a filled copper polygon).
    Solid,
}

impl ZoneKind {
    pub fn fill_type(self) -> &'static str {
        match self {
            Self::Pour => "pour",
            Self::Keepout => "keepout",
            Self::Solid => "solid",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub uuid: Uuid,
    pub kind: ZoneKind,
    pub net: u32,
    pub net_name: String,
    pub layer: String,
    pub outline: Vec<Point>,
    pub priority: u32,
    pub thermal_relief: bool,
    pub thermal_gap: f64,
    pub thermal_width: f64,
    pub clearance: f64,
    pub min_thickness: f64,
    pub remove_dead_islands: bool,
    /// Smallest island kept, mm²; 0 keeps every island.
    pub min_island_area: f64,
}

/// A drawing on the board, board coordinates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoardGraphic {
    pub uuid: Uuid,
    /// `line`, `rect`, `circle`, `arc` or `poly`.
    pub graphic_type: String,
    pub layer: String,
    pub width: f64,
    pub start: Option<Point>,
    pub end: Option<Point>,
    pub center: Option<Point>,
    pub radius: f64,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardText {
    pub uuid: Uuid,
    pub text: String,
    pub position: Point,
    pub layer: String,
    pub font_size: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub uuid: Uuid,
    /// Board thickness, mm.
    pub thickness: f64,
    pub layers: Vec<StackLayer>,
    pub setup: Setup,
    pub nets: Vec<Net>,
    pub footprints: Vec<Footprint>,
    pub tracks: Vec<Track>,
    pub arcs: Vec<TrackArc>,
    pub vias: Vec<Via>,
    pub zones: Vec<Zone>,
    /// The board edge as one closed ring (the last corner joins the first).
    pub outline: Vec<Point>,
    pub graphics: Vec<BoardGraphic>,
    pub texts: Vec<BoardText>,
}

/// A net class for the `.snxprj` sidecar, millimetres; 0 means unset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NetClass {
    pub name: String,
    pub nets: Vec<String>,
    pub patterns: Vec<String>,
    pub clearance: f64,
    pub trace_width: f64,
    pub via_diameter: f64,
    pub via_drill: f64,
    pub diff_pair_gap: f64,
    pub diff_pair_width: f64,
}
