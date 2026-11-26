// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Part rows from the hand-written `parts.kicad_sym`.

use std::collections::HashSet;
use std::path::Path;

use snx_writer::part_table::{Param, content_hash};

use super::*;
use crate::symlib::read_symbol_file;

fn lib_id() -> Uuid {
    Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888)
}

fn build() -> Parts {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/symbols/parts.kicad_sym");
    let read = read_symbol_file(&path).expect("reads");
    let fps: HashSet<String> =
        [item_id(lib_id(), "footprint", "Fps", "R_Test_0603").to_string()].into();
    build_parts(&[("parts".to_string(), read)], lib_id(), &fps)
}

fn row<'a>(p: &'a Parts, pn: &str) -> &'a PartRow {
    p.tables[0]
        .rows
        .iter()
        .find(|r| r.internal_pn == pn)
        .unwrap_or_else(|| panic!("no row {pn}"))
}

fn param<'a>(r: &'a PartRow, key: &str) -> Option<&'a str> {
    r.parameters
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, p)| match p {
            Param::Text(t) => Some(t.as_str()),
            _ => None,
        })
}

#[test]
fn one_row_per_symbol_that_is_not_power_and_has_its_footprint() {
    let p = build();
    let mut pns: Vec<&str> = p.tables[0]
        .rows
        .iter()
        .map(|r| r.internal_pn.as_str())
        .collect();
    pns.sort_unstable();
    assert_eq!(
        pns,
        [
            "parts:C_NoFp",
            "parts:LED_Part",
            "parts:Odd_Part",
            "parts:R_Derived",
            "parts:R_Part"
        ]
    );
    assert_eq!(
        (p.candidates, p.rows),
        (6, 5),
        "power is no candidate, the missing footprint is skipped"
    );
    assert_eq!(p.tables.len(), 1);
    assert_eq!(p.tables[0].name, "parts");
    let missing = p
        .items
        .iter()
        .find(|i| i.code == "part.footprint_missing")
        .expect("item");
    assert_eq!(
        (missing.source.as_str(), missing.severity),
        ("parts:D_Missing", Severity::Error)
    );
    assert_eq!(missing.detail["footprint"], "Fps:Nope");
}

#[test]
fn classes_follow_the_designator_prefix_and_only_used_ones_are_declared() {
    let p = build();
    let class = |pn: &str| row(&p, pn).class.as_str();
    assert_eq!(class("parts:R_Part"), "resistor");
    assert_eq!(class("parts:C_NoFp"), "capacitor");
    assert_eq!(class("parts:LED_Part"), "led");
    assert_eq!(class("parts:Odd_Part"), "generic");
    let keys: Vec<&str> = p.classes.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["resistor", "capacitor", "led", "generic"]);
    assert_eq!(p.classes[2].label, "LED");
    for (prefix, want) in [
        ("U", "ic"),
        ("IC", "ic"),
        ("J", "connector"),
        ("CN", "connector"),
        ("SW", "switch"),
        ("Y", "crystal"),
        ("MH", "mechanical"),
        ("FID", "fiducial"),
        ("Q", "transistor"),
        ("L", "inductor"),
        ("D", "diode"),
        ("RN", "generic"),
    ] {
        assert_eq!(class_of(prefix), want, "{prefix}");
    }
}

#[test]
fn a_row_carries_the_properties_of_its_symbol() {
    let p = build();
    let r = row(&p, "parts:R_Part");
    assert_eq!(
        (r.state.as_str(), r.released, r.version.as_str()),
        ("Released", true, "1.0.0")
    );
    assert_eq!(
        r.footprint.as_ref().map(|f| f.uuid.clone()),
        Some(item_id(lib_id(), "footprint", "Fps", "R_Test_0603").to_string())
    );
    assert_eq!(r.footprint_version, "1.0.0");
    assert_eq!(r.description, "Resistor, test");
    assert!(
        matches!(&r.datasheets[0].document, Document::Url(u) if u == "https://example.com/r.pdf")
    );
    assert_eq!(
        (
            r.primary_mpn.manufacturer.as_str(),
            r.primary_mpn.mpn.as_str()
        ),
        ("Yageo", "RC0603FR-0710KL")
    );
    assert_eq!(param(r, "designator_prefix"), Some("R"));
    assert_eq!(param(r, "value"), Some("10k"));
    assert_eq!(param(r, "keywords"), Some("R res resistor"));
    assert_eq!(param(r, "Tolerance"), Some("1%"));
    assert_eq!(param(r, "value_property"), Some("clash"));
    // The footprint filters only stand in for a missing footprint.
    assert_eq!(param(r, "footprint_filters"), None);
    assert_eq!(
        r.symbol.uuid,
        item_id(lib_id(), "symbol", "parts", "R_Part").to_string()
    );
    assert_eq!(
        r.row_id,
        item_id(lib_id(), "part", "parts", "R_Part").to_string()
    );
}

#[test]
fn a_part_without_a_footprint_keeps_the_filters_and_no_datasheet_for_a_tilde() {
    let p = build();
    let r = row(&p, "parts:C_NoFp");
    assert!(r.footprint.is_none() && r.footprint_version.is_empty());
    assert_eq!(param(r, "footprint_filters"), Some("C_* Capacitor*"));
    assert!(r.datasheets.is_empty());
}

#[test]
fn a_derived_symbol_is_a_row_on_its_parents_symbol_with_its_own_properties() {
    let p = build();
    let d = row(&p, "parts:R_Derived");
    assert_eq!(
        d.symbol.uuid,
        item_id(lib_id(), "symbol", "parts", "R_Part").to_string()
    );
    assert_eq!(param(d, "value"), Some("22k"));
    assert!(d.datasheets.is_empty(), "its own `~` datasheet");
    assert_ne!(d.row_id, row(&p, "parts:R_Part").row_id);
}

#[test]
fn ids_hashes_and_the_run_are_deterministic() {
    let (a, b) = (build(), build());
    let hashes = |p: &Parts| {
        p.tables[0]
            .rows
            .iter()
            .map(content_hash)
            .collect::<Vec<_>>()
    };
    assert_eq!(hashes(&a), hashes(&b));
    assert!(
        hashes(&a)
            .iter()
            .all(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
    );
    assert_eq!(a.tables[0].rows, b.tables[0].rows);
}

#[test]
fn table_names_lose_the_characters_a_table_cannot_hold_and_stay_unique() {
    let mut used = HashSet::new();
    let mut items = Vec::new();
    assert_eq!(table_name("My.Lib", &mut used, &mut items), "My_Lib");
    assert_eq!(table_name("My_Lib", &mut used, &mut items), "My_Lib~2");
    assert_eq!(table_name("Plain", &mut used, &mut items), "Plain");
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.code == "part.table_name_changed"));
}
