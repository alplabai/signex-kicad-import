// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The board's physical stack: `(setup (stackup (layer "<name>" (type ..)
//! (thickness ..) (material ..) (epsilon_r ..)) ..))` into the rows of the
//! Signex `[[stackup.rows]]` list, top to bottom.

use snx_writer::model::PhysicalLayer;

use crate::geometry::number_of;
use crate::layers::Stack;
use crate::report::Report;
use crate::sexpr::SExpr;

/// Copper thickness of one ounce per square foot, mm: the weight a copper
/// row is written with is its thickness over this.
const OUNCE_COPPER_MM: f64 = 0.035;

/// The rows; empty when the board has no stackup, or (reported) when its
/// copper rows are not the board's copper layers in order or a number is
/// negative.
pub(crate) fn read(root: &SExpr, stack: &Stack, report: &mut Report) -> Vec<PhysicalLayer> {
    let Some(source) = root.find("setup").and_then(|s| s.find("stackup")) else {
        return Vec::new();
    };
    let mut seen: Vec<&str> = Vec::new();
    let mut rows = Vec::new();
    for layer in source.find_all("layer") {
        let name = layer.first_arg().unwrap_or("");
        if seen.contains(&name) {
            report.note("stackup layer listed again ignored");
            continue;
        }
        seen.push(name);
        if let Some(row) = row(layer, name, stack, report) {
            rows.push(row);
        }
    }
    let board_copper = stack.copper();
    let matches = rows
        .iter()
        .filter(|r| !r.layer.is_empty())
        .map(|r| r.layer.as_str())
        .eq(board_copper.iter().map(String::as_str));
    if !matches {
        report.note("stackup copper does not match the board's copper layers; stackup not written");
        return Vec::new();
    }
    let numbers_valid = rows.iter().all(|r| {
        [r.thickness_mm, r.dielectric_constant, r.copper_weight_oz]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
    });
    if !numbers_valid {
        report.note("stackup with a negative thickness or permittivity not written");
        return Vec::new();
    }
    rows
}

