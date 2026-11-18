// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Source layer names to Signex layer tokens, and the board's stackup.

use snx_writer::model::StackLayer;

use crate::report::Report;
use crate::sexpr::SExpr;

/// The Signex token for one source layer name; `None` for a name with no
/// Signex layer (the caller reports and drops what sits on it).
pub(crate) fn token(name: &str) -> Option<String> {
    let fixed = match name {
        "F.Cu" => "TopCopper",
        "B.Cu" => "BottomCopper",
        "F.SilkS" | "F.Silkscreen" => "TopSilkscreen",
        "B.SilkS" | "B.Silkscreen" => "BottomSilkscreen",
        "F.Mask" => "TopSolderMask",
        "B.Mask" => "BottomSolderMask",
        "F.Paste" => "TopPaste",
        "B.Paste" => "BottomPaste",
        "F.Adhes" | "F.Adhesive" => "TopAdhesive",
        "B.Adhes" | "B.Adhesive" => "BottomAdhesive",
        "F.Fab" => "TopFab",
        "B.Fab" => "BottomFab",
        "F.CrtYd" | "F.Courtyard" => "TopCourtyard",
        "B.CrtYd" | "B.Courtyard" => "BottomCourtyard",
        "Edge.Cuts" => "EdgeCuts",
        "Margin" => "Margin",
        "Dwgs.User" | "User.Drawings" => "Dimensions",
        "Cmts.User" | "User.Comments" => "Notes",
        "Eco1.User" | "User.Eco1" => "Eco1",
        "Eco2.User" | "User.Eco2" => "Eco2",
        _ => "",
    };
    if !fixed.is_empty() {
        return Some(fixed.to_string());
    }
    if let Some(n) = name
        .strip_prefix("In")
        .and_then(|rest| rest.strip_suffix(".Cu"))
        .and_then(|digits| digits.parse::<u8>().ok())
        .filter(|n| (1..=30).contains(n))
    {
        return Some(format!("Inner{n}Copper"));
    }
    name.strip_prefix("User.")
        .and_then(|digits| digits.parse::<u8>().ok())
        .filter(|n| (1..=32).contains(n))
        .map(|n| format!("Mechanical{n}"))
}

pub(crate) fn is_copper(token: &str) -> bool {
    token == "TopCopper"
        || token == "BottomCopper"
        || (token.starts_with("Inner") && token.ends_with("Copper"))
}

/// The board's layers: copper top to bottom, then the other layers in the
/// order the source lists them.
pub(crate) struct Stack {
    pub(crate) layers: Vec<StackLayer>,
}

impl Stack {
    /// From the root `(layers ..)` list: `(<id> "<name>" <type> ["<user name>"])`.
    pub(crate) fn read(root: &SExpr, report: &mut Report) -> Self {
        let mut inner: Vec<(u8, StackLayer)> = Vec::new();
        let mut outer: Vec<StackLayer> = Vec::new();
        let mut other: Vec<StackLayer> = Vec::new();
        let entries = root.find("layers").map(SExpr::children).unwrap_or(&[]);
        for entry in entries {
            let (Some(name), kind) = (entry.arg(0), entry.arg(1).unwrap_or("signal")) else {
                continue;
            };
            let Some(token) = token(name) else {
                report.note(format!("layer {name} has no Signex layer; its items are dropped"));
                continue;
            };
            let copper_type = if kind == "power" { "plane" } else { "copper" };
            let layer = |layer_type: &str| StackLayer {
                token: token.clone(),
                layer_type: layer_type.to_string(),
            };
            if token == "TopCopper" || token == "BottomCopper" {
                outer.push(layer(copper_type));
            } else if let Some(n) = inner_index(&token) {
                inner.push((n, layer(copper_type)));
            } else if !other.iter().any(|l| l.token == token) {
                other.push(layer(""));
            }
        }
        inner.sort_by_key(|(n, _)| *n);
        let mut layers = Vec::new();
        layers.extend(outer.iter().find(|l| l.token == "TopCopper").cloned());
        layers.extend(inner.into_iter().map(|(_, l)| l));
        layers.extend(outer.iter().find(|l| l.token == "BottomCopper").cloned());
        layers.extend(other);
        if !layers.iter().any(|l| l.token == "TopCopper") {
            layers.insert(0, copper("TopCopper"));
        }
        if !layers.iter().any(|l| l.token == "BottomCopper") {
            let at = layers.iter().filter(|l| is_copper(&l.token)).count();
            layers.insert(at, copper("BottomCopper"));
        }
        Self { layers }
    }

