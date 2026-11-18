// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `.snxprj` project file: pretty-printed JSON naming the board and
//! carrying its net classes. Keys are written in sorted order.

use serde_json::{Map, Value, json};

use crate::model::NetClass;

/// The `.snxprj` text for a project `name` whose board is `pcb_file`
/// (a file name beside the project).
pub fn write_project(name: &str, pcb_file: &str, classes: &[NetClass]) -> String {
    let mut project = json!({
        "format": "snxprj/1",
        "name": name,
        "dir": "",
        "pcb_file": pcb_file,
        "schematic_root": null,
        "sheets": [],
        "libraries": [],
        "variant_definitions": [],
        "active_variant": null,
        "enable_git": false,
    });
    if !classes.is_empty()
        && let Some(map) = project.as_object_mut()
    {
        map.insert(
            "net_classes".into(),
            Value::Array(classes.iter().map(class_json).collect()),
        );
    }
    let mut text = serde_json::to_string_pretty(&project).unwrap_or_default();
    text.push('\n');
    text
}

fn class_json(class: &NetClass) -> Value {
    let mut map = Map::new();
    map.insert("name".into(), Value::from(class.name.clone()));
    if !class.nets.is_empty() {
        map.insert("nets".into(), Value::from(class.nets.clone()));
    }
    if !class.patterns.is_empty() {
        map.insert("patterns".into(), Value::from(class.patterns.clone()));
    }
    for (key, value) in [
        ("clearance", class.clearance),
        ("trace_width", class.trace_width),
        ("via_diameter", class.via_diameter),
        ("via_drill", class.via_drill),
        ("diff_pair_gap", class.diff_pair_gap),
        ("diff_pair_width", class.diff_pair_width),
    ] {
        if value.is_finite() && value > 0.0 {
            map.insert(key.into(), Value::from(value));
        }
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_names_its_board_and_lists_its_classes() {
        let classes = [NetClass {
            name: "Power".into(),
            nets: vec!["GND".into()],
            patterns: vec!["VCC*".into()],
            clearance: 0.2,
            trace_width: 0.4,
            ..NetClass::default()
        }];
        let text = write_project("board", "board.snxpcb", &classes);
        let value: Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(value["format"], "snxprj/1");
        assert_eq!(value["dir"], "");
        assert_eq!(value["pcb_file"], "board.snxpcb");
        let class = &value["net_classes"][0];
        assert_eq!(class["name"], "Power");
        assert_eq!(class["nets"][0], "GND");
        assert_eq!(class["trace_width"], 0.4);
        assert!(class.get("via_drill").is_none());
    }

    #[test]
    fn a_project_without_classes_writes_no_class_key() {
        let text = write_project("b", "b.snxpcb", &[]);
        let value: Value = serde_json::from_str(&text).expect("JSON");
        assert!(value.get("net_classes").is_none());
    }
}
