// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Source layer names to Signex layer tokens, and the board's stackup.

use std::collections::BTreeMap;

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

/// The source layer types that are copper.
const COPPER_KINDS: [&str; 4] = ["signal", "power", "mixed", "jumper"];

/// The board's layers: copper top to bottom, then the other layers in the
/// order the source lists them.
pub(crate) struct Stack {
    pub(crate) layers: Vec<StackLayer>,
    /// The board's own name of each copper layer (a user may rename them,
    /// and items then name the layer by that name) to its token.
    aliases: BTreeMap<String, String>,
}

impl Stack {
    /// From the root `(layers ..)` list: `(<id> "<name>" <type> ["<user name>"])`.
    ///
    /// A copper layer is known by its id, not its name, since a user may
    /// rename it: id 0 is the top; the bottom is id 31 in files that number
    /// the inner layers 1 to 30 and id 2 in files that number them 4, 6, ..;
    /// the inner layers follow in id order.
    pub(crate) fn read(root: &SExpr, report: &mut Report) -> Self {
        // (id, name, type, the user's own name when the board gives one)
        let entries: Vec<(u32, &str, &str, Option<&str>)> = root
            .find("layers")
            .map(SExpr::children)
            .unwrap_or(&[])
            .iter()
            .filter_map(|e| {
                let id = e.keyword()?.parse().ok()?;
                Some((id, e.arg(0)?, e.arg(1).unwrap_or("signal"), e.arg(2)))
            })
            .collect();
        let is_copper_kind = |kind: &str| COPPER_KINDS.contains(&kind);
        let bottom_id = if entries
            .iter()
            .any(|&(id, _, kind, _)| id == 31 && is_copper_kind(kind))
        {
            31
        } else {
            2
        };
        let mut inner_ids: Vec<u32> = entries
            .iter()
            .filter(|&&(id, _, kind, _)| is_copper_kind(kind) && id != 0 && id != bottom_id)
            .map(|&(id, _, _, _)| id)
            .collect();
        inner_ids.sort_unstable();

        let mut aliases = BTreeMap::new();
        let (mut top, mut bottom) = (None, None);
        let mut inner: Vec<(usize, StackLayer)> = Vec::new();
        let mut other: Vec<StackLayer> = Vec::new();
        for &(id, name, kind, user_name) in &entries {
            if is_copper_kind(kind) {
                let layer_type = if kind == "power" { "plane" } else { "signal" };
                let rank = inner_ids.iter().position(|&i| i == id);
                let token = match (id, rank) {
                    (0, _) => "TopCopper".to_string(),
                    (id, _) if id == bottom_id => "BottomCopper".to_string(),
                    (_, Some(r)) if r < 30 => format!("Inner{}Copper", r + 1),
                    _ => {
                        report.note(format!("copper layer {name} past 32 layers dropped"));
                        continue;
                    }
                };
                aliases.insert(name.to_string(), token.clone());
                // The board's own name for the layer: the user name it
                // carries, or the layer's name when the user renamed it.
                let display_name = match user_name.filter(|n| !n.is_empty()) {
                    Some(n) => n.to_string(),
                    None if crate::layers::token(name).as_deref() != Some(token.as_str()) => {
                        name.to_string()
                    }
                    None => String::new(),
                };
                let layer = StackLayer {
                    token,
                    layer_type: layer_type.to_string(),
                    display_name,
                };
                match (id, rank) {
                    (0, _) => top = Some(layer),
                    (_, Some(r)) => inner.push((r, layer)),
                    _ => bottom = Some(layer),
                }
                continue;
            }
            match token(name) {
                Some(t) if !other.iter().any(|l| l.token == t) => other.push(StackLayer {
                    token: t,
                    layer_type: String::new(),
                    display_name: String::new(),
                }),
                Some(_) => {}
                None => {
                    report.note(format!("layer {name} has no Signex layer; its items are dropped"));
                }
            }
        }
        inner.sort_by_key(|(r, _)| *r);
        let mut layers = Vec::new();
        layers.push(top.unwrap_or_else(|| copper("TopCopper")));
        layers.extend(inner.into_iter().map(|(_, l)| l));
        layers.push(bottom.unwrap_or_else(|| copper("BottomCopper")));
        layers.extend(other);
        Self { layers, aliases }
    }

    /// The token for a layer name an item uses: the board's own copper
    /// names first, then the fixed names.
    pub(crate) fn token(&self, name: &str) -> Option<String> {
        self.aliases.get(name).cloned().or_else(|| token(name))
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
                other => match self.token(other) {
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
        layer_type: "signal".to_string(),
        display_name: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    #[test]
    fn renamed_copper_layers_are_known_by_their_id() {
        let root = sexpr::parse(
            r#"(kicad_pcb (layers (0 "Top" signal) (1 "GND" power) (2 "3V3" power)
               (31 "Bottom" signal) (44 "Edge.Cuts" user)))"#,
        )
        .unwrap();
        let mut report = Report::default();
        let stack = Stack::read(&root, &mut report);
        assert!(report.is_empty(), "{}", report.summary());
        assert_eq!(stack.token("Top").as_deref(), Some("TopCopper"));
        assert_eq!(stack.token("GND").as_deref(), Some("Inner1Copper"));
        assert_eq!(stack.token("3V3").as_deref(), Some("Inner2Copper"));
        assert_eq!(stack.token("Bottom").as_deref(), Some("BottomCopper"));
        assert_eq!(stack.token("Edge.Cuts").as_deref(), Some("EdgeCuts"));
        assert_eq!(stack.copper().len(), 4);
        let pads = stack.pad_layers(&["Top", "F.Mask"], &mut report);
        assert_eq!(pads, ["TopCopper", "TopSolderMask"]);
        let names: Vec<(&str, &str, &str)> = stack
            .layers
            .iter()
            .map(|l| (l.token.as_str(), l.layer_type.as_str(), l.display_name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("TopCopper", "signal", "Top"),
                ("Inner1Copper", "plane", "GND"),
                ("Inner2Copper", "plane", "3V3"),
                ("BottomCopper", "signal", "Bottom"),
                ("EdgeCuts", "", ""),
            ]
        );
    }

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
