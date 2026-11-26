// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's convert phase: the read symbols and footprints
//! as the files of a Signex library directory.

use std::collections::HashSet;

use kicad_parser::libids::item_id;
use kicad_parser::symlib::to_snx::to_symbol_file;
use kicad_parser::{Item, ItemKind, Severity};
use snx_writer::{write_footprint_file, write_manifest, write_symbol_file};
use uuid::Uuid;

use crate::contract::Counts;
use crate::lib_read::Read;
use crate::progress::Emitter;

/// The files to write, relative to the library root, and what they hold.
pub struct Built {
    pub files: Vec<(String, Vec<u8>)>,
    pub written: Counts,
    pub items: Vec<Item>,
}

/// A file name for a symbol envelope: lower-case ASCII letters and digits,
/// each run of anything else one `-`, none at the ends (`LIBRARY_FORMAT.md`
/// section 1); `untitled` when nothing is left.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}

/// The `.snxlib` file name for a library name: the name with the characters
/// a file name cannot hold replaced.
pub fn manifest_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim().trim_matches('.').to_string();
    format!("{}.snxlib", if safe.is_empty() { "library" } else { &safe })
}

pub fn build(read: &Read, library_id: Uuid, name: &str, emitter: &Emitter) -> Built {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut items = Vec::new();
    let mut written = Counts::default();
    let total = read.symbols.len() + read.footprints.len();
    emitter.phase("convert", Some(total));
    let mut done = 0;
    let mut taken: HashSet<String> = HashSet::new();
    for (library, symbols) in &read.symbols {
        emitter.progress(done, Some(total), library);
        let file_uuid = item_id(library_id, "symbol-file", library, library).to_string();
        let id_of = |symbol: &str| item_id(library_id, "symbol", library, symbol).to_string();
        let file = to_symbol_file(&symbols.lib, library, file_uuid.clone(), &id_of, &mut items);
        written.symbols += file.symbols.len();
        let mut rel = format!("symbols/{}.snxsym", slug(library));
        if !taken.insert(rel.clone()) {
            rel = format!("symbols/{file_uuid}.snxsym");
            taken.insert(rel.clone());
        }
        files.push((rel, write_symbol_file(&file).into_bytes()));
        for s in &file.symbols {
            items.push(
                Item::new(
                    ItemKind::Symbol,
                    format!("{library}:{}", s.name),
                    "symbol.written",
                    Severity::Info,
                    "symbol written",
                )
                .with_id(&s.uuid),
            );
        }
        done += 1;
        emitter.progress(done, Some(total), library);
    }
    for fp in &read.footprints {
        emitter.progress(done, Some(total), &fp.footprint.name);
        written.footprints += 1;
        items.push(
            Item::new(
                ItemKind::Footprint,
                fp.footprint.name.clone(),
                "footprint.written",
                Severity::Info,
                "footprint written",
            )
            .with_id(&fp.footprint.uuid),
        );
        files.push((
            format!("footprints/{}.snxfpt", fp.footprint.uuid),
            write_footprint_file(fp).into_bytes(),
        ));
        done += 1;
        emitter.progress(done, Some(total), &fp.footprint.name);
    }
    files.push((
        manifest_name(name),
        write_manifest(&library_id.to_string(), name, None).into_bytes(),
    ));
    Built {
        files,
        written,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_the_envelope_naming_rule() {
        assert_eq!(slug("Conformance Symbols"), "conformance-symbols");
        assert_eq!(slug("--A__b--"), "a-b");
        assert_eq!(slug("日本"), "untitled");
        assert_eq!(manifest_name("A/B:C"), "A_B_C.snxlib");
        assert_eq!(manifest_name("  "), "library.snxlib");
    }
}
