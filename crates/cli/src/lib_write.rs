// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's convert phase: the read symbols and footprints
//! as the files of a Signex library directory.

use std::collections::{HashMap, HashSet};

use kicad_parser::libids::item_id;
use kicad_parser::parts::{Update, build_parts};
use kicad_parser::symlib::to_snx::to_symbol_file;
use kicad_parser::{Item, ItemKind, Severity};
use snx_writer::{write_footprint_file, write_manifest, write_symbol_file};

use crate::lib_notice;
use uuid::Uuid;

use crate::contract::Counts;
use crate::lib_read::Read;
use crate::lib_update::Previous;
use crate::progress::Emitter;

/// The files to write, relative to the library root, and what they hold.
pub struct Built {
    pub files: Vec<(String, Vec<u8>)>,
    /// With a split, the files of the footprint library.
    pub footprint_files: Vec<(String, Vec<u8>)>,
    pub written: Counts,
    pub items: Vec<Item>,
    /// Symbols that could have a part row (not power symbols).
    pub candidates: usize,
    /// The source library of each library file, by relative path.
    pub file_libs: Vec<(String, String)>,
    /// Existing envelopes this run left alone because they hold symbols the
    /// user added; they count as written for the update's bookkeeping.
    pub protected: Vec<String>,
    /// What this run wrote itself: row ids with their library, symbol ids,
    /// footprint ids (the report's ledger).
    pub ledger_rows: Vec<(String, String)>,
    pub ledger_symbols: Vec<String>,
    pub ledger_footprints: Vec<String>,
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

/// What the user said about the source: `--license` and `--source`.
#[derive(Clone, Copy, Default)]
pub struct Provenance<'a> {
    pub license: Option<&'a str>,
    pub source: Option<&'a str>,
}

/// The split output: footprints go to a second library with its own id.
#[derive(Clone, Copy)]
pub struct Split<'a> {
    pub footprint_library_id: Uuid,
    pub footprint_library_name: &'a str,
}

/// The state an `--update` run builds against.
pub struct UpdateCx<'a> {
    pub previous: &'a Previous,
    /// The run's time, `2026-01-02T03:04:05Z`.
    pub now: &'a str,
}

