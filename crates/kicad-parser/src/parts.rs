// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Part rows of the `.snxlib` from the read symbol libraries: one row per
//! symbol that is not a power symbol, in one table per source library.
//!
//! A derived symbol (`extends`) is a row on its parent's symbol with its own
//! properties. The `Footprint` property `Lib:Name` binds the footprint of
//! that library name; a missing footprint skips the row (error item). No
//! footprint: a row without one and a `footprint_filters` parameter.

use std::collections::HashSet;

use serde_json::json;
use snx_writer::canon::clean_cell;
use snx_writer::part_table::{Class, Document, LabelledDocument, Mpn, Param, PartRow, Ref, Table};
use uuid::Uuid;

use crate::items::{Item, ItemKind, Severity};
use crate::libids::item_id;
use crate::symlib::{Symbol, SymbolLib, SymbolLibRead};

const EPOCH_ROW: &str = "1970-01-01T00:00:00+00:00";

/// The classes this converter assigns, in manifest order, from the leading
/// letters of the reference designator.
const CLASSES: [(&str, &str); 13] = [
    ("resistor", "Resistor"),
    ("capacitor", "Capacitor"),
    ("inductor", "Inductor"),
    ("diode", "Diode"),
    ("led", "LED"),
    ("transistor", "Transistor"),
    ("ic", "Integrated Circuit"),
    ("connector", "Connector"),
    ("switch", "Switch"),
    ("crystal", "Crystal"),
    ("mechanical", "Mechanical"),
    ("fiducial", "Fiducial"),
    ("generic", "Generic"),
];

/// The class key for a designator prefix (upper-case letters).
pub fn class_of(prefix: &str) -> &'static str {
    match prefix {
        "R" => "resistor",
        "C" => "capacitor",
        "L" => "inductor",
        "D" => "diode",
        "LED" => "led",
        "Q" => "transistor",
        "U" | "IC" => "ic",
        "J" | "P" | "CN" => "connector",
        "SW" | "S" => "switch",
        "Y" | "X" => "crystal",
        "H" | "MH" => "mechanical",
        "FID" => "fiducial",
        _ => "generic",
    }
}

#[derive(Debug, Default)]
pub struct Parts {
    pub tables: Vec<Table>,
    pub classes: Vec<Class>,
    pub items: Vec<Item>,
    /// Symbols that could have a row (not power symbols).
    pub candidates: usize,
    pub rows: usize,
    /// Every row built this run (written or kept byte for byte): row id and source library.
    pub generated: Vec<(String, String)>,
}

/// A row of the library an earlier run wrote, as an update needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct OldRow {
    /// The line of cells, as written.
    pub line: String,
    pub created: String,
    pub state: String,
    pub released: bool,
    pub version: String,
    pub content_hash: String,
    pub symbol_version: String,
    pub footprint_version: String,
}

/// What an update brings to the rows: the versions of the primitives as of
/// this run, the rows of the last run and the run's time.
#[derive(Debug, Clone, Default)]
pub struct Update {
    /// Symbol uuid to its version; absent means `1.0.0`.
    pub symbol_versions: std::collections::HashMap<String, String>,
    pub footprint_versions: std::collections::HashMap<String, String>,
    pub old_rows: std::collections::HashMap<String, OldRow>,
    /// The run's time as a row cell (`...+00:00`); empty outside an update.
    pub now: String,
}

/// The next patch version: `1.0.0` to `1.0.1`; text that is not `X.Y.Z` gets `.1` appended.
pub fn bump_patch(version: &str) -> String {
    let mut parts: Vec<&str> = version.split('.').collect();
    if let Some(n) = parts.last().and_then(|p| p.parse::<u64>().ok())
        && parts.len() == 3
    {
        let last = (n + 1).to_string();
        let len = parts.len();
        parts[len - 1] = &last;
        return parts.join(".");
    }
    format!("{version}.1")
}

