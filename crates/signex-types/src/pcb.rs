use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::property::PcbProperty;

pub use crate::schematic::Point;

// ---------------------------------------------------------------------------
// PCB domain constants
// ---------------------------------------------------------------------------

/// Standard PCB board thickness in mm (1.6 mm = 63 mil).
pub const PCB_DEFAULT_THICKNESS_MM: f64 = 1.6;

/// Default PCB grid size in mm (1.27 mm = 50 mil).
pub const PCB_GRID_MM: f64 = 1.27;

/// Default trace / segment width in mm (0.25 mm = ~10 mil).
pub const PCB_DEFAULT_TRACE_WIDTH_MM: f64 = 0.25;

/// Minimum trace width fallback in mm (0.1 mm = ~4 mil).
pub const PCB_TRACK_MIN_MM: f64 = 0.1;

/// Default via finished-hole diameter in mm (0.6 mm = ~24 mil).
pub const PCB_DEFAULT_VIA_DIAMETER_MM: f64 = 0.6;

/// Default via drill diameter in mm (0.3 mm = ~12 mil).
pub const PCB_DEFAULT_VIA_DRILL_MM: f64 = 0.3;

/// Minimum via diameter fallback in mm (0.4 mm = ~16 mil).
pub const PCB_VIA_MIN_DIAMETER_MM: f64 = 0.4;

/// Minimum via drill fallback in mm (0.2 mm = ~8 mil).
pub const PCB_VIA_MIN_DRILL_MM: f64 = 0.2;

/// Default design-rule clearance in mm (0.2 mm = ~8 mil).
pub const PCB_DEFAULT_CLEARANCE_MM: f64 = 0.2;

/// Default copper-zone minimum fill thickness in mm (0.254 mm = 10 mil).
pub const PCB_ZONE_MIN_THICKNESS_MM: f64 = 0.254;

/// Default copper-zone thermal relief gap in mm (0.508 mm = 20 mil).
pub const PCB_THERMAL_GAP_MM: f64 = 0.508;

/// Default copper-zone thermal bridge width in mm (0.254 mm = 10 mil).
pub const PCB_THERMAL_BRIDGE_MM: f64 = 0.254;

/// Default graphic / silkscreen line width in mm (0.1 mm = ~4 mil).
pub const PCB_DEFAULT_GRAPHIC_WIDTH_MM: f64 = 0.1;

/// Default pad size (width and height) in mm (1.0 mm = ~39 mil).
pub const PCB_DEFAULT_PAD_SIZE_MM: f64 = 1.0;

/// Default PCB text / fp_text font size in mm (1.0 mm = ~39 mil).
pub const PCB_DEFAULT_TEXT_SIZE_MM: f64 = 1.0;

/// Stroke thickness for PCB text in mm (0.15 mm = ~6 mil).
pub const PCB_TEXT_THICKNESS_MM: f64 = 0.15;

/// Vertical offset for auto-placed Reference / Value fp_text in mm (2.0 mm = ~79 mil).
pub const PCB_FP_TEXT_OFFSET_MM: f64 = 2.0;

/// Point-coincidence epsilon for outline deduplication (0.01 mm).
pub const PCB_COINCIDENCE_EPS: f64 = 0.01;

// ---------------------------------------------------------------------------
// Pad / Via enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadType {
    Thru,
    Smd,
    Connect,
    NpThru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    Circle,
    Rect,
    Oval,
    Trapezoid,
    RoundRect,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViaType {
    Through,
    Blind,
    Micro,
}

// ---------------------------------------------------------------------------
// Layer definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDef {
    pub id: u8,
    pub name: String,
    pub layer_type: String,
    /// Optional display name for a user layer.
    ///
    /// KiCad board format: <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_layers_section>
    #[serde(default)]
    pub user_name: Option<String>,
}

