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
    internal_pn: String,
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
    rows: Vec<OldEntry>,
    classes: Vec<(String, String)>,
}

fn read_toml(path: &Path) -> Option<toml::Table> {
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

impl Previous {
    pub fn load(out: &Path) -> Previous {
        let mut p = Previous::default();
        if let Ok(text) = std::fs::read_to_string(out.join("import-report.json"))
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
        {
            for f in v["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str())
            {
                p.files.push(f.to_string());
            }
            for (f, l) in v["file_libraries"].as_object().into_iter().flatten() {
                if let Some(l) = l.as_str() {
                    p.file_libs.insert(f.clone(), l.to_string());
                }
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
                p.envelopes.insert(rel, doc);
            }
        }
        if let Ok(entries) = std::fs::read_dir(out) {
            for e in entries.filter_map(Result::ok) {
                if e.path().extension().is_some_and(|x| x == "snxlib")
                    && let Some(doc) = read_toml(&e.path())
                {
                    p.read_manifest(&doc);
                }
            }
        }
        p
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
            for line in tsv.lines().skip(1) {
                let c: Vec<&str> = line.split('\t').collect();
                if c.len() != 22 {
                    continue; // not a row this converter wrote
                }
                self.rows.push(OldEntry {
                    table: name.clone(),
                    row_id: c[0].to_string(),
                    internal_pn: c[1].to_string(),
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
        if let Some(old) = self.envelopes.get(rel) {
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
    /// not user files, and not those of a library that was not read cleanly.
    pub fn stale(&self, keep: &[String], clean: &HashSet<String>) -> Vec<PathBuf> {
        let mut stale: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|f| !keep.contains(f))
            .filter(|f| self.file_libs.get(*f).is_none_or(|lib| clean.contains(lib)))
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

    /// Rows of libraries that were not read cleanly this run are kept as they
    /// were, in their tables; their classes stay declared.
    pub fn carry_over(
        &self,
        clean: &HashSet<String>,
        tables: &mut Vec<Table>,
        classes: &mut Vec<Class>,
        items: &mut Vec<Item>,
    ) {
        let mut kept_libs: Vec<&str> = Vec::new();
        for r in &self.rows {
            let lib = r.internal_pn.split(':').next().unwrap_or("");
            if clean.contains(lib) {
                continue;
            }
            if !kept_libs.contains(&lib) {
                kept_libs.push(lib);
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
            if !r.class.is_empty() && !classes.iter().any(|c| c.key == r.class) {
                let label = self
                    .classes
                    .iter()
                    .find(|(k, _)| *k == r.class)
                    .map_or(r.class.clone(), |(_, l)| l.clone());
                classes.push(Class {
                    key: r.class.clone(),
                    label,
                });
            }
        }
        for lib in kept_libs {
            items.push(Item::new(
                ItemKind::Library,
                lib,
                "update.library_kept",
                Severity::Info,
                format!(
                    "{lib} was not read cleanly this run; its earlier part rows and files are kept"
                ),
            ));
        }
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
        let stale = p.stale(&keep, &clean);
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
}