/// `key` of `s`, or of the nearest ancestor along `extends` that has it.
fn lookup<'a>(lib: &'a SymbolLib, s: &'a Symbol, key: &str) -> Option<&'a str> {
    let mut cur = s;
    for _ in 0..=lib.symbols.len() {
        if let Some(v) = cur.property(key) {
            return Some(v);
        }
        cur = lib.symbol(cur.extends.as_deref()?)?;
    }
    None
}

/// The symbol that carries the drawing: `s` itself, or its nearest ancestor with units.
fn drawing_root<'a>(lib: &'a SymbolLib, s: &'a Symbol) -> Option<&'a Symbol> {
    let mut cur = s;
    for _ in 0..=lib.symbols.len() {
        if !cur.units.is_empty() || cur.extends.is_none() {
            return Some(cur);
        }
        cur = lib.symbol(cur.extends.as_deref()?)?;
    }
    None
}

fn is_power(lib: &SymbolLib, s: &Symbol) -> bool {
    let mut cur = s;
    for _ in 0..=lib.symbols.len() {
        if cur.power {
            return true;
        }
        match cur.extends.as_deref().and_then(|p| lib.symbol(p)) {
            Some(parent) => cur = parent,
            None => return false,
        }
    }
    false
}

fn table_name(library: &str, used: &mut HashSet<String>, items: &mut Vec<Item>) -> String {
    let mut name: String = library
        .chars()
        .map(|c| {
            if "/\\.:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if name.trim().is_empty() {
        name = "library".to_string();
    }
    let mut unique = name.clone();
    let mut n = 2;
    while !used.insert(unique.clone()) {
        unique = format!("{name}~{n}");
        n += 1;
    }
    if unique != library {
        items.push(Item::new(
            ItemKind::Library,
            library,
            "part.table_name_changed",
            Severity::Approximated,
            format!("part table name {library} written as {unique}: a table name cannot hold / \\ . : * ? \" < > |"),
        ));
    }
    unique
}

/// Reserved parameter names; a source property with one of these names keeps
/// its text under `<name>_property`.
const RESERVED: [&str; 4] = [
    "designator_prefix",
    "footprint_filters",
    "keywords",
    "value",
];

pub fn build_parts(
    libs: &[(String, SymbolLibRead)],
    library_id: Uuid,
    footprints: &HashSet<String>,
    update: &Update,
) -> Parts {
    let mut out = Parts::default();
    let mut used_classes: HashSet<&'static str> = HashSet::new();
    let mut used_tables: HashSet<String> = HashSet::new();
    for (library, read) in libs {
        let lib = &read.lib;
        let mut rows: Vec<PartRow> = Vec::new();
        let mut kept: Vec<(String, String)> = Vec::new();
        for s in &lib.symbols {
            if is_power(lib, s) {
                continue;
            }
            let Some(root) = drawing_root(lib, s) else {
                continue;
            };
            if lib.effective_units(s).is_none() {
                continue; // reported by the reader (broken extends)
            }
            out.candidates += 1;
            let source = format!("{library}:{}", s.name);
            let footprint = match lookup(lib, s, "Footprint")
                .map(str::trim)
                .filter(|f| !f.is_empty())
            {
                None => None,
                Some(fp) => {
                    let id = fp
                        .split_once(':')
                        .map(|(flib, fname)| {
                            item_id(library_id, "footprint", flib, fname).to_string()
                        })
                        .filter(|id| footprints.contains(id));
                    match id {
                        Some(uuid) => Some(Ref {
                            library_id: library_id.to_string(),
                            uuid,
                        }),
                        None => {
                            out.items.push(
                                Item::new(
                                    ItemKind::Part,
                                    &source,
                                    "part.footprint_missing",
                                    Severity::Error,
                                    format!("footprint {fp} not found in the converted footprint libraries; part skipped"),
                                )
                                .with_detail(json!({ "footprint": fp })),
                            );
                            continue;
                        }
                    }
                }
            };
            let row = row(
                lib,
                s,
                root,
                library,
                library_id,
                footprint,
                &mut used_classes,
                &mut out.items,
            );
            let mut row = row;
            if row.symbol.uuid != s_uuid(library_id, library, &s.name) {
                // A derived symbol shares its parent's drawing, and with it the
                // value placement reads from the symbol.
                if lookup(lib, s, "Value") != lookup(lib, root, "Value") {
                    out.items.push(Item::new(
                        ItemKind::Part,
                        &source,
                        "part.value_from_parent",
                        Severity::Approximated,
                        "the part shares its parent's symbol, whose value text it shows instead of its own",
                    ));
                }
            }
            if let Some(v) = update.symbol_versions.get(&row.symbol.uuid) {
                row.symbol_version.clone_from(v);
            }
            if let Some(f) = row
                .footprint
                .as_ref()
                .and_then(|f| update.footprint_versions.get(&f.uuid))
            {
                row.footprint_version.clone_from(f);
            }
            match update.old_rows.get(&row.row_id) {
                Some(old) => {
                    row.state.clone_from(&old.state);
                    row.released = old.released;
                    let same = snx_writer::part_table::content_hash(&row) == old.content_hash
                        && row.symbol_version == old.symbol_version
                        && row.footprint_version == old.footprint_version;
                    out.generated.push((row.row_id.clone(), library.clone()));
                    if same {
                        kept.push((row.row_id.clone(), old.line.clone()));
                        continue;
                    }
                    row.version = bump_patch(&old.version);
                    row.created.clone_from(&old.created);
                    row.updated.clone_from(&update.now);
                }
                None if !update.now.is_empty() => {
                    row.created.clone_from(&update.now);
                    row.updated.clone_from(&update.now);
                }
                None => {}
            }
            if !update.old_rows.contains_key(&row.row_id) {
                out.generated.push((row.row_id.clone(), library.clone()));
            }
            rows.push(row);
        }
        if !rows.is_empty() || !kept.is_empty() {
            out.rows += rows.len() + kept.len();
            let name = table_name(library, &mut used_tables, &mut out.items);
            out.tables.push(Table {
                name,
                rows,
                kept,
                raw_tsv: None,
            });
        }
    }
    out.classes = CLASSES
        .iter()
        .filter(|(key, _)| used_classes.contains(key))
        .map(|(key, label)| Class {
            key: (*key).to_string(),
            label: (*label).to_string(),
        })
        .collect();
    out
}

fn s_uuid(library_id: Uuid, library: &str, symbol: &str) -> String {
    item_id(library_id, "symbol", library, symbol).to_string()
}

fn push_param(parameters: &mut Vec<(String, Param)>, key: &str, value: &str) {
    parameters.push((key.to_string(), Param::Text(value.to_string())));
}

#[allow(clippy::too_many_arguments)]
fn row(
    lib: &SymbolLib,
    s: &Symbol,
    root: &Symbol,
    library: &str,
    library_id: Uuid,
    footprint: Option<Ref>,
    used_classes: &mut HashSet<&'static str>,
    items: &mut Vec<Item>,
) -> PartRow {
    let source = format!("{library}:{}", s.name);
    let mut cleaned = |text: &str, what: &str| -> String {
        let (t, changed) = clean_cell(text);
        if changed {
            items.push(Item::new(
                ItemKind::Part,
                &source,
                "part.text_sanitized",
                Severity::Approximated,
                format!("control characters in the {what} replaced by spaces"),
            ));
        }
        t
    };
    let prefix: String = lookup(lib, s, "Reference")
        .unwrap_or("")
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_uppercase();
    let class = class_of(&prefix);
    used_classes.insert(class);
    let text = |key: &str| lookup(lib, s, key).map(str::trim).filter(|v| !v.is_empty());
    let datasheets = match text("Datasheet").filter(|d| *d != "~") {
        Some(url) => vec![LabelledDocument {
            label: String::new(),
            document: Document::Url(cleaned(url, "datasheet")),
        }],
        None => vec![],
    };
    let manufacturer = text("Manufacturer")
        .or_else(|| text("MFR"))
        .or_else(|| text("Manufacturer_Name"));
    let mpn = text("MPN")
        .or_else(|| text("Manufacturer_Part_Number"))
        .or_else(|| text("MFG_PN"));
    let description = lookup(lib, s, "Description")
        .or_else(|| lookup(lib, s, "ki_description"))
        .unwrap_or("");
    let mut parameters: Vec<(String, Param)> = Vec::new();
    if !prefix.is_empty() {
        push_param(&mut parameters, "designator_prefix", &prefix);
    }
    if let Some(v) = text("Value") {
        push_param(&mut parameters, "value", v);
    }
    if let Some(k) = text("ki_keywords") {
        push_param(&mut parameters, "keywords", k);
    }
    if footprint.is_none()
        && let Some(f) = text("ki_fp_filters")
    {
        push_param(&mut parameters, "footprint_filters", f);
    }
    // Everything else the symbol says becomes a text parameter.
    let skip = [
        "Reference",
        "Value",
        "Footprint",
        "Datasheet",
        "Description",
        "ki_description",
        "ki_keywords",
        "ki_fp_filters",
        "ki_locked",
        "Manufacturer",
        "MFR",
        "Manufacturer_Name",
        "MPN",
        "Manufacturer_Part_Number",
        "MFG_PN",
    ];
    for p in &s.properties {
        let v = p.value.trim();
        if skip.contains(&p.key.as_str()) || v.is_empty() || p.key.is_empty() {
            continue;
        }
        let key = if RESERVED.contains(&p.key.as_str()) {
            format!("{}_property", p.key)
        } else {
            p.key.clone()
        };
        if !parameters.iter().any(|(k, _)| *k == key) {
            push_param(&mut parameters, &key, v);
        }
    }
    let has_fp = footprint.is_some();
    let row_id = item_id(library_id, "part", library, &s.name).to_string();
    let internal_pn = cleaned(&format!("{library}:{}", s.name), "part number");
    let description = cleaned(description, "description");
    let mut row = PartRow {
        row_id: row_id.clone(),
        internal_pn,
        class: class.to_string(),
        datasheets,
        state: "Released".to_string(),
        symbol: Ref {
            library_id: library_id.to_string(),
            uuid: item_id(library_id, "symbol", library, &root.name).to_string(),
        },
        footprint,
        alternate_footprints: vec![],
        sim: None,
        primary_mpn: Mpn {
            manufacturer: manufacturer.unwrap_or("").to_string(),
            mpn: mpn.unwrap_or("").to_string(),
            status: "Primary".to_string(),
            notes: None,
        },
        alternates: vec![],
        supply: vec![],
        parameters: vec![],
        pin_map_overrides: vec![],
        created: EPOCH_ROW.to_string(),
        updated: EPOCH_ROW.to_string(),
        version: "1.0.0".to_string(),
        released: true,
        symbol_version: "1.0.0".to_string(),
        footprint_version: if has_fp { "1.0.0" } else { "" }.to_string(),
        sim_version: String::new(),
        description,
        alternate_symbols: vec![],
    };
    // Parameter names and texts and the part's maker go into table cells too.
    let mut changed = false;
    let mut scrub = |t: &str| {
        let (out, c) = clean_cell(t);
        changed |= c;
        out
    };
    for (k, v) in parameters {
        let v = match v {
            Param::Text(t) => Param::Text(scrub(&t)),
            other => other,
        };
        row.parameters.push((scrub(&k), v));
    }
    row.primary_mpn.manufacturer = scrub(&row.primary_mpn.manufacturer);
    row.primary_mpn.mpn = scrub(&row.primary_mpn.mpn);
    if changed {
        items.push(Item::new(
            ItemKind::Part,
            &source,
            "part.text_sanitized",
            Severity::Approximated,
            "control characters or triple quotes in a property replaced by spaces",
        ));
    }
    items.push(
        Item::new(
            ItemKind::Part,
            &source,
            "part.written",
            Severity::Info,
            "part row written",
        )
        .with_id(&row_id),
    );
    row
}

#[cfg(test)]
mod tests;