    /// Copper tokens, top to bottom.
    pub(crate) fn copper(&self) -> Vec<String> {
        self.layers
            .iter()
            .filter(|l| is_copper(&l.token))
            .map(|l| l.token.clone())
            .collect()
    }

    /// The tokens a pad's `(layers ..)` names, wildcards expanded against
    /// this stack; names with no Signex layer are reported and left out.
    pub(crate) fn pad_layers(&self, names: &[&str], report: &mut Report) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |token: &str| {
            if !out.iter().any(|t| t == token) {
                out.push(token.to_string());
            }
        };
        for &name in names {
            match name {
                "*.Cu" => {
                    for t in self.copper() {
                        push(&t);
                    }
                }
                "F&B.Cu" => {
                    push("TopCopper");
                    push("BottomCopper");
                }
                "*.Mask" | "F&B.Mask" => {
                    push("TopSolderMask");
                    push("BottomSolderMask");
                }
                "*.Paste" => {
                    push("TopPaste");
                    push("BottomPaste");
                }
                "*.SilkS" => {
                    push("TopSilkscreen");
                    push("BottomSilkscreen");
                }
                other => match token(other) {
                    Some(t) => push(&t),
                    None => report.note(format!("pad layer {other} dropped")),
                },
            }
        }
        out
    }
}

fn copper(token: &str) -> StackLayer {
    StackLayer {
        token: token.to_string(),
        layer_type: "copper".to_string(),
    }
}

fn inner_index(token: &str) -> Option<u8> {
    token
        .strip_prefix("Inner")?
        .strip_suffix("Copper")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    #[test]
    fn names_map_to_signex_tokens() {
        assert_eq!(token("F.Cu").as_deref(), Some("TopCopper"));
        assert_eq!(token("In4.Cu").as_deref(), Some("Inner4Copper"));
        assert_eq!(token("B.CrtYd").as_deref(), Some("BottomCourtyard"));
        assert_eq!(token("User.3").as_deref(), Some("Mechanical3"));
        assert_eq!(token("In31.Cu"), None);
        assert_eq!(token("Rescue"), None);
    }

    #[test]
    fn the_stack_orders_copper_top_inner_bottom() {
        let root = sexpr::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal) (6 "In2.Cu" signal)
               (4 "In1.Cu" power "GND") (25 "Edge.Cuts" user) (5 "F.SilkS" user)))"#,
        )
        .unwrap();
        let mut report = Report::default();
        let stack = Stack::read(&root, &mut report);
        let tokens: Vec<&str> = stack.layers.iter().map(|l| l.token.as_str()).collect();
        assert_eq!(
            tokens,
            [
                "TopCopper",
                "Inner1Copper",
                "Inner2Copper",
                "BottomCopper",
                "EdgeCuts",
                "TopSilkscreen"
            ]
        );
        assert_eq!(stack.layers[1].layer_type, "plane");
        assert_eq!(stack.layers[4].layer_type, "");
    }

    #[test]
    fn pad_wildcards_expand_against_the_stack() {
        let root = sexpr::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (4 "In1.Cu" signal) (2 "B.Cu" signal)))"#,
        )
        .unwrap();
        let mut report = Report::default();
        let stack = Stack::read(&root, &mut report);
        let layers = stack.pad_layers(&["*.Cu", "*.Mask"], &mut report);
        assert_eq!(
            layers,
            [
                "TopCopper",
                "Inner1Copper",
                "BottomCopper",
                "TopSolderMask",
                "BottomSolderMask"
            ]
        );
    }
}
