// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `.snxlib` manifest (`snxlib/2`, `LIBRARY_FORMAT.md` section 2.1) of a
//! library with no part tables yet: every key the format writes, default
//! workflow and users, then the classes and part tables given.

use std::fmt::Write as _;

use crate::canon::toml_string;
use crate::part_table::{Class, Table, write_classes, write_table};

pub fn write_manifest(
    library_id: &str,
    name: &str,
    description: Option<&str>,
    classes: &[Class],
    tables: &[Table],
) -> String {
    let mut o = String::new();
    o.push_str("format = \"snxlib/2\"\n");
    let _ = writeln!(o, "library_id = {}", toml_string(library_id));
    o.push_str("\n[library]\n");
    let _ = writeln!(o, "name = {}", toml_string(name));
    if let Some(d) = description {
        let _ = writeln!(o, "description = {}", toml_string(d));
    }
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
