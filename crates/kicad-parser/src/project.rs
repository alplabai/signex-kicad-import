// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The net classes and board rules of a `.kicad_pro` (JSON).

use serde_json::Value;
use snx_writer::model::{NetClass, Setup};

use crate::error::ParseError;

/// What the project file says about the board.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectRules {
    /// The classes in project order, the default class last with the
    /// pattern `*` so it holds every net no other class takes.
    pub classes: Vec<NetClass>,
    pub setup: Setup,
}

const DEFAULT_CLASS: &str = "Default";

/// Read the classes and rules from `.kicad_pro` text.
pub fn parse_project_rules(text: &str) -> Result<ProjectRules, ParseError> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| ParseError::InvalidValue(format!("project file: {e}")))?;
    let settings = &root["net_settings"];
    let mut classes: Vec<NetClass> = settings["classes"]
        .as_array()
        .map(|list| list.iter().filter_map(class_of).collect())
        .unwrap_or_default();
    for entry in settings["netclass_patterns"].as_array().into_iter().flatten() {
        let (Some(name), Some(pattern)) =
            (entry["netclass"].as_str(), entry["pattern"].as_str())
        else {
            continue;
        };
        if let Some(class) = classes.iter_mut().find(|c| c.name == name) {
            class.patterns.push(pattern.to_string());
        }
    }
    if let Some(assignments) = settings["netclass_assignments"].as_object() {
        for (net, value) in assignments {
            let name = value
                .as_str()
                .or_else(|| value.as_array()?.first()?.as_str());
            if let Some(class) = name.and_then(|n| classes.iter_mut().find(|c| c.name == n)) {
                class.nets.push(net.clone());
            }
        }
    }
    let setup = setup_of(&root, classes.iter().find(|c| c.name == DEFAULT_CLASS));
    // The default class matches every net, so it goes last: Signex takes
    // the first class that matches.
    if let Some(i) = classes.iter().position(|c| c.name == DEFAULT_CLASS) {
        let mut default = classes.remove(i);
        default.nets.clear();
        default.patterns = vec!["*".to_string()];
        classes.push(default);
    }
    classes.retain(|c| c.name == DEFAULT_CLASS || !c.nets.is_empty() || !c.patterns.is_empty());
    Ok(ProjectRules { classes, setup })
}

fn mm(value: &Value) -> f64 {
    value.as_f64().filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.0)
}

fn class_of(value: &Value) -> Option<NetClass> {
    let name = value["name"].as_str()?.to_string();
    let nets = value["nets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(NetClass {
        name,
        nets,
        patterns: Vec::new(),
        clearance: mm(&value["clearance"]),
        trace_width: mm(&value["track_width"]),
        via_diameter: mm(&value["via_diameter"]),
        via_drill: mm(&value["via_drill"]),
        diff_pair_gap: mm(&value["diff_pair_gap"]),
        diff_pair_width: mm(&value["diff_pair_width"]),
    })
}

/// The board defaults: the default class's sizes and the minimums of the
/// project's board rules, each falling back to the Signex default.
fn setup_of(root: &Value, default: Option<&NetClass>) -> Setup {
    let base = Setup::default();
    let or = |v: f64, fallback: f64| if v > 0.0 { v } else { fallback };
    let rules = &root["board"]["design_settings"]["rules"];
    let class = default.cloned().unwrap_or_default();
    Setup {
        grid_size: base.grid_size,
        trace_width: or(class.trace_width, base.trace_width),
        via_diameter: or(class.via_diameter, base.via_diameter),
        via_drill: or(class.via_drill, base.via_drill),
        clearance: or(class.clearance, base.clearance),
        track_min_width: or(mm(&rules["min_track_width"]), base.track_min_width),
        via_min_diameter: or(mm(&rules["min_via_diameter"]), base.via_min_diameter),
        via_min_drill: or(mm(&rules["min_through_hole_diameter"]), base.via_min_drill),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = r#"{
      "board": { "design_settings": { "rules": { "min_track_width": 0.127 } } },
      "net_settings": {
        "classes": [
          { "name": "Default", "clearance": 0.15, "track_width": 0.2,
            "via_diameter": 0.5, "via_drill": 0.25 },
          { "name": "Power", "clearance": 0.2, "track_width": 0.5 },
          { "name": "Unused", "clearance": 0.3 }
        ],
        "netclass_patterns": [ { "netclass": "Power", "pattern": "+*V*" } ],
        "netclass_assignments": { "GND": "Power" }
      }
    }"#;

    #[test]
    fn classes_keep_their_members_and_the_default_goes_last() {
        let rules = parse_project_rules(PROJECT).expect("parses");
        let names: Vec<&str> = rules.classes.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Power", "Default"]);
        assert_eq!(rules.classes[0].patterns, ["+*V*"]);
        assert_eq!(rules.classes[0].nets, ["GND"]);
        assert_eq!(rules.classes[0].trace_width, 0.5);
        assert_eq!(rules.classes[1].patterns, ["*"]);
    }

    #[test]
    fn the_setup_takes_the_default_class_and_the_rules() {
        let rules = parse_project_rules(PROJECT).expect("parses");
        assert_eq!(rules.setup.trace_width, 0.2);
        assert_eq!(rules.setup.clearance, 0.15);
        assert_eq!(rules.setup.track_min_width, 0.127);
        assert_eq!(rules.setup.via_min_drill, Setup::default().via_min_drill);
    }

    #[test]
    fn text_that_is_not_json_is_an_error() {
        assert!(parse_project_rules("not json").is_err());
    }
}