pub fn build(
    read: &Read,
    library_id: Uuid,
    name: &str,
    emitter: &Emitter,
    update: Option<&UpdateCx<'_>>,
    provenance: Provenance<'_>,
    split: Option<Split<'_>>,
) -> Built {
    let mut footprint_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut file_libs: Vec<(String, String)> = Vec::new();
    let mut protected: Vec<String> = Vec::new();
    let mut ledger_symbols: Vec<String> = Vec::new();
    let mut ledger_footprints: Vec<String> = Vec::new();
    let mut symbol_versions: HashMap<String, String> = HashMap::new();
    let mut footprint_versions: HashMap<String, String> = HashMap::new();
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
        let mut file = to_symbol_file(&symbols.lib, library, file_uuid.clone(), &id_of, &mut items);
        written.symbols += file.symbols.len();
        let mut rel = format!("symbols/{}.snxsym", slug(library));
        if !taken.insert(rel.clone()) {
            rel = format!("symbols/{file_uuid}.snxsym");
            taken.insert(rel.clone());
        }
        if let Some(u) = update {
            symbol_versions.extend(u.previous.apply_symbols(&rel, &mut file, u.now));
        } else {
            symbol_versions.extend(
                file.symbols
                    .iter()
                    .map(|s| (s.uuid.clone(), s.version.clone())),
            );
        }
        if update.is_some_and(|u| u.previous.has_user_symbols(&rel)) {
            // The user added symbols to this envelope: it stays as it is.
            items.push(Item::new(
                ItemKind::Library,
                library.clone(),
                "update.envelope_has_user_items",
                Severity::Dropped,
                format!("{rel} holds symbols this converter did not write; it was not rewritten, so its symbols are not updated"),
            ));
            written.symbols -= file.symbols.len();
            protected.push(rel);
            continue;
        }
        ledger_symbols.extend(file.symbols.iter().map(|s| s.uuid.clone()));
        file_libs.push((rel.clone(), library.clone()));
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
    for (fp, lib) in read.footprints.iter().zip(&read.footprint_libs) {
        let mut fp = fp.clone();
        emitter.progress(done, Some(total), &fp.footprint.name);
        written.footprints += 1;
        written.models += fp.models.len();
        let rel = format!("footprints/{}.snxfpt", fp.footprint.uuid);
        if let Some(u) = update {
            let (uuid, version) = u.previous.apply_footprint(&rel, &mut fp, u.now);
            footprint_versions.insert(uuid, version);
        } else {
            footprint_versions.insert(fp.footprint.uuid.clone(), fp.footprint.version.clone());
        }
        items.push(
            Item::new(
                ItemKind::Footprint,
                format!("{lib}:{}", fp.footprint.name),
                "footprint.written",
                Severity::Info,
                "footprint written",
            )
            .with_id(&fp.footprint.uuid),
        );
        ledger_footprints.push(fp.footprint.uuid.clone());
        file_libs.push((rel.clone(), lib.clone()));
        let target = if split.is_some() {
            &mut footprint_files
        } else {
            &mut files
        };
        target.push((rel, write_footprint_file(&fp).into_bytes()));
        done += 1;
        emitter.progress(done, Some(total), &fp.footprint.name);
    }
    // Part rows: one per symbol that is not a power symbol, whose footprint was converted.
    let footprints: HashSet<String> = read
        .footprints
        .iter()
        .map(|f| f.footprint.uuid.clone())
        .collect();
    let row_update = match update {
        Some(u) => u.previous.row_update(
            &u.now.replace('Z', "+00:00"),
            symbol_versions,
            footprint_versions,
        ),
        None => Update::default(),
    };
    let mut parts = build_parts(
        &read.symbols,
        library_id,
        split.map_or(library_id, |s| s.footprint_library_id),
        &footprints,
        &row_update,
    );
    let generated: HashSet<String> = parts.generated.iter().map(|(id, _)| id.clone()).collect();
    let mut ledger_rows = parts.generated.clone();
    if let Some(u) = update {
        let retained = u.previous.carry_over(
            &generated,
            &read.clean_libraries,
            &read.attempted_libraries,
            &mut parts.tables,
            &mut parts.classes,
            &mut parts.items,
        );
        ledger_rows.extend(u.previous.retained_rows(&retained));
    }
    written.parts = parts.rows;
    items.extend(parts.items);
    if provenance.license.is_none() {
        items.push(Item::new(
            ItemKind::Library,
            name,
            "library.license_unknown",
            Severity::Info,
            "no --license given: the library carries no licence of its own; the source's licence applies",
        ));
    }
    let requires = split
        .map(|s| vec![s.footprint_library_id.to_string()])
        .unwrap_or_default();
    let mut meta = lib_notice::meta(provenance.license, provenance.source, requires);
    meta.description = None;
    files.push((
        manifest_name(name),
        write_manifest(
            &library_id.to_string(),
            name,
            &meta,
            &parts.classes,
            &parts.tables,
        )
        .into_bytes(),
    ));
    if let Some(text) = lib_notice::license_text(provenance.license) {
        files.push((lib_notice::LICENSE_FILE.to_string(), text.into_bytes()));
    }
    let inputs: Vec<(String, String)> = read
        .sources
        .iter()
        .map(|s| {
            (
                s.path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                s.sha256.clone(),
            )
        })
        .collect();
    let mut libraries: Vec<String> = read
        .symbols
        .iter()
        .map(|(l, _)| l.clone())
        .chain(
            read.footprint_libs
                .iter()
                .filter(|_| split.is_none())
                .cloned(),
        )
        .collect();
    libraries.sort();
    libraries.dedup();
    files.push((
        lib_notice::NOTICE_FILE.to_string(),
        lib_notice::notice(provenance.license, provenance.source, &inputs, &libraries).into_bytes(),
    ));
    if let Some(s) = split {
        // The footprint library: its own manifest (no part tables), licence and notice.
        let meta = lib_notice::meta(provenance.license, provenance.source, Vec::new());
        footprint_files.push((
            manifest_name(s.footprint_library_name),
            write_manifest(
                &s.footprint_library_id.to_string(),
                s.footprint_library_name,
                &meta,
                &[],
                &[],
            )
            .into_bytes(),
        ));
        if let Some(text) = lib_notice::license_text(provenance.license) {
            footprint_files.push((lib_notice::LICENSE_FILE.to_string(), text.into_bytes()));
        }
        let mut fp_libs: Vec<String> = read.footprint_libs.clone();
        fp_libs.sort();
        fp_libs.dedup();
        footprint_files.push((
            lib_notice::NOTICE_FILE.to_string(),
            lib_notice::notice(provenance.license, provenance.source, &inputs, &fp_libs)
                .into_bytes(),
        ));
    }
    Built {
        footprint_files,
        files,
        written,
        items,
        candidates: parts.candidates,
        file_libs,
        protected,
        ledger_rows,
        ledger_symbols,
        ledger_footprints,
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