// ---------------------------------------------------------------------------
// PCB setup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcbSetup {
    #[serde(default)]
    pub grid_size: f64,
    #[serde(default)]
    pub trace_width: f64,
    #[serde(default)]
    pub via_diameter: f64,
    #[serde(default)]
    pub via_drill: f64,
    #[serde(default)]
    pub clearance: f64,
    #[serde(default)]
    pub track_min_width: f64,
    #[serde(default)]
    pub via_min_diameter: f64,
    #[serde(default)]
    pub via_min_drill: f64,
    /// Custom routing-grid origin. This is an origin, not the grid spacing.
    #[serde(default)]
    pub grid_origin: Option<Point>,
    #[serde(default)]
    pub aux_axis_origin: Option<Point>,
    #[serde(default)]
    pub pad_to_mask_clearance: f64,
    #[serde(default)]
    pub solder_mask_min_width: Option<f64>,
    #[serde(default)]
    pub pad_to_paste_clearance: Option<f64>,
    #[serde(default)]
    pub pad_to_paste_clearance_ratio: Option<f64>,
    /// Physical construction of the board.
    ///
    /// KiCad board format: <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_stack_up_settings>
    #[serde(default)]
    pub stackup: Option<BoardStackup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoardStackup {
    #[serde(default)]
    pub layers: Vec<StackupLayer>,
    #[serde(default)]
    pub copper_finish: String,
    #[serde(default)]
    pub dielectric_constraints: bool,
    #[serde(default)]
    pub edge_connector: String,
    #[serde(default)]
    pub castellated_pads: bool,
    #[serde(default)]
    pub edge_plating: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackupLayer {
    pub name: String,
    #[serde(default)]
    pub number: Option<u32>,
    #[serde(default)]
    pub layer_type: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub thickness: Option<f64>,
    #[serde(default)]
    pub material: String,
    #[serde(default)]
    pub epsilon_r: Option<f64>,
    #[serde(default)]
    pub loss_tangent: Option<f64>,
}

// ---------------------------------------------------------------------------
// Net definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetDef {
    pub number: u32,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Drill definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillDef {
    pub diameter: f64,
    #[serde(default)]
    pub shape: String,
    /// Second slot dimension for an oval drill.
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub offset: Point,
}

// ---------------------------------------------------------------------------
// Pad net reference
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PadNet {
    pub number: u32,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Pad
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pad {
    pub uuid: Uuid,
    #[serde(default)]
    pub number: String,
    pub pad_type: PadType,
    pub shape: PadShape,
    pub position: Point,
    #[serde(default)]
    pub rotation: f64,
    pub size: Point,
    pub drill: Option<DrillDef>,
    #[serde(default)]
    pub layers: Vec<String>,
    pub net: Option<PadNet>,
    #[serde(default)]
    pub roundrect_ratio: f64,
    #[serde(default)]
    pub chamfer_ratio: f64,
    #[serde(default)]
    pub chamfer_corners: Vec<String>,
    #[serde(default)]
    pub pin_function: String,
    #[serde(default)]
    pub pin_type: String,
    #[serde(default)]
    pub die_length: Option<f64>,
    #[serde(default)]
    pub solder_mask_margin: Option<f64>,
    #[serde(default)]
    pub solder_paste_margin: Option<f64>,
    #[serde(default)]
    pub solder_paste_margin_ratio: Option<f64>,
    #[serde(default)]
    pub clearance: Option<f64>,
    #[serde(default)]
    pub zone_connect: Option<u8>,
    #[serde(default)]
    pub thermal_width: Option<f64>,
    #[serde(default)]
    pub thermal_gap: Option<f64>,
    /// Custom-pad geometry and clearance behavior.
    ///
    /// KiCad common format: <https://dev-docs.kicad.org/en/file-formats/sexpr-intro/#_footprint_pad>
    #[serde(default)]
    pub custom: Option<CustomPad>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomPad {
    #[serde(default)]
    pub anchor_shape: String,
    #[serde(default)]
    pub clearance_shape: String,
    #[serde(default)]
    pub primitives: Vec<PadPrimitive>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PadPrimitive {
    #[serde(default)]
    pub primitive_type: String,
    #[serde(default)]
    pub start: Option<Point>,
    #[serde(default)]
    pub mid: Option<Point>,
    #[serde(default)]
    pub end: Option<Point>,
    #[serde(default)]
    pub center: Option<Point>,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub fill: bool,
}

// ---------------------------------------------------------------------------
// Footprint graphic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpGraphic {
    #[serde(default)]
    pub graphic_type: String,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub width: f64,
    pub start: Option<Point>,
    pub end: Option<Point>,
    pub center: Option<Point>,
    pub mid: Option<Point>,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub font_size: f64,
    pub position: Option<Point>,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub fill: String,
}

// ---------------------------------------------------------------------------
// Footprint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Footprint {
    pub uuid: Uuid,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub footprint_id: String,
    pub position: Point,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub pads: Vec<Pad>,
    #[serde(default)]
    pub graphics: Vec<FpGraphic>,
    #[serde(default)]
    pub properties: Vec<PcbProperty>,
}

// ---------------------------------------------------------------------------
// Traces / routing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub uuid: Uuid,
    pub start: Point,
    pub end: Point,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub net: u32,
    #[serde(default)]
    pub locked: bool,
}

/// Electrically meaningful curved copper track.
///
/// KiCad board format: <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_track_arc>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackArc {
    pub uuid: Uuid,
    pub start: Point,
    pub mid: Point,
    pub end: Point,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub net: u32,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Via {
    pub uuid: Uuid,
    pub position: Point,
    #[serde(default)]
    pub diameter: f64,
    #[serde(default)]
    pub drill: f64,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub net: u32,
    #[serde(default = "default_via_type")]
    pub via_type: ViaType,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub remove_unused_layers: bool,
    #[serde(default)]
    pub keep_end_layers: bool,
    #[serde(default)]
    pub free: bool,
}

fn default_via_type() -> ViaType {
    ViaType::Through
}

// ---------------------------------------------------------------------------
// Zone
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub uuid: Uuid,
    #[serde(default)]
    pub net: u32,
    #[serde(default)]
    pub net_name: String,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub hatch_style: String,
    #[serde(default)]
    pub hatch_pitch: f64,
    #[serde(default)]
    pub outline: Vec<Point>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub fill_type: String,
    #[serde(default)]
    pub thermal_relief: bool,
    #[serde(default)]
    pub thermal_gap: f64,
    #[serde(default)]
    pub thermal_width: f64,
    #[serde(default)]
    pub clearance: f64,
    #[serde(default)]
    pub min_thickness: f64,
    #[serde(default)]
    pub pad_connection: String,
    #[serde(default)]
    pub keepout: Option<ZoneKeepout>,
    #[serde(default)]
    pub fill: ZoneFill,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneKeepout {
    #[serde(default)]
    pub tracks: bool,
    #[serde(default)]
    pub vias: bool,
    #[serde(default)]
    pub pads: bool,
    #[serde(default)]
    pub copper_pour: bool,
    #[serde(default)]
    pub footprints: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneFill {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub smoothing: String,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub island_removal_mode: Option<u32>,
    #[serde(default)]
    pub island_area_min: Option<f64>,
    #[serde(default)]
    pub hatch_thickness: Option<f64>,
    #[serde(default)]
    pub hatch_gap: Option<f64>,
    #[serde(default)]
    pub hatch_orientation: Option<f64>,
}

/// Primitive retained from the `Edge.Cuts` layer. The legacy flattened
/// `PcbBoard::outline` remains available for simple consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardOutlinePrimitive {
    Line {
        uuid: Uuid,
        start: Point,
        end: Point,
    },
    Arc {
        uuid: Uuid,
        start: Point,
        mid: Point,
        end: Point,
    },
    Circle {
        uuid: Uuid,
        center: Point,
        end: Point,
    },
    Rect {
        uuid: Uuid,
        start: Point,
        end: Point,
    },
}

// ---------------------------------------------------------------------------
// Board-level graphics and text
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardGraphic {
    #[serde(default)]
    pub graphic_type: String,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub width: f64,
    pub start: Option<Point>,
    pub end: Option<Point>,
    pub center: Option<Point>,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardText {
    pub uuid: Uuid,
    #[serde(default)]
    pub text: String,
    pub position: Point,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub font_size: f64,
    #[serde(default)]
    pub rotation: f64,
}

// ---------------------------------------------------------------------------
// Top-level board
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcbBoard {
    pub uuid: Uuid,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub generator: String,
    #[serde(default)]
    pub thickness: f64,
    #[serde(default)]
    pub outline: Vec<Point>,
    /// Exact Edge.Cuts geometry, including disjoint contours and curves.
    ///
    /// KiCad board format: <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_graphic_items_section>
    #[serde(default)]
    pub outline_primitives: Vec<BoardOutlinePrimitive>,
    #[serde(default)]
    pub layers: Vec<LayerDef>,
    pub setup: Option<PcbSetup>,
    #[serde(default)]
    pub nets: Vec<NetDef>,
    #[serde(default)]
    pub footprints: Vec<Footprint>,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub arcs: Vec<TrackArc>,
    #[serde(default)]
    pub vias: Vec<Via>,
    #[serde(default)]
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub graphics: Vec<BoardGraphic>,
    #[serde(default)]
    pub texts: Vec<BoardText>,
}
