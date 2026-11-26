// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! `--update` (contract section 5): what the previous run wrote, read back
//! from the output folder, and the rules that follow from it.
//!
//! * Only files the previous run wrote (its report's `files`) are ever
//!   deleted, and only when the library they came from was read cleanly this
//!   run; anything else in the folder, and everything of a library that failed
//!   to read, stays.
//! * A primitive (symbol, footprint) whose content is unchanged keeps its
//!   version and times; a changed one gets a patch version bump and the run's
//!   time as `updated`.
//! * A part row whose `content_hash` is unchanged keeps its bytes; a changed
//!   one gets a patch bump, `updated` = the run's time, and keeps `created`,
//!   state and `released`.
//! * Rows of libraries that were not read cleanly this run are carried over.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use kicad_parser::parts::{OldRow, Update, bump_patch};
use kicad_parser::{Item, ItemKind, Severity};
use snx_writer::footprint_file::FootprintFile;
use snx_writer::part_table::{Class, Table};
use snx_writer::symbol_file::SymbolFile;
use snx_writer::{write_footprint_file, write_symbol_file};

struct OldEntry {
    table: String,
    row_id: String,
    class: String,
    old: OldRow,
}

#[derive(Default)]
pub struct Previous {
    /// Relative paths the previous run wrote, and the library each came from.
    files: Vec<String>,
    file_libs: HashMap<String, String>,
    symbols: HashMap<String, toml::Table>,
    footprints: HashMap<String, toml::Table>,
    envelopes: HashMap<String, toml::Table>,
    /// The same envelopes by their `file_uuid`, so a renamed file is found.
    envelopes_by_uuid: HashMap<String, toml::Table>,
    /// Root manifests found (relative names).
    manifests: Vec<String>,
    /// Entries of the earlier report that were not trusted.
    pub ignored: Vec<String>,
    /// An untrusted entry was found: this run deletes nothing.
    tainted: bool,
    /// What the earlier run wrote itself, from its report (validated):
    /// row ids with their source library, symbol ids, footprint ids. Only
    /// these are ever deleted; every other row, symbol and file is the
    /// user's and stays.
    ledger_rows: HashMap<String, String>,
    ledger_symbols: HashSet<String>,
    ledger_footprints: HashSet<String>,
    rows: Vec<OldEntry>,
    /// Tables whose columns are not ours, verbatim: (name, tsv text).
    foreign_tables: Vec<(String, String)>,
    classes: Vec<(String, String)>,
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    // A link is never followed: only a regular file is read.
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return None;
    }
    toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn sans_meta(t: &toml::Table) -> toml::Table {
    let mut t = t.clone();
    for k in ["version", "created", "updated"] {
        t.remove(k);
    }
    t
}

fn text<'a>(t: &'a toml::Table, key: &str) -> &'a str {
    t.get(key).and_then(toml::Value::as_str).unwrap_or("")
}

/// A library or file name that is safe in a ledger: not empty, no separator,
/// no `:` (a drive or stream on Windows), no control character.
fn plain_name(s: &str) -> bool {
    !s.is_empty() && !s.contains(['/', '\\', ':']) && !s.chars().any(char::is_control)
}

fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

