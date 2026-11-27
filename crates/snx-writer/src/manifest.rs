// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `.snxlib` manifest (`snxlib/2`, `LIBRARY_FORMAT.md` section 2.1) of a
//! library with no part tables yet: every key the format writes, default
//! workflow and users, then the classes and part tables given.

use std::fmt::Write as _;

use crate::canon::toml_string;
use crate::part_table::{Class, Table, write_classes, write_table};

/// The optional `[library]` keys. `license` (an SPDX identifier), `source`,
/// `converted_by` and `requires` (library ids this library's parts refer to)
/// are keys the format spec of this converter's time does not list yet; they
/// are written after `description`, each only when given.
#[derive(Debug, Clone, Default)]
pub struct LibraryMeta {
    pub description: Option<String>,
    pub license: Option<String>,
    pub source: Option<String>,
    pub converted_by: Option<String>,
    pub requires: Vec<String>,
}

/// The converter-owned `[library]` lines after `description`: `license`,
/// `source`, `converted_by` and `requires`, as `write_manifest` writes them.
pub fn library_block_lines(meta: &LibraryMeta) -> Vec<String> {
    let mut o = String::new();
    for (key, value) in [
        ("license", &meta.license),
        ("source", &meta.source),
        ("converted_by", &meta.converted_by),
    ] {
        if let Some(v) = value {
            let _ = writeln!(o, "{key} = {}", toml_string(v));
        }
    }
    write_requires(&mut o, &meta.requires);
    o.lines().map(str::to_string).collect()
}

fn write_requires(o: &mut String, requires: &[String]) {
    if requires.is_empty() {
        return;
    }
    let ids: Vec<String> = requires.iter().map(|r| toml_string(r)).collect();
    if let [one] = ids.as_slice() {
        let _ = writeln!(o, "requires = [{one}]");
    } else {
        // Two or more: one per line, as the TOML printer writes a longer array.
        o.push_str("requires = [\n");
        for id in &ids {
            let _ = writeln!(o, "    {id},");
        }
        o.push_str("]\n");
    }
}

pub fn write_manifest(
    library_id: &str,
    name: &str,
    meta: &LibraryMeta,
    classes: &[Class],
    tables: &[Table],
) -> String {
    let mut o = String::new();
    o.push_str("format = \"snxlib/2\"\n");
    let _ = writeln!(o, "library_id = {}", toml_string(library_id));
    // With no `[[classes]]` the format writes this root key on line 3.
    if classes.is_empty() {
        o.push_str("classes = []\n");
    }
    o.push_str("\n[library]\n");
    let _ = writeln!(o, "name = {}", toml_string(name));
    for (key, value) in [
        ("description", &meta.description),
        ("license", &meta.license),
        ("source", &meta.source),
        ("converted_by", &meta.converted_by),
    ] {
        if let Some(v) = value {
            let _ = writeln!(o, "{key} = {}", toml_string(v));
        }
    }
    write_requires(&mut o, &meta.requires);
    o.push_str(
        "\n[mode]\nkind = \"local_git\"\n\
         \n[workflow]\nmode = \"personal\"\nreview_required = false\nreviewers_required = 1\n\
         auto_lifecycle_promote = \"Released\"\n\
         \n[users]\ndefault_role = \"Designer\"\nallow_self_signup = false\n",
    );
    o.push_str(&write_classes(classes));
    // Tables in byte order of their names.
    let mut sorted: Vec<&Table> = tables.iter().collect();
    sorted.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    for t in sorted {
        o.push_str(&write_table(t));
    }
    o
}
