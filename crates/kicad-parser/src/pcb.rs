use std::path::Path;

use uuid::Uuid;

use signex_types::pcb::{
    BoardGraphic, BoardOutlinePrimitive, BoardStackup, BoardText, CustomPad, DrillDef, Footprint,
    FpGraphic, LayerDef, NetDef, PCB_COINCIDENCE_EPS, PCB_DEFAULT_CLEARANCE_MM,
    PCB_DEFAULT_GRAPHIC_WIDTH_MM, PCB_DEFAULT_PAD_SIZE_MM, PCB_DEFAULT_TEXT_SIZE_MM,
    PCB_DEFAULT_THICKNESS_MM, PCB_DEFAULT_TRACE_WIDTH_MM, PCB_DEFAULT_VIA_DIAMETER_MM,
    PCB_DEFAULT_VIA_DRILL_MM, PCB_GRID_MM, PCB_THERMAL_BRIDGE_MM, PCB_THERMAL_GAP_MM,
    PCB_TRACK_MIN_MM, PCB_VIA_MIN_DIAMETER_MM, PCB_VIA_MIN_DRILL_MM, PCB_ZONE_MIN_THICKNESS_MM,
    Pad, PadNet, PadPrimitive, PadShape, PadType, PcbBoard, PcbSetup, Point, Segment, StackupLayer,
    TrackArc, Via, ViaType, Zone, ZoneFill, ZoneKeepout,
};
use signex_types::property::PcbProperty;

use crate::error::ParseError;
use crate::sexpr::{self, SExpr};
use crate::{ImportDiagnostic, ParseReport};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn parse_point(node: &SExpr) -> Point {
    Point {
        x: node
            .arg(0)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0),
        y: node
            .arg(1)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0),
    }
}