impl Previous {
    /// The ledger of the earlier report, as untrusted as the file list: an id
    /// that is not a uuid or a library name that is not plain taints the run.
    fn read_ledger(&mut self, ledger: &serde_json::Value) {
        let strings = |key: &str| -> Vec<String> {
            ledger[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        };
        for id in strings("symbols") {
            if is_uuid(&id) {
                self.ledger_symbols.insert(id);
            } else {
                self.ignored.push(id);
                self.tainted = true;
            }
        }
        for id in strings("footprints") {
            if is_uuid(&id) {
                self.ledger_footprints.insert(id);
            } else {
                self.ignored.push(id);
                self.tainted = true;
            }
        }
        for row in ledger["rows"].as_array().into_iter().flatten() {
            match (row["id"].as_str(), row["library"].as_str()) {
                (Some(id), Some(lib)) if is_uuid(id) && plain_name(lib) => {
                    self.ledger_rows.insert(id.to_string(), lib.to_string());
                }
                _ => {
                    self.ignored.push(row.to_string());
                    self.tainted = true;
                }
            }
        }
    }

    /// Whether the envelope at `rel` holds a symbol the converter did not write.
    pub fn has_user_symbols(&self, rel: &str) -> bool {
        self.envelopes.get(rel).is_some_and(|e| {
            e.get("symbols")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .any(|s| {
                    s.as_table()
                        .is_some_and(|t| !self.ledger_symbols.contains(text(t, "uuid")))
                })
        })
    }

    /// Ids written earlier that stay because their file stays.
    pub fn retained_ids(&self, kept_files: &[String]) -> (Vec<String>, Vec<String>) {
        let (mut syms, mut fps) = (Vec::new(), Vec::new());
        for f in kept_files {
            let Some(e) = self.envelopes.get(f) else {
                continue;
            };
            for s in e
                .get("symbols")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_table)
            {
                if self.ledger_symbols.contains(text(s, "uuid")) {
                    syms.push(text(s, "uuid").to_string());
                }
            }
            for s in e
                .get("footprints")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_table)
            {
                if self.ledger_footprints.contains(text(s, "uuid")) {
                    fps.push(text(s, "uuid").to_string());
                }
            }
        }
        (syms, fps)
    }

    /// The earlier row ids that remain the converter's after this run: kept
    /// rows of libraries that failed to read.
    pub fn retained_rows(&self, kept: &HashSet<String>) -> Vec<(String, String)> {
        kept.iter()
            .filter_map(|id| self.ledger_rows.get(id).map(|l| (id.clone(), l.clone())))
            .collect()
    }

    pub fn load(out: &Path) -> Previous {
        let mut p = Previous::default();
        let mut claimed: Vec<String> = Vec::new();
        if let Ok(text) = std::fs::read_to_string(out.join("import-report.json"))
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
        {
            if v["converter"] == "signex-kicad-import" {
                claimed = v["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect();
                for (f, l) in v["file_libraries"].as_object().into_iter().flatten() {
                    match l.as_str() {
                        Some(l) if plain_name(l) => {
                            p.file_libs.insert(f.clone(), l.to_string());
                        }
                        _ => {
                            p.ignored.push(f.clone());
                            p.tainted = true;
                        }
                    }
                }
                p.read_ledger(&v["ledger"]);
            } else {
                p.ignored
                    .push("import-report.json is not from this converter".to_string());
                p.tainted = true;
            }
        }
        for (dir, ext) in [("symbols", "snxsym"), ("footprints", "snxfpt")] {
            let Ok(entries) = std::fs::read_dir(out.join(dir)) else {
                continue;
            };
            for e in entries.filter_map(Result::ok) {
                let path = e.path();
                if path.extension().is_none_or(|x| x != ext) {
                    continue;
                }
                let Some(doc) = read_toml(&path) else {
                    continue;
                };
                let rel = format!("{dir}/{}", e.file_name().to_string_lossy());
                let list = if dir == "symbols" {
                    "symbols"
                } else {
                    "footprints"
                };
                for item in doc
                    .get(list)
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(t) = item.as_table() {
                        let map = if dir == "symbols" {
                            &mut p.symbols
                        } else {
                            &mut p.footprints
                        };
                        map.insert(text(t, "uuid").to_string(), t.clone());
                    }
                }
                if let Some(u) = doc.get("file_uuid").and_then(toml::Value::as_str) {
                    p.envelopes_by_uuid.insert(u.to_string(), doc.clone());
                }
                p.envelopes.insert(rel, doc);
            }
        }
        if let Ok(entries) = std::fs::read_dir(out) {
            for e in entries.filter_map(Result::ok) {
                if e.path().extension().is_some_and(|x| x == "snxlib")
                    && let Some(doc) = read_toml(&e.path())
                {
                    p.manifests
                        .push(e.file_name().to_string_lossy().into_owned());
                    p.read_manifest(&doc);
                }
            }
        }
        // Only entries that are exactly what the converter writes, found
        // where the folder says, as regular files of a kind that matches
        // their name, are ever deleted. One bad entry stops all deletion.
        for f in claimed {
            if p.trusted(out, &f) {
                p.files.push(f);
            } else {
                p.ignored.push(f);
                p.tainted = true;
            }
        }
        p
    }

    /// Whether a path the earlier report lists may be touched.
    fn trusted(&self, out: &Path, f: &str) -> bool {
        let rel = Path::new(f);
        let parts: Vec<&std::ffi::OsStr> = rel
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(n) => Some(n),
                _ => None,
            })
            .collect();
        if parts.len() != rel.components().count() || f.contains(['\\', ':']) {
            return false;
        }
        let name = |i: usize| parts.get(i).and_then(|n| n.to_str()).unwrap_or("");
        let shape = match parts.len() {
            1 => name(0).ends_with(".snxlib") && name(0).len() > ".snxlib".len(),
            2 => match name(0) {
                "symbols" => name(1).ends_with(".snxsym") && name(1).len() > ".snxsym".len(),
                "footprints" => name(1)
                    .strip_suffix(".snxfpt")
                    .is_some_and(|u| uuid::Uuid::parse_str(u).is_ok()),
                _ => false,
            },
            _ => false,
        };
        if !shape {
            return false;
        }
        // A file that is there must be a regular file, in a real folder inside
        // the output; one that is not there is harmless.
        if parts.len() == 2 {
            let dir = out.join(name(0));
            if !std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
                return false;
            }
            match (dir.canonicalize(), out.canonicalize()) {
                (Ok(d), Ok(o)) if d.starts_with(&o) => {}
                _ => return false,
            }
        }
        match std::fs::symlink_metadata(out.join(rel)) {
            Ok(m) if !m.is_file() => return false,
            Err(_) => return true,
            Ok(_) => {}
        }
        // What the file holds must match its name and kind.
        match (parts.len(), name(0)) {
            (1, _) => self.manifests.iter().any(|m| m == f),
            (_, "symbols") => self
                .envelopes
                .get(f)
                .is_some_and(|e| e.contains_key("symbols")),
            _ => {
                let stem = name(1).trim_end_matches(".snxfpt");
                self.envelopes.get(f).is_some_and(|e| {
                    e.get("footprints")
                        .and_then(toml::Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(toml::Value::as_table)
                        .is_some_and(|t| {
                            text(t, "uuid") == stem && self.ledger_footprints.contains(stem)
                        })
                })
            }
        }
    }

    fn read_manifest(&mut self, doc: &toml::Table) {
        for c in doc
            .get("classes")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(c) = c.as_table() {
                self.classes
                    .push((text(c, "key").to_string(), text(c, "label").to_string()));
            }
        }
        for (name, table) in doc
            .get("tables")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten()
        {
            let Some(tsv) = table.get("tsv").and_then(toml::Value::as_str) else {
                continue;
            };
            let header = tsv.lines().next().unwrap_or("");
            if !header.starts_with("row_id\tinternal_pn\tclass\tdatasheet\t")
                || header.split('\t').count() != 22
            {
                self.foreign_tables.push((name.clone(), tsv.to_string()));
                continue;
            }
            for line in tsv.lines().skip(1) {
                let c: Vec<&str> = line.split('\t').collect();
                if c.len() != 22 {
                    continue; // not a row this converter wrote
                }
                self.rows.push(OldEntry {
                    table: name.clone(),
                    row_id: c[0].to_string(),
                    class: c[2].to_string(),
                    old: OldRow {
                        line: line.to_string(),
                        created: c[13].to_string(),
                        state: c[4].to_string(),
                        released: c[17] == "true",
                        version: c[16].to_string(),
                        content_hash: c[15].to_string(),
                        symbol_version: c[18].to_string(),
                        footprint_version: c[19].to_string(),
                    },
                });
            }
        }
    }

    /// The earlier rows by `row_id`, the run's `now` and the primitive
    /// versions, for the row builder.
    pub fn row_update(
        &self,
        now: &str,
        symbols: HashMap<String, String>,
        footprints: HashMap<String, String>,
    ) -> Update {
        Update {
            symbol_versions: symbols,
            footprint_versions: footprints,
            old_rows: self
                .rows
                .iter()
                .map(|r| (r.row_id.clone(), r.old.clone()))
                .collect(),
            now: now.to_string(),
        }
    }

    /// Give the symbols their versions: unchanged ones what they had, changed
    /// ones a patch bump and `now`. Returns each symbol's uuid and version.
    pub fn apply_symbols(
        &self,
        rel: &str,
        file: &mut SymbolFile,
        now: &str,
    ) -> HashMap<String, String> {
        let rendered: toml::Table = toml::from_str(&write_symbol_file(file)).unwrap_or_default();
        let new: Vec<toml::Table> = rendered
            .get("symbols")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_table().cloned())
            .collect();
        let mut versions = HashMap::new();
        let mut all_same = new.len() == file.symbols.len();
        for (s, n) in file.symbols.iter_mut().zip(&new) {
            if let Some(old) = self.symbols.get(&s.uuid) {
                s.created = text(old, "created").to_string();
                if sans_meta(old) == sans_meta(n) {
                    s.version = text(old, "version").to_string();
                    s.updated = text(old, "updated").to_string();
                } else {
                    s.version = bump_patch(text(old, "version"));
                    s.updated = now.to_string();
                    all_same = false;
                }
            } else {
                all_same = false;
            }
            versions.insert(s.uuid.clone(), s.version.clone());
        }
        if let Some(old) = self
            .envelopes
            .get(rel)
            .or_else(|| self.envelopes_by_uuid.get(&file.file_uuid))
        {
            file.created = text(old, "created").to_string();
            let old_count = old
                .get("symbols")
                .and_then(toml::Value::as_array)
                .map_or(0, Vec::len);
            file.updated = if all_same && old_count == file.symbols.len() {
                text(old, "updated").to_string()
            } else {
                now.to_string()
            };
        }
        versions
    }

    /// The same for a footprint envelope; returns its uuid and version.
    pub fn apply_footprint(
        &self,
        rel: &str,
        file: &mut FootprintFile,
        now: &str,
    ) -> (String, String) {
        let rendered: toml::Table = toml::from_str(&write_footprint_file(file)).unwrap_or_default();
        let new = rendered
            .get("footprints")
            .and_then(toml::Value::as_array)
            .and_then(|a| a.first())
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default();
        let f = &mut file.footprint;
        let mut same = false;
        if let Some(old) = self.footprints.get(&f.uuid) {
            f.created = text(old, "created").to_string();
            if sans_meta(old) == sans_meta(&new) {
                same = true;
                f.version = text(old, "version").to_string();
                f.updated = text(old, "updated").to_string();
            } else {
                f.version = bump_patch(text(old, "version"));
                f.updated = now.to_string();
            }
        }
        if let Some(old) = self.envelopes.get(rel) {
            file.created = text(old, "created").to_string();
            file.updated = if same {
                text(old, "updated").to_string()
            } else {
                now.to_string()
            };
        }
        (file.footprint.uuid.clone(), file.footprint.version.clone())
    }

    /// Files the previous run wrote that this run does not, and may delete:
    /// only those the report names with a source library, when that library
    /// was read cleanly this run or is no longer an input; the root manifest
    /// is rewritten, so an older one under another name goes. Nothing at all
    /// when the report held an entry that could not be trusted.
    pub fn stale(
        &self,
        keep: &[String],
        clean: &HashSet<String>,
        attempted: &HashSet<String>,
    ) -> Vec<PathBuf> {
        if self.tainted {
            return Vec::new();
        }
        let mut stale: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|f| !keep.contains(f))
            .filter(|f| !self.has_user_symbols(f))
            .filter(|f| match self.file_libs.get(*f) {
                Some(lib) => clean.contains(lib) || !attempted.contains(lib),
                None => !f.contains('/'),
            })
            .map(PathBuf::from)
            .collect();
        stale.sort();
        stale
    }

    /// The files of the report after this run: the ones written now, and the
    /// earlier ones that stay (their library was not read cleanly).
    pub fn files_after(&self, keep: &[String], stale: &[PathBuf]) -> Vec<String> {
        let mut files: Vec<String> = keep.to_vec();
        for f in &self.files {
            if !keep.contains(f) && !stale.iter().any(|s| s == Path::new(f)) {
                files.push(f.clone());
            }
        }
        files.sort();
        files
    }

    /// The libraries of earlier files that stay (see [`Previous::files_after`]).
    pub fn lib_of(&self, file: &str) -> Option<&String> {
        self.file_libs.get(file)
    }

    /// Old rows that stay, byte for byte, in their tables. Every row that is
    /// not the converter's own (not in the ledger) is the user's and stays.
    /// A row the converter wrote and did not write again is deleted when its
    /// library read cleanly this run or is no longer an input, and kept when
    /// the library is an input that failed to read. Returns the ids of the
    /// converter's own rows that stay.
    pub fn carry_over(
        &self,
        generated: &HashSet<String>,
        clean: &HashSet<String>,
        attempted: &HashSet<String>,
        tables: &mut Vec<Table>,
        classes: &mut Vec<Class>,
        items: &mut Vec<Item>,
    ) -> HashSet<String> {
        let mut kept_libs: Vec<String> = Vec::new();
        let mut removed_libs: Vec<String> = Vec::new();
        let mut retained: HashSet<String> = HashSet::new();
        for r in &self.rows {
            if generated.contains(&r.row_id) {
                continue;
            }
            if let Some(lib) = self.ledger_rows.get(&r.row_id) {
                let drop_it = !self.tainted && (clean.contains(lib) || !attempted.contains(lib));
                if drop_it {
                    if !attempted.contains(lib) && !removed_libs.contains(lib) {
                        removed_libs.push(lib.clone());
                    }
                    continue;
                }
                retained.insert(r.row_id.clone());
                if attempted.contains(lib) && !kept_libs.contains(lib) {
                    kept_libs.push(lib.clone());
                }
            }
            let idx = match tables.iter().position(|t| t.name == r.table) {
                Some(i) => i,
                None => {
                    tables.push(Table {
                        name: r.table.clone(),
                        ..Table::default()
                    });
                    tables.len() - 1
                }
            };
            tables[idx]
                .kept
                .push((r.row_id.clone(), r.old.line.clone()));
            self.declare_class(&r.class, classes);
        }
        for (name, tsv) in &self.foreign_tables {
            if !tables.iter().any(|t| &t.name == name) {
                tables.push(Table {
                    name: name.clone(),
                    raw_tsv: Some(tsv.clone()),
                    ..Table::default()
                });
            }
        }
        // Classes the user declared stay declared.
        for (key, _) in &self.classes {
            self.declare_class(key, classes);
        }
        removed_libs.extend(
            self.file_libs
                .values()
                .filter(|l| !attempted.contains(*l))
                .cloned(),
        );
        removed_libs.sort_unstable();
        removed_libs.dedup();
        for lib in removed_libs.into_iter().filter(|_| !self.tainted) {
            items.push(Item::new(
                ItemKind::Library,
                &lib,
                "update.library_removed",
                Severity::Info,
                format!("{lib} is no longer an input; the files and part rows this converter wrote for it are removed"),
            ));
        }
        for item in &self.ignored {
            items.push(Item::new(
                ItemKind::Library,
                item,
                "update.report_entry_ignored",
                Severity::Info,
                "an entry of the earlier report is not one this converter writes; nothing was deleted in this run",
            ));
        }
        for lib in kept_libs {
            items.push(Item::new(
                ItemKind::Library,
                &lib,
                "update.library_kept",
                Severity::Info,
                format!(
                    "{lib} was not read cleanly this run; its earlier part rows and files are kept"
                ),
            ));
        }
        retained
    }

    fn declare_class(&self, key: &str, classes: &mut Vec<Class>) {
        if key.is_empty() || classes.iter().any(|c| c.key == key) {
            return;
        }
        let label = self
            .classes
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| key.to_string(), |(_, l)| l.clone());
        classes.push(Class {
            key: key.to_string(),
            label,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prev_with(files: &[(&str, &str)]) -> Previous {
        let mut p = Previous::default();
        for (f, lib) in files {
            p.files.push((*f).to_string());
            if !lib.is_empty() {
                p.file_libs.insert((*f).to_string(), (*lib).to_string());
            }
        }
        p
    }

    #[test]
    fn only_files_of_clean_libraries_the_run_no_longer_writes_are_stale() {
        let p = prev_with(&[
            ("symbols/a.snxsym", "A"),
            ("symbols/b.snxsym", "B"),
            ("footprints/x.snxfpt", "A"),
            ("footprints/y.snxfpt", "A"),
            ("Old.snxlib", ""),
        ]);
        let keep = vec![
            "symbols/a.snxsym".to_string(),
            "footprints/x.snxfpt".to_string(),
        ];
        let clean: HashSet<String> = ["A".to_string()].into();
        let attempted: HashSet<String> = ["A".to_string(), "B".to_string()].into();
        let stale = p.stale(&keep, &clean, &attempted);
        // B failed to read this run: its file stays. The manifest is rewritten under a new name.
        assert_eq!(
            stale,
            [
                PathBuf::from("Old.snxlib"),
                PathBuf::from("footprints/y.snxfpt")
            ]
        );
        let files = p.files_after(&keep, &stale);
        assert_eq!(
            files,
            [
                "footprints/x.snxfpt",
                "symbols/a.snxsym",
                "symbols/b.snxsym"
            ]
        );
    }

    #[test]
    fn a_removed_library_is_deleted_and_an_entry_without_a_library_never_is() {
        let mut p = prev_with(&[
            ("symbols/gone.snxsym", "Gone"),
            ("symbols/stray.snxsym", ""),
            ("Old.snxlib", ""),
        ]);
        let none: HashSet<String> = HashSet::new();
        assert_eq!(
            p.stale(&[], &none, &none),
            [
                PathBuf::from("Old.snxlib"),
                PathBuf::from("symbols/gone.snxsym")
            ]
        );
        p.tainted = true;
        assert!(p.stale(&[], &none, &none).is_empty());
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("snx-upd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    #[test]
    fn a_crafted_report_cannot_make_an_update_delete_outside_the_library() {
        let base = scratch("crafted");
        let out = base.join("lib");
        std::fs::create_dir_all(out.join("symbols")).expect("d");
        std::fs::write(base.join("victim"), "victim").expect("f");
        std::fs::write(base.join("victim2"), "victim2").expect("f");
        #[cfg(unix)]
        std::os::unix::fs::symlink(base.join("victim2"), out.join("symbols/link.snxsym"))
            .expect("link");
        let report = serde_json::json!({
            "converter": "signex-kicad-import",
            "files": ["../victim", base.join("victim2").display().to_string(), "symbols/link.snxsym", "symbols/../../victim", "other/x.snxsym", "symbols/a.txt"],
            "file_libraries": {"../victim": "L", "symbols/link.snxsym": "L"},
        });
        std::fs::write(out.join("import-report.json"), report.to_string()).expect("report");
        let p = Previous::load(&out);
        assert!(p.files.is_empty(), "{:?}", p.files);
        assert_eq!(p.ignored.len(), 6);
        let all: HashSet<String> = ["L".to_string()].into();
        assert!(p.stale(&[], &all, &all).is_empty());
        assert_eq!(std::fs::read(base.join("victim")).expect("v"), b"victim");
        assert_eq!(std::fs::read(base.join("victim2")).expect("v"), b"victim2");
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[test]
    fn a_report_from_another_converter_is_not_trusted_and_a_good_one_is() {
        let base = scratch("converter");
        let out = base.join("lib");
        std::fs::create_dir_all(out.join("symbols")).expect("d");
        let env = "format = \"snxsym/v1\"\nfile_uuid = \"u\"\n[[symbols]]\nuuid = \"s\"\n";
        std::fs::write(out.join("symbols/a.snxsym"), env).expect("f");
        let write = |converter: &str| {
            let report = serde_json::json!({"converter": converter, "files": ["symbols/a.snxsym"], "file_libraries": {"symbols/a.snxsym": "A"}});
            std::fs::write(out.join("import-report.json"), report.to_string()).expect("report");
            Previous::load(&out)
        };
        let bad = write("something-else");
        assert!(bad.files.is_empty() && bad.tainted);
        let good = write("signex-kicad-import");
        assert_eq!(good.files, ["symbols/a.snxsym"]);
        assert!(!good.tainted);
        // A file whose content is not what its name says is not trusted either.
        std::fs::write(out.join("symbols/a.snxsym"), "not = \"an envelope\"\n").expect("f");
        assert!(write("signex-kicad-import").tainted);
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[test]
    fn a_renamed_envelope_keeps_its_times_and_a_changed_symbol_is_bumped() {
        use snx_writer::symbol_file::Symbol;
        let symbol = |uuid: &str, comment: &str| Symbol {
            uuid: uuid.into(),
            name: "S".into(),
            anchor: [0.0, 0.0],
            pins: vec![],
            designator: "U?".into(),
            comment: comment.into(),
            designator_pos: None,
            comment_pos: None,
            description: String::new(),
            component_type: "Standard".into(),
            mirrored: false,
            local_fill_color: None,
            local_line_color: None,
            local_pin_color: None,
            version: "1.0.0".into(),
            released: false,
            part_count: 1,
            created: "1970-01-01T00:00:00Z".into(),
            updated: "1970-01-01T00:00:00Z".into(),
            graphics: vec![],
            params: vec![],
        };
        let file = |comment: &str| SymbolFile {
            file_uuid: "file-1".into(),
            display_name: "L".into(),
            created: "1970-01-01T00:00:00Z".into(),
            updated: "1970-01-01T00:00:00Z".into(),
            symbols: vec![symbol("11111111-1111-4111-8111-111111111111", comment)],
        };
        // The earlier run wrote the envelope at symbols/old-name.snxsym with real times.
        let base = scratch("renamed");
        std::fs::create_dir_all(base.join("symbols")).expect("d");
        let mut first = file("*");
        first.created = "2020-01-01T00:00:00Z".into();
        first.updated = "2020-02-02T00:00:00Z".into();
        first.symbols[0].created.clone_from(&first.created);
        first.symbols[0].updated.clone_from(&first.updated);
        std::fs::write(
            base.join("symbols/old-name.snxsym"),
            write_symbol_file(&first),
        )
        .expect("f");
        let prev = Previous::load(&base);
        // Unchanged content under a new file name: times and version stay.
        let mut same = file("*");
        prev.apply_symbols("symbols/new-name.snxsym", &mut same, "2026-11-26T10:00:00Z");
        assert_eq!(
            (same.created.as_str(), same.updated.as_str()),
            ("2020-01-01T00:00:00Z", "2020-02-02T00:00:00Z")
        );
        assert_eq!(
            (
                same.symbols[0].version.as_str(),
                same.symbols[0].updated.as_str()
            ),
            ("1.0.0", "2020-02-02T00:00:00Z")
        );
        // Changed content: created kept, updated now, version bumped.
        let mut changed = file("different");
        prev.apply_symbols(
            "symbols/new-name.snxsym",
            &mut changed,
            "2026-11-26T10:00:00Z",
        );
        assert_eq!(
            (changed.created.as_str(), changed.updated.as_str()),
            ("2020-01-01T00:00:00Z", "2026-11-26T10:00:00Z")
        );
        assert_eq!(
            (
                changed.symbols[0].version.as_str(),
                changed.symbols[0].created.as_str()
            ),
            ("1.0.1", "2020-01-01T00:00:00Z")
        );
        std::fs::remove_dir_all(&base).expect("cleanup");
    }
}