/// One source stackup layer as a row; `None` (reported) for a type with no
/// Signex row.
fn row(layer: &SExpr, name: &str, stack: &Stack, report: &mut Report) -> Option<PhysicalLayer> {
    let source_type = layer
        .find("type")
        .and_then(SExpr::first_arg)
        .unwrap_or("")
        .to_ascii_lowercase();
    let thicknesses: Vec<f64> = layer
        .find_all("thickness")
        .into_iter()
        .filter_map(|t| t.arg_f64(0))
        .collect();
    let thickness_mm: f64 = thicknesses.iter().sum();
    let material = layer
        .find("material")
        .and_then(SExpr::first_arg)
        .unwrap_or("")
        .to_string();
    let epsilon = number_of(layer, "epsilon_r").unwrap_or(0.0);
    let mut out = PhysicalLayer {
        thickness_mm,
        ..PhysicalLayer::default()
    };
    if source_type == "copper" {
        let Some(token) = stack.token(name) else {
            report.note(format!(
                "stackup copper layer {name} has no Signex layer; dropped"
            ));
            return None;
        };
        let plane = stack
            .layers
            .iter()
            .any(|l| l.token == token && l.layer_type == "plane");
        out.kind = if plane { "plane" } else { "signal" }.to_string();
        out.layer = token;
        out.material = "Copper".to_string();
        out.copper_weight_oz = (thickness_mm / OUNCE_COPPER_MM * 100.0).round() / 100.0;
    } else if matches!(source_type.as_str(), "core" | "prepreg") || name.starts_with("dielectric") {
        out.kind = "dielectric".to_string();
        out.material = material;
        out.dielectric_constant = epsilon;
        if thicknesses.len() > 1 {
            report.note("stackup dielectric sublayers written as one row");
        }
        if epsilon == 0.0 {
            report.note("stackup dielectric without a permittivity written without one");
        }
    } else if source_type.contains("solder mask") {
        out.kind = "solder_mask".to_string();
        out.material = if material.is_empty() {
            "Solder Resist".to_string()
        } else {
            material
        };
        out.dielectric_constant = epsilon;
    } else if source_type.contains("solder paste") {
        out.kind = "paste".to_string();
    } else if source_type.contains("silk screen") {
        out.kind = "overlay".to_string();
    } else {
        report.note(format!("stackup layer type {source_type} dropped"));
        return None;
    }
    if layer.find("color").is_some() {
        report.note("stackup layer colour dropped");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    fn read_text(text: &str) -> (Vec<PhysicalLayer>, Report) {
        let root = sexpr::parse(text).expect("parses");
        let mut report = Report::default();
        let stack = Stack::read(&root, &mut report);
        let rows = read(&root, &stack, &mut report);
        (rows, report)
    }

    const FOUR_LAYER: &str = r#"(kicad_pcb
      (layers (0 "F.Cu" signal) (1 "In1.Cu" power) (2 "In2.Cu" signal) (31 "B.Cu" signal))
      (setup (stackup
        (layer "F.SilkS" (type "Top Silk Screen"))
        (layer "F.Paste" (type "Top Solder Paste"))
        (layer "F.Mask" (type "Top Solder Mask") (color "Green") (thickness 0.01))
        (layer "F.Cu" (type "copper") (thickness 0.035))
        (layer "dielectric 1" (type "prepreg") (thickness 0.1) (material "FR4") (epsilon_r 4.4))
        (layer "In1.Cu" (type "copper") (thickness 0.0175))
        (layer "dielectric 2" (type "core") (thickness 1.2) (material "FR4") (epsilon_r 4.5))
        (layer "In2.Cu" (type "copper") (thickness 0.0175))
        (layer "dielectric 3" (type "prepreg") (thickness 0.1) (material "FR4") (epsilon_r 4.4))
        (layer "B.Cu" (type "copper") (thickness 0.07))
        (layer "B.Mask" (type "Bottom Solder Mask") (thickness 0.01))
        (layer "B.Paste" (type "Bottom Solder Paste"))
        (layer "B.SilkS" (type "Bottom Silk Screen"))
        (copper_finish "None"))))"#;

    #[test]
    fn the_physical_stack_becomes_rows_top_to_bottom() {
        let (rows, report) = read_text(FOUR_LAYER);
        let kinds: Vec<&str> = rows.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "overlay",
                "paste",
                "solder_mask",
                "signal",
                "dielectric",
                "plane",
                "dielectric",
                "signal",
                "dielectric",
                "signal",
                "solder_mask",
                "paste",
                "overlay"
            ]
        );
        let copper: Vec<&str> = rows
            .iter()
            .filter(|r| !r.layer.is_empty())
            .map(|r| r.layer.as_str())
            .collect();
        assert_eq!(
            copper,
            ["TopCopper", "Inner1Copper", "Inner2Copper", "BottomCopper"]
        );
        let core = &rows[6];
        assert_eq!(
            (
                core.material.as_str(),
                core.thickness_mm,
                core.dielectric_constant
            ),
            ("FR4", 1.2, 4.5)
        );
        assert_eq!(rows[3].copper_weight_oz, 1.0);
        assert_eq!(rows[5].copper_weight_oz, 0.5);
        assert_eq!(rows[9].copper_weight_oz, 2.0);
        assert_eq!(rows[2].material, "Solder Resist");
        assert!(report.summary().contains("stackup layer colour dropped"));
    }

    #[test]
    fn a_stack_whose_copper_does_not_match_is_reported_not_written() {
        let (rows, report) = read_text(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (31 "B.Cu" signal))
              (setup (stackup (layer "F.Cu" (type "copper") (thickness 0.035)))))"#,
        );
        assert!(rows.is_empty());
        assert!(report.summary().contains("stackup not written"));
    }

    #[test]
    fn a_layer_listed_twice_is_read_once() {
        let (rows, report) = read_text(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (31 "B.Cu" signal))
              (setup (stackup
                (layer "F.Cu" (type "copper") (thickness 0.035))
                (layer "dielectric 1" (type "core") (thickness 1.5) (epsilon_r 4.5))
                (layer "B.Cu" (type "copper") (thickness 0.035))
                (layer "F.Cu" (type "copper") (thickness 0.035)))))"#,
        );
        assert_eq!(rows.len(), 3);
        assert!(report.summary().contains("listed again"));
    }
}