fn parse_at(node: &SExpr) -> (Point, f64) {
    if let Some(at) = node.find("at") {
        let x = at.arg(0).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let y = at.arg(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let rot = at.arg(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        (Point { x, y }, rot)
    } else {
        (Point { x: 0.0, y: 0.0 }, 0.0)
    }
}

fn parse_uuid(node: &SExpr) -> Uuid {
    node.find("uuid")
        .or_else(|| node.find("tstamp"))
        .and_then(|u: &SExpr| u.first_arg())
        .and_then(|s: &str| Uuid::parse_str(s).ok())
        .unwrap_or_else(|| Uuid::new_v5(&Uuid::NAMESPACE_OID, node.to_string().as_bytes()))
}

fn parse_optional_point(node: &SExpr, keyword: &str) -> Option<Point> {
    node.find(keyword).map(parse_point)
}

/// Parse KiCad's physical board-construction model.
///
/// <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_stack_up_settings>
fn parse_stackup(setup: &SExpr) -> Option<BoardStackup> {
    let stackup = setup.find("stackup")?;
    let layers = stackup
        .find_all("layer")
        .iter()
        .map(|layer| StackupLayer {
            name: layer.first_arg().unwrap_or("").to_string(),
            number: layer.arg(1).and_then(|value| value.parse().ok()),
            layer_type: layer
                .find("type")
                .and_then(SExpr::first_arg)
                .unwrap_or("")
                .to_string(),
            color: layer
                .find("color")
                .and_then(SExpr::first_arg)
                .unwrap_or("")
                .to_string(),
            thickness: layer.find("thickness").and_then(|value| value.arg_f64(0)),
            material: layer
                .find("material")
                .and_then(SExpr::first_arg)
                .unwrap_or("")
                .to_string(),
            epsilon_r: layer.find("epsilon_r").and_then(|value| value.arg_f64(0)),
            loss_tangent: layer
                .find("loss_tangent")
                .and_then(|value| value.arg_f64(0)),
        })
        .collect();

    Some(BoardStackup {
        layers,
        copper_finish: stackup
            .find("copper_finish")
            .and_then(SExpr::first_arg)
            .unwrap_or("")
            .to_string(),
        dielectric_constraints: stackup
            .find("dielectric_constraints")
            .and_then(SExpr::first_arg)
            == Some("yes"),
        edge_connector: stackup
            .find("edge_connector")
            .and_then(SExpr::first_arg)
            .unwrap_or("")
            .to_string(),
        castellated_pads: stackup.find("castellated_pads").is_some(),
        edge_plating: stackup.find("edge_plating").is_some(),
    })
}

fn parse_custom_pad(pad: &SExpr) -> Option<CustomPad> {
    let options = pad.find("options");
    let primitives_node = pad.find("primitives")?;
    let primitives = primitives_node
        .children()
        .iter()
        .filter_map(|primitive| {
            let primitive_type = primitive.keyword()?.to_string();
            let points = primitive
                .find("pts")
                .map(|pts| {
                    pts.find_all("xy")
                        .iter()
                        .map(|xy| parse_point(xy))
                        .collect()
                })
                .unwrap_or_default();
            Some(PadPrimitive {
                primitive_type,
                start: parse_optional_point(primitive, "start"),
                mid: parse_optional_point(primitive, "mid"),
                end: parse_optional_point(primitive, "end"),
                center: parse_optional_point(primitive, "center"),
                points,
                width: primitive
                    .find("width")
                    .and_then(|value| value.arg_f64(0))
                    .unwrap_or(0.0),
                fill: primitive
                    .find("fill")
                    .and_then(SExpr::first_arg)
                    .map(|value| value == "yes")
                    .unwrap_or(false),
            })
        })
        .collect();

    Some(CustomPad {
        anchor_shape: options
            .and_then(|node| node.find("anchor"))
            .and_then(SExpr::first_arg)
            .unwrap_or("")
            .to_string(),
        clearance_shape: options
            .and_then(|node| node.find("clearance"))
            .and_then(SExpr::first_arg)
            .unwrap_or("")
            .to_string(),
        primitives,
    })
}

fn parse_pad_type(s: &str) -> PadType {
    match s {
        "thru_hole" => PadType::Thru,
        "smd" => PadType::Smd,
        "connect" => PadType::Connect,
        "np_thru_hole" => PadType::NpThru,
        _ => PadType::Smd,
    }
}

fn parse_pad_shape(s: &str) -> PadShape {
    match s {
        "circle" => PadShape::Circle,
        "rect" => PadShape::Rect,
        "oval" => PadShape::Oval,
        "trapezoid" => PadShape::Trapezoid,
        "roundrect" => PadShape::RoundRect,
        "custom" => PadShape::Custom,
        _ => PadShape::Rect,
    }
}

fn parse_via_type(s: &str) -> ViaType {
    match s {
        "blind" => ViaType::Blind,
        "micro" => ViaType::Micro,
        _ => ViaType::Through,
    }
}

fn parse_pcb_property(node: &SExpr) -> PcbProperty {
    let (position, rotation) = parse_at(node);
    let layer = node
        .find("layer")
        .and_then(|child| child.first_arg())
        .map(ToOwned::to_owned);
    let font_size = node
        .find("effects")
        .and_then(|effects| effects.find("font"))
        .and_then(|font| font.find("size"))
        .and_then(|size| size.first_arg())
        .and_then(|value| value.parse::<f64>().ok());
    let hidden = node.find("hide").is_some()
        || node
            .find("effects")
            .map(|effects| {
                effects.find("hide").is_some()
                    || effects
                        .children()
                        .iter()
                        .any(|child| matches!(child, SExpr::Atom(atom) if atom.as_str() == "hide"))
            })
            .unwrap_or(false);

    PcbProperty {
        key: node.first_arg().unwrap_or("").to_string(),
        value: node.arg(1).unwrap_or("").to_string(),
        position: node.find("at").map(|_| position),
        rotation,
        layer,
        font_size,
        hidden,
    }
}

// ---------------------------------------------------------------------------
// Main PCB parser
// ---------------------------------------------------------------------------

/// Parse a `.kicad_pcb` file from its string contents.
pub fn parse_pcb(content: &str) -> Result<PcbBoard, ParseError> {
    let root = sexpr::parse(content)?;

    if root.keyword() != Some("kicad_pcb") {
        return Err(ParseError::InvalidSExpr("Not a KiCad PCB file".to_string()));
    }

    let version = root
        .find("version")
        .and_then(|v| v.first_arg())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let generator = root
        .find("generator")
        .and_then(|g| g.first_arg())
        .unwrap_or("unknown")
        .to_string();

    let uuid = parse_uuid(&root);

    // Layers
    let layers: Vec<LayerDef> = if let Some(layers_node) = root.find("layers") {
        layers_node
            .children()
            .iter()
            .filter_map(|l| {
                let id_num = l.keyword()?.parse::<u8>().ok()?;
                let name = l.first_arg()?;
                let ltype = l.arg(1).unwrap_or("signal");
                Some(LayerDef {
                    id: id_num,
                    name: name.to_string(),
                    layer_type: ltype.to_string(),
                    user_name: l.arg(2).map(ToOwned::to_owned),
                })
            })
            .collect()
    } else {
        vec![]
    };

    // Setup
    let setup = if let Some(s) = root.find("setup") {
        Some(PcbSetup {
            grid_size: PCB_GRID_MM,
            trace_width: s
                .find("trace_min")
                .and_then(|t| t.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_TRACE_WIDTH_MM),
            via_diameter: s
                .find("via_size")
                .and_then(|v| v.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_VIA_DIAMETER_MM),
            via_drill: s
                .find("via_drill")
                .and_then(|v| v.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_VIA_DRILL_MM),
            clearance: s
                .find("clearance")
                .and_then(|c| c.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_CLEARANCE_MM),
            track_min_width: s
                .find("trace_min")
                .and_then(|t| t.first_arg()?.parse().ok())
                .unwrap_or(PCB_TRACK_MIN_MM),
            via_min_diameter: s
                .find("via_min_size")
                .and_then(|v| v.first_arg()?.parse().ok())
                .unwrap_or(PCB_VIA_MIN_DIAMETER_MM),
            via_min_drill: s
                .find("via_min_drill")
                .and_then(|v| v.first_arg()?.parse().ok())
                .unwrap_or(PCB_VIA_MIN_DRILL_MM),
            grid_origin: parse_optional_point(s, "grid_origin"),
            aux_axis_origin: parse_optional_point(s, "aux_axis_origin"),
            pad_to_mask_clearance: s
                .find("pad_to_mask_clearance")
                .and_then(|value| value.arg_f64(0))
                .unwrap_or(0.0),
            solder_mask_min_width: s
                .find("solder_mask_min_width")
                .and_then(|value| value.arg_f64(0)),
            pad_to_paste_clearance: s
                .find("pad_to_paste_clearance")
                .and_then(|value| value.arg_f64(0)),
            pad_to_paste_clearance_ratio: s
                .find("pad_to_paste_clearance_ratio")
                .and_then(|value| value.arg_f64(0)),
            stackup: parse_stackup(s),
        })
    } else {
        Some(PcbSetup {
            grid_size: PCB_GRID_MM,
            trace_width: PCB_DEFAULT_TRACE_WIDTH_MM,
            via_diameter: PCB_DEFAULT_VIA_DIAMETER_MM,
            via_drill: PCB_DEFAULT_VIA_DRILL_MM,
            clearance: PCB_DEFAULT_CLEARANCE_MM,
            track_min_width: PCB_TRACK_MIN_MM,
            via_min_diameter: PCB_VIA_MIN_DIAMETER_MM,
            via_min_drill: PCB_VIA_MIN_DRILL_MM,
            grid_origin: None,
            aux_axis_origin: None,
            pad_to_mask_clearance: 0.0,
            solder_mask_min_width: None,
            pad_to_paste_clearance: None,
            pad_to_paste_clearance_ratio: None,
            stackup: None,
        })
    };

    // Nets
    let nets: Vec<NetDef> = root
        .find_all("net")
        .iter()
        .filter_map(|n| {
            let num: u32 = n.first_arg()?.parse().ok()?;
            let name = n.arg(1).unwrap_or("").to_string();
            Some(NetDef { number: num, name })
        })
        .collect();

    // Preserve exact Edge.Cuts primitives. The flattened point list is kept
    // for compatibility with existing renderers.
    // <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_graphic_items_section>
    let mut outline_points = Vec::new();
    let mut outline_primitives = Vec::new();
    for gr in root.find_all("gr_line") {
        let layer = gr.find("layer").and_then(|l| l.first_arg()).unwrap_or("");
        if layer == "Edge.Cuts" {
            if let (Some(start), Some(end)) = (gr.find("start"), gr.find("end")) {
                let s = parse_point(start);
                let e = parse_point(end);
                if outline_points.is_empty()
                    || outline_points
                        .last()
                        .map(|p: &Point| {
                            (p.x - s.x).abs() > PCB_COINCIDENCE_EPS
                                || (p.y - s.y).abs() > PCB_COINCIDENCE_EPS
                        })
                        .unwrap_or(true)
                {
                    outline_points.push(s);
                }
                outline_points.push(e);
                outline_primitives.push(BoardOutlinePrimitive::Line {
                    uuid: parse_uuid(gr),
                    start: s,
                    end: e,
                });
            }
        }
    }

    // Footprints
    let footprints: Vec<Footprint> = root
        .find_all("footprint")
        .iter()
        .map(|fp| parse_footprint_node(fp))
        .collect();

    // Trace segments
    let segments: Vec<Segment> = root
        .find_all("segment")
        .iter()
        .map(|s| {
            let start = s
                .find("start")
                .map(|p| parse_point(p))
                .unwrap_or(Point { x: 0.0, y: 0.0 });
            let end = s
                .find("end")
                .map(|p| parse_point(p))
                .unwrap_or(Point { x: 0.0, y: 0.0 });
            let width = s
                .find("width")
                .and_then(|w| w.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_TRACE_WIDTH_MM);
            let layer = s
                .find("layer")
                .and_then(|l| l.first_arg())
                .unwrap_or("F.Cu")
                .to_string();
            let net: u32 = s
                .find("net")
                .and_then(|n| n.first_arg()?.parse().ok())
                .unwrap_or(0);
            Segment {
                uuid: parse_uuid(s),
                start,
                end,
                width,
                layer,
                net,
                locked: s.find("locked").is_some(),
            }
        })
        .collect();

    // Curved copper is electrically meaningful and must not be confused with
    // graphical `gr_arc` records.
    // <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_track_arc>
    let arcs: Vec<TrackArc> = root
        .find_all("arc")
        .iter()
        .map(|arc| TrackArc {
            uuid: parse_uuid(arc),
            start: arc.find("start").map(parse_point).unwrap_or(Point::ZERO),
            mid: arc.find("mid").map(parse_point).unwrap_or(Point::ZERO),
            end: arc.find("end").map(parse_point).unwrap_or(Point::ZERO),
            width: arc
                .find("width")
                .and_then(|value| value.arg_f64(0))
                .unwrap_or(PCB_DEFAULT_TRACE_WIDTH_MM),
            layer: arc
                .find("layer")
                .and_then(SExpr::first_arg)
                .unwrap_or("F.Cu")
                .to_string(),
            net: arc
                .find("net")
                .and_then(SExpr::first_arg)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            locked: arc.find("locked").is_some(),
        })
        .collect();

    // Vias
    let vias: Vec<Via> = root
        .find_all("via")
        .iter()
        .map(|v| {
            let (pos, _) = parse_at(v);
            let diameter = v
                .find("size")
                .and_then(|s| s.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_VIA_DIAMETER_MM);
            let drill = v
                .find("drill")
                .and_then(|d| d.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_VIA_DRILL_MM);
            let layers = if let Some(l) = v.find("layers") {
                vec![
                    l.arg(0).unwrap_or("F.Cu").to_string(),
                    l.arg(1).unwrap_or("B.Cu").to_string(),
                ]
            } else {
                vec!["F.Cu".to_string(), "B.Cu".to_string()]
            };
            let net: u32 = v
                .find("net")
                .and_then(|n| n.first_arg()?.parse().ok())
                .unwrap_or(0);
            // KiCad stores `blind` / `micro` as an optional leading atom.
            // <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_track_via>
            let via_type = v.first_arg().unwrap_or("through");
            Via {
                uuid: parse_uuid(v),
                position: pos,
                diameter,
                drill,
                layers,
                net,
                via_type: parse_via_type(via_type),
                locked: v.find("locked").is_some(),
                remove_unused_layers: v.find("remove_unused_layers").is_some(),
                keep_end_layers: v.find("keep_end_layers").is_some(),
                free: v.find("free").is_some(),
            }
        })
        .collect();

    // Zones
    let zones: Vec<Zone> = root
        .find_all("zone")
        .iter()
        .map(|z| {
            let net: u32 = z
                .find("net")
                .and_then(|n| n.first_arg()?.parse().ok())
                .unwrap_or(0);
            let net_name = z
                .find("net_name")
                .and_then(|n| n.first_arg())
                .unwrap_or("")
                .to_string();
            let layer = z
                .find("layer")
                .and_then(|l| l.first_arg())
                .unwrap_or("F.Cu")
                .to_string();
            let layers = z
                .find("layers")
                .map(|node| {
                    node.children()
                        .iter()
                        .filter_map(|child| match child {
                            SExpr::Atom(atom) => Some(atom.as_str().to_string()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![layer.clone()]);
            let priority: u32 = z
                .find("priority")
                .and_then(|p| p.first_arg()?.parse().ok())
                .unwrap_or(0);
            let connect = z.find("connect_pads");
            let clearance = connect
                .and_then(|node| node.find("clearance"))
                .and_then(|c| c.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_CLEARANCE_MM);
            let min_thickness = z
                .find("min_thickness")
                .and_then(|m| m.first_arg()?.parse().ok())
                .unwrap_or(PCB_ZONE_MIN_THICKNESS_MM);

            // Outline polygon
            let outline: Vec<Point> = if let Some(poly) = z.find("polygon") {
                if let Some(pts) = poly.find("pts") {
                    pts.find_all("xy")
                        .iter()
                        .map(|xy| parse_point(xy))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            // Thermal and hatch settings live in `(fill ...)`.
            // <https://dev-docs.kicad.org/en/file-formats/sexpr-intro/#_zone_fill_settings>
            let fill_node = z.find("fill");
            let thermal_gap = fill_node
                .and_then(|c| c.find("thermal_gap"))
                .and_then(|t| t.first_arg()?.parse().ok())
                .unwrap_or(PCB_THERMAL_GAP_MM);
            let thermal_width = fill_node
                .and_then(|c| c.find("thermal_bridge_width"))
                .and_then(|t| t.first_arg()?.parse().ok())
                .unwrap_or(PCB_THERMAL_BRIDGE_MM);
            let pad_connection = connect
                .and_then(SExpr::first_arg)
                .filter(|value| ["thru_hole_only", "full", "no"].contains(value))
                .unwrap_or("thermal")
                .to_string();
            let thermal_relief = pad_connection == "thermal";
            let keepout = z.find("keepout").map(|node| ZoneKeepout {
                tracks: node.find("tracks").and_then(SExpr::first_arg) == Some("not_allowed"),
                vias: node.find("vias").and_then(SExpr::first_arg) == Some("not_allowed"),
                pads: node.find("pads").and_then(SExpr::first_arg) == Some("not_allowed"),
                copper_pour: node.find("copperpour").and_then(SExpr::first_arg)
                    == Some("not_allowed"),
                footprints: node.find("footprints").and_then(SExpr::first_arg)
                    == Some("not_allowed"),
            });
            let fill = ZoneFill {
                enabled: fill_node.is_some(),
                mode: fill_node
                    .and_then(|node| node.find("mode"))
                    .and_then(SExpr::first_arg)
                    .unwrap_or("solid")
                    .to_string(),
                smoothing: fill_node
                    .and_then(|node| node.find("smoothing"))
                    .and_then(SExpr::first_arg)
                    .unwrap_or("")
                    .to_string(),
                radius: fill_node
                    .and_then(|node| node.find("radius"))
                    .and_then(|value| value.arg_f64(0))
                    .unwrap_or(0.0),
                island_removal_mode: fill_node
                    .and_then(|node| node.find("island_removal_mode"))
                    .and_then(SExpr::first_arg)
                    .and_then(|value| value.parse().ok()),
                island_area_min: fill_node
                    .and_then(|node| node.find("island_area_min"))
                    .and_then(|value| value.arg_f64(0)),
                hatch_thickness: fill_node
                    .and_then(|node| node.find("hatch_thickness"))
                    .and_then(|value| value.arg_f64(0)),
                hatch_gap: fill_node
                    .and_then(|node| node.find("hatch_gap"))
                    .and_then(|value| value.arg_f64(0)),
                hatch_orientation: fill_node
                    .and_then(|node| node.find("hatch_orientation"))
                    .and_then(|value| value.arg_f64(0)),
            };
            let hatch = z.find("hatch");

            Zone {
                uuid: parse_uuid(z),
                net,
                net_name,
                layer,
                layers,
                name: z
                    .find("name")
                    .and_then(SExpr::first_arg)
                    .unwrap_or("")
                    .to_string(),
                hatch_style: hatch.and_then(SExpr::first_arg).unwrap_or("").to_string(),
                hatch_pitch: hatch.and_then(|node| node.arg_f64(1)).unwrap_or(0.0),
                outline,
                priority,
                fill_type: fill.mode.clone(),
                thermal_relief,
                thermal_gap,
                thermal_width,
                clearance,
                min_thickness,
                pad_connection,
                keepout,
                fill,
            }
        })
        .collect();

    // Board-level graphics
    let mut board_graphics = Vec::new();
    for g in root.find_all("gr_line") {
        let layer = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        if layer == "Edge.Cuts" {
            continue; // Already handled as outline
        }
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .or_else(|| g.find("width").and_then(|w| w.first_arg()?.parse().ok()))
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let start = g.find("start").map(|s| parse_point(s));
        let end = g.find("end").map(|e| parse_point(e));
        board_graphics.push(BoardGraphic {
            graphic_type: "line".to_string(),
            layer,
            width: w,
            start,
            end,
            center: None,
            radius: 0.0,
            points: vec![],
        });
    }
    for g in root.find_all("gr_rect") {
        let layer = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .or_else(|| g.find("width").and_then(|w| w.first_arg()?.parse().ok()))
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let start = g.find("start").map(|s| parse_point(s));
        let end = g.find("end").map(|e| parse_point(e));
        // If on Edge.Cuts, also add to outline
        if layer == "Edge.Cuts" {
            if let (Some(s), Some(e)) = (&start, &end) {
                if outline_points.is_empty() {
                    outline_points.push(Point { x: s.x, y: s.y });
                    outline_points.push(Point { x: e.x, y: s.y });
                    outline_points.push(Point { x: e.x, y: e.y });
                    outline_points.push(Point { x: s.x, y: e.y });
                }
            }
            if let (Some(start), Some(end)) = (start, end) {
                outline_primitives.push(BoardOutlinePrimitive::Rect {
                    uuid: parse_uuid(g),
                    start,
                    end,
                });
            }
        }
        board_graphics.push(BoardGraphic {
            graphic_type: "rect".to_string(),
            layer,
            width: w,
            start,
            end,
            center: None,
            radius: 0.0,
            points: vec![],
        });
    }
    for g in root.find_all("gr_circle") {
        let layer = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .or_else(|| g.find("width").and_then(|w| w.first_arg()?.parse().ok()))
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let center = g.find("center").map(|c| parse_point(c));
        let end = g.find("end").map(|e| parse_point(e));
        let radius = if let (Some(c), Some(e)) = (&center, &end) {
            ((e.x - c.x).powi(2) + (e.y - c.y).powi(2)).sqrt()
        } else {
            0.0
        };
        if layer == "Edge.Cuts" {
            if let (Some(center), Some(end)) = (center, end) {
                outline_primitives.push(BoardOutlinePrimitive::Circle {
                    uuid: parse_uuid(g),
                    center,
                    end,
                });
            }
        }
        board_graphics.push(BoardGraphic {
            graphic_type: "circle".to_string(),
            layer,
            width: w,
            start: None,
            end: None,
            center,
            radius,
            points: vec![],
        });
    }
    for g in root.find_all("gr_arc") {
        let layer = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .or_else(|| g.find("width").and_then(|w| w.first_arg()?.parse().ok()))
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let start = g.find("start").map(|s| parse_point(s));
        let mid = g.find("mid").map(|m| parse_point(m));
        let end = g.find("end").map(|e| parse_point(e));
        if layer == "Edge.Cuts" {
            if let (Some(start), Some(mid), Some(end)) = (start, mid, end) {
                outline_primitives.push(BoardOutlinePrimitive::Arc {
                    uuid: parse_uuid(g),
                    start,
                    mid,
                    end,
                });
            }
        }
        // Store mid point in the points vec for the renderer
        let mut pts = vec![];
        if let Some(ref m) = mid {
            pts.push(*m);
        }
        board_graphics.push(BoardGraphic {
            graphic_type: "arc".to_string(),
            layer,
            width: w,
            start,
            end,
            center: None,
            radius: 0.0,
            points: pts,
        });
    }

    // Board-level texts
    let texts: Vec<BoardText> = root
        .find_all("gr_text")
        .iter()
        .map(|t| {
            let text = t.first_arg().unwrap_or("").to_string();
            let (pos, rot) = parse_at(t);
            let layer = t
                .find("layer")
                .and_then(|l| l.first_arg())
                .unwrap_or("F.SilkS")
                .to_string();
            let fs = t
                .find("effects")
                .and_then(|e| e.find("font"))
                .and_then(|f| f.find("size"))
                .and_then(|s| s.first_arg()?.parse().ok())
                .unwrap_or(PCB_DEFAULT_TEXT_SIZE_MM);
            BoardText {
                uuid: parse_uuid(t),
                text,
                position: pos,
                layer,
                font_size: fs,
                rotation: rot,
            }
        })
        .collect();

    let thickness = root
        .find("general")
        .and_then(|g| g.find("thickness"))
        .and_then(|t| t.first_arg()?.parse().ok())
        .unwrap_or(PCB_DEFAULT_THICKNESS_MM);

    Ok(PcbBoard {
        uuid,
        version,
        generator,
        thickness,
        outline: outline_points,
        outline_primitives,
        layers,
        setup,
        nets,
        footprints,
        segments,
        arcs,
        vias,
        zones,
        graphics: board_graphics,
        texts,
    })
}

// ---------------------------------------------------------------------------
// Footprint parsing
// ---------------------------------------------------------------------------

/// Parse a footprint from an S-expression node (reusable for both PCB and standalone .kicad_mod)
fn parse_footprint_node(fp: &SExpr) -> Footprint {
    let footprint_id = fp.first_arg().unwrap_or("").to_string();
    let (pos, rot) = parse_at(fp);
    let layer = fp
        .find("layer")
        .and_then(|l| l.first_arg())
        .unwrap_or("F.Cu")
        .to_string();
    let locked = fp.find("locked").is_some();
    let uuid = parse_uuid(fp);

    let reference = fp
        .find_all("property")
        .iter()
        .find(|p| p.first_arg() == Some("Reference"))
        .and_then(|p| p.arg(1))
        .unwrap_or("?")
        .to_string();

    let value = fp
        .find_all("property")
        .iter()
        .find(|p| p.first_arg() == Some("Value"))
        .and_then(|p| p.arg(1))
        .unwrap_or("")
        .to_string();

    let properties: Vec<PcbProperty> = fp
        .find_all("property")
        .iter()
        .map(|prop| parse_pcb_property(prop))
        .collect();

    // Pads
    let pads: Vec<Pad> = fp
        .find_all("pad")
        .iter()
        .map(|p| {
            let number = p.first_arg().unwrap_or("").to_string();
            let pad_type = parse_pad_type(p.arg(1).unwrap_or("smd"));
            let shape = parse_pad_shape(p.arg(2).unwrap_or("rect"));
            let (pad_pos, pad_rotation) = parse_at(p);
            let size = if let Some(sz) = p.find("size") {
                Point {
                    x: sz
                        .arg(0)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(PCB_DEFAULT_PAD_SIZE_MM),
                    y: sz
                        .arg(1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(PCB_DEFAULT_PAD_SIZE_MM),
                }
            } else {
                Point {
                    x: PCB_DEFAULT_PAD_SIZE_MM,
                    y: PCB_DEFAULT_PAD_SIZE_MM,
                }
            };
            // KiCad pad drill syntax supports both round holes and oval slots:
            // `(drill 0.8)` / `(drill oval 0.8 1.6 (offset 0.1 0))`.
            // <https://dev-docs.kicad.org/en/file-formats/sexpr-intro/#_footprint_pad>
            let drill = p.find("drill").map(|d| {
                let is_oval = d.first_arg() == Some("oval");
                let diameter_index = usize::from(is_oval);
                DrillDef {
                    diameter: d
                        .arg(diameter_index)
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(PCB_DEFAULT_VIA_DRILL_MM),
                    shape: if is_oval { "oval" } else { "round" }.to_string(),
                    width: if is_oval {
                        d.arg(diameter_index + 1)
                            .and_then(|value| value.parse().ok())
                    } else {
                        None
                    },
                    offset: d.find("offset").map(parse_point).unwrap_or(Point::ZERO),
                }
            });
            let pad_layers: Vec<String> = if let Some(layers) = p.find("layers") {
                layers
                    .children()
                    .iter()
                    .filter_map(|s| {
                        s.keyword().or_else(|| {
                            if let SExpr::Atom(a) = s {
                                Some(a.as_str())
                            } else {
                                None
                            }
                        })
                    })
                    .map(|s| s.to_string())
                    .collect()
            } else {
                vec![layer.clone()]
            };
            let net = p.find("net").map(|n| PadNet {
                number: n.first_arg().and_then(|s| s.parse().ok()).unwrap_or(0),
                name: n.arg(1).unwrap_or("").to_string(),
            });
            let roundrect_ratio = p
                .find("roundrect_rratio")
                .and_then(|r| r.first_arg()?.parse().ok())
                .unwrap_or(0.0);

            Pad {
                uuid: parse_uuid(p),
                number,
                pad_type,
                shape,
                position: pad_pos,
                rotation: pad_rotation,
                size,
                drill,
                layers: pad_layers,
                net,
                roundrect_ratio,
                chamfer_ratio: p
                    .find("chamfer_ratio")
                    .and_then(|value| value.arg_f64(0))
                    .unwrap_or(0.0),
                chamfer_corners: p
                    .find("chamfer")
                    .map(|node| {
                        node.children()
                            .iter()
                            .filter_map(|child| match child {
                                SExpr::Atom(atom) => Some(atom.as_str().to_string()),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                pin_function: p
                    .find("pinfunction")
                    .and_then(SExpr::first_arg)
                    .unwrap_or("")
                    .to_string(),
                pin_type: p
                    .find("pintype")
                    .and_then(SExpr::first_arg)
                    .unwrap_or("")
                    .to_string(),
                die_length: p.find("die_length").and_then(|value| value.arg_f64(0)),
                solder_mask_margin: p
                    .find("solder_mask_margin")
                    .and_then(|value| value.arg_f64(0)),
                solder_paste_margin: p
                    .find("solder_paste_margin")
                    .and_then(|value| value.arg_f64(0)),
                solder_paste_margin_ratio: p
                    .find("solder_paste_margin_ratio")
                    .and_then(|value| value.arg_f64(0)),
                clearance: p.find("clearance").and_then(|value| value.arg_f64(0)),
                zone_connect: p
                    .find("zone_connect")
                    .and_then(SExpr::first_arg)
                    .and_then(|value| value.parse().ok()),
                thermal_width: p.find("thermal_width").and_then(|value| value.arg_f64(0)),
                thermal_gap: p.find("thermal_gap").and_then(|value| value.arg_f64(0)),
                custom: (shape == PadShape::Custom)
                    .then(|| parse_custom_pad(p))
                    .flatten(),
            }
        })
        .collect();

    // Footprint graphics
    let mut graphics = Vec::new();
    for g in fp.find_all("fp_line") {
        let gl = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let start = g.find("start").map(|s| parse_point(s));
        let end = g.find("end").map(|e| parse_point(e));
        graphics.push(FpGraphic {
            graphic_type: "line".to_string(),
            layer: gl,
            width: w,
            start,
            end,
            center: None,
            mid: None,
            radius: 0.0,
            points: vec![],
            text: String::new(),
            font_size: 0.0,
            position: None,
            rotation: 0.0,
            fill: String::new(),
        });
    }
    for g in fp.find_all("fp_circle") {
        let gl = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let center = g.find("center").map(|c| parse_point(c));
        let end = g.find("end").map(|e| parse_point(e));
        let radius = if let (Some(c), Some(e)) = (&center, &end) {
            ((e.x - c.x).powi(2) + (e.y - c.y).powi(2)).sqrt()
        } else {
            0.0
        };
        graphics.push(FpGraphic {
            graphic_type: "circle".to_string(),
            layer: gl,
            width: w,
            start: None,
            end: None,
            center,
            mid: None,
            radius,
            points: vec![],
            text: String::new(),
            font_size: 0.0,
            position: None,
            rotation: 0.0,
            fill: String::new(),
        });
    }
    for g in fp.find_all("fp_text") {
        let text_type = g.first_arg().unwrap_or("user");
        let text_val = g.arg(1).unwrap_or("").to_string();
        let (text_pos, text_rot) = parse_at(g);
        let gl = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let fs = g
            .find("effects")
            .and_then(|e| e.find("font"))
            .and_then(|f| f.find("size"))
            .and_then(|s| s.first_arg()?.parse().ok())
            .unwrap_or(PCB_DEFAULT_TEXT_SIZE_MM);
        let display_text = match text_type {
            "reference" => "%R".to_string(),
            "value" => "%V".to_string(),
            _ => text_val,
        };
        graphics.push(FpGraphic {
            graphic_type: "text".to_string(),
            layer: gl,
            width: PCB_DEFAULT_GRAPHIC_WIDTH_MM,
            start: None,
            end: None,
            center: None,
            mid: None,
            radius: 0.0,
            points: vec![],
            text: display_text,
            font_size: fs,
            position: Some(text_pos),
            rotation: text_rot,
            fill: String::new(),
        });
    }
    for g in fp.find_all("fp_arc") {
        let gl = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let start = g.find("start").map(|s| parse_point(s));
        let mid = g.find("mid").map(|m| parse_point(m));
        let end = g.find("end").map(|e| parse_point(e));
        graphics.push(FpGraphic {
            graphic_type: "arc".to_string(),
            layer: gl,
            width: w,
            start,
            end,
            center: None,
            mid,
            radius: 0.0,
            points: vec![],
            text: String::new(),
            font_size: 0.0,
            position: None,
            rotation: 0.0,
            fill: String::new(),
        });
    }
    for g in fp.find_all("fp_poly") {
        let gl = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let pts: Vec<Point> = if let Some(pts_node) = g.find("pts") {
            pts_node
                .find_all("xy")
                .iter()
                .map(|xy| parse_point(xy))
                .collect()
        } else {
            vec![]
        };
        let fill = g
            .find("fill")
            .and_then(|f| f.first_arg())
            .map(|f| if f == "none" { "" } else { f })
            .unwrap_or("")
            .to_string();
        graphics.push(FpGraphic {
            graphic_type: "poly".to_string(),
            layer: gl,
            width: w,
            start: None,
            end: None,
            center: None,
            mid: None,
            radius: 0.0,
            points: pts,
            text: String::new(),
            font_size: 0.0,
            position: None,
            rotation: 0.0,
            fill,
        });
    }
    for prop in fp.find_all("property") {
        let prop_name = prop.first_arg().unwrap_or("");
        let prop_val = prop.arg(1).unwrap_or("").to_string();
        if let Some(_at) = prop.find("at") {
            if let Some(layer_node) = prop.find("layer") {
                let gl = layer_node.first_arg().unwrap_or("").to_string();
                if gl.is_empty() {
                    continue;
                }
                let (text_pos, text_rot) = parse_at(prop);
                let fs = prop
                    .find("effects")
                    .and_then(|e| e.find("font"))
                    .and_then(|f| f.find("size"))
                    .and_then(|s| s.first_arg()?.parse().ok())
                    .unwrap_or(PCB_DEFAULT_TEXT_SIZE_MM);
                let hidden = prop.find("effects").and_then(|e| e.find("hide")).is_some();
                if hidden {
                    continue;
                }
                let display_text = match prop_name {
                    "Reference" => "%R".to_string(),
                    "Value" => "%V".to_string(),
                    _ => prop_val,
                };
                graphics.push(FpGraphic {
                    graphic_type: "text".to_string(),
                    layer: gl,
                    width: PCB_DEFAULT_GRAPHIC_WIDTH_MM,
                    start: None,
                    end: None,
                    center: None,
                    mid: None,
                    radius: 0.0,
                    points: vec![],
                    text: display_text,
                    font_size: fs,
                    position: Some(text_pos),
                    rotation: text_rot,
                    fill: String::new(),
                });
            }
        }
    }
    for g in fp.find_all("fp_rect") {
        let gl = g
            .find("layer")
            .and_then(|l| l.first_arg())
            .unwrap_or("")
            .to_string();
        let w = g
            .find("stroke")
            .and_then(|s| s.find("width"))
            .and_then(|w| w.first_arg()?.parse().ok())
            .unwrap_or(PCB_DEFAULT_GRAPHIC_WIDTH_MM);
        let start = g.find("start").map(|s| parse_point(s));
        let end = g.find("end").map(|e| parse_point(e));
        let fill = g
            .find("fill")
            .and_then(|f| f.first_arg())
            .map(|f| if f == "none" { "" } else { f })
            .unwrap_or("")
            .to_string();
        graphics.push(FpGraphic {
            graphic_type: "rect".to_string(),
            layer: gl,
            width: w,
            start,
            end,
            center: None,
            mid: None,
            radius: 0.0,
            points: vec![],
            text: String::new(),
            font_size: 0.0,
            position: None,
            rotation: 0.0,
            fill,
        });
    }

    Footprint {
        uuid,
        reference,
        value,
        footprint_id,
        position: pos,
        rotation: rot,
        layer,
        locked,
        pads,
        graphics,
        properties,
    }
}

/// Parse a `.kicad_pcb` file from a file path.
pub fn parse_pcb_file(path: &Path) -> Result<PcbBoard, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_pcb(&content)
}

/// Parse a board and report known constructs that still cannot be represented.
pub fn parse_pcb_with_diagnostics(content: &str) -> Result<ParseReport<PcbBoard>, ParseError> {
    let board = parse_pcb(content)?;
    let root = sexpr::parse(content)?;
    let mut diagnostics = Vec::new();
    let board_doc = "https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/";
    let common_doc = "https://dev-docs.kicad.org/en/file-formats/sexpr-intro/";

    for (keyword, code, documentation) in [
        ("dimension", "KICAD-PCB-DIMENSION", common_doc),
        ("image", "KICAD-PCB-IMAGE", common_doc),
        ("group", "KICAD-PCB-GROUP", common_doc),
        ("gr_text_box", "KICAD-PCB-TEXT-BOX", common_doc),
        ("gr_curve", "KICAD-PCB-CURVE", common_doc),
    ] {
        let count = root.find_all(keyword).len();
        if count > 0 {
            diagnostics.push(ImportDiagnostic::unsupported(
                code,
                keyword,
                count,
                documentation,
            ));
        }
    }

    let footprints = root.find_all("footprint");
    for (keyword, code) in [
        ("model", "KICAD-PCB-3D-MODEL"),
        ("zone", "KICAD-PCB-FOOTPRINT-ZONE"),
        ("group", "KICAD-PCB-FOOTPRINT-GROUP"),
        ("image", "KICAD-PCB-FOOTPRINT-IMAGE"),
        ("dimension", "KICAD-PCB-FOOTPRINT-DIMENSION"),
        ("fp_text_box", "KICAD-PCB-FOOTPRINT-TEXT-BOX"),
        ("fp_curve", "KICAD-PCB-FOOTPRINT-CURVE"),
    ] {
        let count: usize = footprints
            .iter()
            .map(|footprint| footprint.find_all(keyword).len())
            .sum();
        if count > 0 {
            diagnostics.push(ImportDiagnostic::unsupported(
                code, keyword, count, board_doc,
            ));
        }
    }

    Ok(ParseReport::new(board, diagnostics))
}

pub fn parse_pcb_file_with_diagnostics(path: &Path) -> Result<ParseReport<PcbBoard>, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_pcb_with_diagnostics(&content)
}

/// Parse a standalone `.kicad_mod` footprint file.
pub fn parse_footprint_file(content: &str) -> Result<Footprint, ParseError> {
    let root = sexpr::parse(content)?;

    if root.keyword() != Some("footprint") {
        // Some older files might use "module"
        if root.keyword() != Some("module") {
            return Err(ParseError::InvalidSExpr(
                "Not a KiCad footprint file".to_string(),
            ));
        }
    }

    Ok(parse_footprint_node(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_footprint_property_metadata() {
        let footprint = parse_footprint_file(
            r#"(footprint "Resistor_SMD:R_0603"
    (layer "F.Cu")
    (at 10 20 90)
    (uuid "00000000-0000-0000-0000-000000000010")
    (property "Reference" "R1"
        (at 0 -2 0)
        (layer "F.SilkS")
        (effects (font (size 1 1) (thickness 0.15)))
    )
    (property "Value" "10k"
        (at 0 2 0)
        (layer "F.Fab")
        (effects (font (size 1 1) (thickness 0.15)))
    )
    (property "MPN" "RC0603FR-0710KL"
        (at 1 3 180)
        (layer "Cmts.User")
        (hide yes)
        (effects (font (size 1.2 1.2) (thickness 0.15)))
    )
)"#,
        )
        .unwrap();

        assert_eq!(footprint.reference, "R1");
        assert_eq!(footprint.value, "10k");
        assert_eq!(footprint.properties.len(), 3);
        let mpn = footprint
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .unwrap();
        assert_eq!(mpn.value, "RC0603FR-0710KL");
        assert_eq!(mpn.position, Some(Point { x: 1.0, y: 3.0 }));
        assert_eq!(mpn.rotation, 180.0);
        assert_eq!(mpn.layer.as_deref(), Some("Cmts.User"));
        assert_eq!(mpn.font_size, Some(1.2));
        assert!(mpn.hidden);
    }

    #[test]
    fn parse_setup_preserves_origins_stackup_and_modern_clearances() {
        // <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_setup_section>
        let board = parse_pcb(
            r#"(kicad_pcb
  (version 20240108)
  (generator test)
  (layers (0 "F.Cu" signal) (36 "B.SilkS" user "Bottom legend"))
  (setup
    (stackup
      (layer "F.Cu" (type "copper") (thickness 0.035))
      (layer dielectric 1 (type "core") (thickness 1.53) (material "FR4") (epsilon_r 4.5) (loss_tangent 0.02))
      (copper_finish "ENIG")
      (dielectric_constraints yes)
      (castellated_pads yes))
    (pad_to_mask_clearance 0.05)
    (solder_mask_min_width 0.1)
    (pad_to_paste_clearance -0.02)
    (pad_to_paste_clearance_ratio -0.1)
    (aux_axis_origin 2 3)
    (grid_origin 4 5)))"#,
        )
        .unwrap();

        assert_eq!(board.layers[1].user_name.as_deref(), Some("Bottom legend"));
        let setup = board.setup.unwrap();
        assert_eq!(setup.grid_size, PCB_GRID_MM);
        assert_eq!(setup.grid_origin, Some(Point::new(4.0, 5.0)));
        assert_eq!(setup.aux_axis_origin, Some(Point::new(2.0, 3.0)));
        assert_eq!(setup.pad_to_mask_clearance, 0.05);
        let stackup = setup.stackup.unwrap();
        assert_eq!(stackup.copper_finish, "ENIG");
        assert!(stackup.dielectric_constraints);
        assert!(stackup.castellated_pads);
        assert_eq!(stackup.layers[1].material, "FR4");
        assert_eq!(stackup.layers[1].epsilon_r, Some(4.5));
    }

    #[test]
    fn parse_tracks_and_edge_cuts_preserves_arcs_and_identity() {
        // <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/#_tracks_section>
        let board = parse_pcb(
            r#"(kicad_pcb
  (version 20240108)
  (generator test)
  (layers (0 "F.Cu" signal) (44 "Edge.Cuts" user))
  (segment (start 1 2) (end 3 4) (width 0.2) (layer "F.Cu") (locked) (net 1) (uuid "00000000-0000-0000-0000-000000000001"))
  (arc (start 3 4) (mid 4 5) (end 6 4) (width 0.25) (layer "F.Cu") (locked) (net 1) (uuid "00000000-0000-0000-0000-000000000002"))
  (gr_line (start 0 0) (end 10 0) (stroke (width 0.1) (type default)) (layer "Edge.Cuts") (uuid "00000000-0000-0000-0000-000000000003"))
  (gr_arc (start 10 0) (mid 11 1) (end 10 2) (stroke (width 0.1) (type default)) (layer "Edge.Cuts") (uuid "00000000-0000-0000-0000-000000000004")))"#,
        )
        .unwrap();

        assert!(board.segments[0].locked);
        assert_eq!(board.arcs.len(), 1);
        assert_eq!(board.arcs[0].mid, Point::new(4.0, 5.0));
        assert!(board.arcs[0].locked);
        assert_eq!(board.outline_primitives.len(), 2);
        assert!(matches!(
            board.outline_primitives[1],
            BoardOutlinePrimitive::Arc { .. }
        ));
    }

    #[test]
    fn parse_pad_and_via_preserves_rotation_slot_custom_geometry_and_flags() {
        // <https://dev-docs.kicad.org/en/file-formats/sexpr-intro/#_footprint_pad>
        let board = parse_pcb(
            r#"(kicad_pcb
  (version 20240108)
  (generator test)
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal))
  (footprint "Test:Custom" (layer "F.Cu") (at 10 20)
    (uuid "00000000-0000-0000-0000-000000000010")
    (pad "1" thru_hole custom (at 1 2 45) (size 2 3)
      (drill oval 0.8 1.6 (offset 0.1 -0.2))
      (layers "*.Cu" "*.Mask")
      (pinfunction "GPIO") (pintype "bidirectional") (die_length 0.4)
      (solder_mask_margin 0.05) (zone_connect 2)
      (options (clearance outline) (anchor rect))
      (primitives (gr_poly (pts (xy -1 -1) (xy 1 -1) (xy 1 1)) (width 0) (fill yes)))
      (uuid "00000000-0000-0000-0000-000000000011")))
  (via blind (locked) (at 5 6) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu")
    (remove_unused_layers) (keep_end_layers) (free) (net 1)
    (uuid "00000000-0000-0000-0000-000000000012")))"#,
        )
        .unwrap();

        let pad = &board.footprints[0].pads[0];
        assert_eq!(pad.rotation, 45.0);
        let drill = pad.drill.as_ref().unwrap();
        assert_eq!(drill.shape, "oval");
        assert_eq!(drill.diameter, 0.8);
        assert_eq!(drill.width, Some(1.6));
        assert_eq!(drill.offset, Point::new(0.1, -0.2));
        assert_eq!(pad.pin_function, "GPIO");
        assert_eq!(pad.zone_connect, Some(2));
        assert_eq!(pad.custom.as_ref().unwrap().primitives.len(), 1);

        let via = &board.vias[0];
        assert_eq!(via.via_type, ViaType::Blind);
        assert!(via.locked);
        assert!(via.remove_unused_layers);
        assert!(via.keep_end_layers);
        assert!(via.free);
    }

    #[test]
    fn parse_zone_reads_clearance_thermals_fill_and_keepout_from_correct_nodes() {
        // <https://dev-docs.kicad.org/en/file-formats/sexpr-intro/#_zone_settings>
        let board = parse_pcb(
            r#"(kicad_pcb
  (version 20240108)
  (generator test)
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal))
  (zone (net 0) (net_name "") (layers "F.Cu" "B.Cu")
    (uuid "00000000-0000-0000-0000-000000000020")
    (name "Mechanical keepout") (hatch edge 0.5)
    (connect_pads full (clearance 0.3))
    (min_thickness 0.2)
    (keepout (tracks not_allowed) (vias allowed) (pads not_allowed) (copperpour not_allowed) (footprints allowed))
    (fill yes (mode hatched) (thermal_gap 0.35) (thermal_bridge_width 0.45)
      (smoothing fillet) (radius 0.6) (island_removal_mode 2) (island_area_min 5)
      (hatch_thickness 0.2) (hatch_gap 0.3) (hatch_orientation 45))
    (polygon (pts (xy 0 0) (xy 10 0) (xy 10 10)))))"#,
        )
        .unwrap();

        let zone = &board.zones[0];
        assert_eq!(zone.layers, ["F.Cu", "B.Cu"]);
        assert_eq!(zone.clearance, 0.3);
        assert_eq!(zone.thermal_gap, 0.35);
        assert_eq!(zone.thermal_width, 0.45);
        assert_eq!(zone.pad_connection, "full");
        assert!(!zone.thermal_relief);
        assert!(zone.keepout.as_ref().unwrap().tracks);
        assert!(zone.keepout.as_ref().unwrap().pads);
        assert_eq!(zone.fill.mode, "hatched");
        assert_eq!(zone.fill.island_removal_mode, Some(2));
    }

    #[test]
    fn parse_report_warns_about_remaining_unsupported_board_constructs() {
        let report = parse_pcb_with_diagnostics(
            r#"(kicad_pcb
  (version 20240108)
  (generator test)
  (dimension (type aligned) (layer "F.Fab"))
  (group "Parts" (id "00000000-0000-0000-0000-000000000001") (members))
  (footprint "Test:Part" (layer "F.Cu")
    (model "part.step" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0)))))"#,
        )
        .unwrap();

        assert_eq!(report.diagnostics.len(), 3);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "KICAD-PCB-DIMENSION")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .all(
                    |diagnostic| diagnostic
                        .documentation
                        .is_some_and(|documentation| documentation
                            .starts_with("https://dev-docs.kicad.org/"))
                )
        );
    }
}
