// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's read phase: every input is hashed for the
//! report's `sources`; symbol libraries are read into the model and
//! footprints are converted (their envelopes are built here, since a
//! footprint converts on its own). A library table reads every library it
//! lists, under the table's names.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use kicad_parser::footprint_lib::{read_footprint_file, read_pretty_dir};
use kicad_parser::lib_table::{TableKind, parse_lib_table, resolve_entry};
use kicad_parser::libids::item_id;
use kicad_parser::symlib::{SymbolLibRead, read_symbol_dir, read_symbol_file};
use kicad_parser::{Item, ItemKind, Severity};
use serde_json::json;
use snx_writer::footprint_file::FootprintFile;
use uuid::Uuid;

use crate::contract::Counts;
use crate::lib_inputs::InputKind;
use crate::lib_report::{Source, sha256_of};
use crate::progress::Emitter;

pub struct Read {
    pub sources: Vec<Source>,
    /// What the sources hold: symbols and footprints found.
    pub counts: Counts,
    pub items: Vec<Item>,
    /// The symbol libraries read, as (library name, library), in input order.
    pub symbols: Vec<(String, SymbolLibRead)>,
    pub footprints: Vec<FootprintFile>,
}

struct Reader {
    read: Read,
    library_id: Uuid,
    seen_libraries: HashSet<String>,
    seen_footprints: HashSet<String>,
}

pub fn read_inputs(
    inputs: &[(PathBuf, InputKind)],
    library_id: Uuid,
    search: &[PathBuf],
    emitter: &Emitter,
) -> Result<Read> {
    let mut r = Reader {
        read: Read {
            sources: Vec::new(),
            counts: Counts::default(),
            items: Vec::new(),
            symbols: Vec::new(),
            footprints: Vec::new(),
        },
        library_id,
        seen_libraries: HashSet::new(),
        seen_footprints: HashSet::new(),
    };
    for (done, (path, kind)) in inputs.iter().enumerate() {
        let name = file_name(path);
        emitter.progress(done, Some(inputs.len()), &name);
        r.read.sources.push(Source {
            path: path.clone(),
            sha256: sha256_of(path)?,
        });
        let stem = path
            .file_stem()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match kind {
            InputKind::SymbolFile | InputKind::SymbolDir => r.symbol_library(path, &stem),
            InputKind::FootprintDir => r.footprint_library(path, &stem),
            InputKind::FootprintFile => {
                let library = path.parent().and_then(|p| p.file_stem()).map_or_else(
                    || "footprints".to_string(),
                    |s| s.to_string_lossy().into_owned(),
                );
                r.footprint_file(path, &library);
            }
            InputKind::SymLibTable | InputKind::FpLibTable => r.table(path, search)?,
        }
        emitter.progress(done + 1, Some(inputs.len()), &name);
    }
    Ok(r.read)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

impl Reader {
    fn error(&mut self, kind: ItemKind, source: &str, code: &str, message: String, file: &str) {
        self.read.items.push(
            Item::new(kind, source, code, Severity::Error, message)
                .with_detail(json!({ "file": file })),
        );
    }

    /// Read a symbol file or folder as the library called `lib`.
    fn symbol_library(&mut self, path: &Path, lib: &str) {
        let name = file_name(path);
        if !self.seen_libraries.insert(lib.to_string()) {
            let message = format!(
                "{name}: a symbol library named {lib} was already read; this one is not read"
            );
            self.error(
                ItemKind::Library,
                lib,
                "symlib.duplicate_name",
                message,
                &name,
            );
            return;
        }
        let result = if path.is_dir() {
            read_symbol_dir(path)
        } else {
            read_symbol_file(path)
        };
        match result {
            Ok(symbols) => {
                self.read.counts.symbols += symbols.lib.symbols.len();
                self.read.items.extend(symbols.items.iter().cloned());
                self.read.symbols.push((lib.to_string(), symbols));
            }
            Err(e) => {
                let message = format!("{name}: {e}; its contents are not read");
                self.error(ItemKind::Library, lib, "symlib.unreadable", message, &name);
            }
        }
    }

    fn footprint_library(&mut self, path: &Path, lib: &str) {
        let name = file_name(path);
        let library_id = self.library_id;
        let ids = move |is_file: bool, fp: &str| footprint_id(library_id, lib, is_file, fp);
        match read_pretty_dir(path, &ids) {
            Ok(fps) => self.add_footprints(fps.found, fps.items, fps.files, lib, &name),
            Err(e) => {
                let message = format!("{name}: {e}; its contents are not read");
                self.error(ItemKind::Library, lib, "symlib.unreadable", message, &name);
            }
        }
    }

    fn footprint_file(&mut self, path: &Path, lib: &str) {
        let name = file_name(path);
        let library_id = self.library_id;
        let ids = move |is_file: bool, fp: &str| footprint_id(library_id, lib, is_file, fp);
        let fps = read_footprint_file(path, lib, &ids);
        self.add_footprints(fps.found, fps.items, fps.files, lib, &name);
    }

    fn add_footprints(
        &mut self,
        found: usize,
        items: Vec<Item>,
        files: Vec<FootprintFile>,
        lib: &str,
        from: &str,
    ) {
        self.read.counts.footprints += found;
        self.read.items.extend(items);
        for file in files {
            // The same footprint can arrive twice (a .pretty and a .kicad_mod
            // inside it): one id, one file.
            if self.seen_footprints.insert(file.footprint.uuid.clone()) {
                self.read.footprints.push(file);
            } else {
                self.read.items.push(Item::new(
                    ItemKind::Footprint,
                    format!("{lib}:{}", file.footprint.name),
                    "footprint.duplicate_name",
                    Severity::Error,
                    format!(
                        "footprint {} was already read; the repeat from {from} is not read",
                        file.footprint.name
                    ),
                ));
            }
        }
    }

    /// Read every library a `sym-lib-table` / `fp-lib-table` lists. An entry
    /// that cannot be used is an error item; the others are still converted.
    fn table(&mut self, path: &Path, search: &[PathBuf]) -> Result<()> {
        let name = file_name(path);
        let table = match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| parse_lib_table(&t).map_err(|e| e.to_string()))
        {
            Ok(t) => t,
            Err(e) => {
                let message = format!("{name}: {e}; its libraries are not read");
                self.error(
                    ItemKind::Library,
                    &name,
                    "library.table_unreadable",
                    message,
                    &name,
                );
                return Ok(());
            }
        };
        let dir = path.parent().unwrap_or(Path::new("."));
        let env = |var: &str| std::env::var(var).ok();
        for entry in &table.entries {
            let source = format!("{name}:{}", entry.name);
            if entry.disabled {
                self.read.items.push(Item::new(
                    ItemKind::Library,
                    &source,
                    "library.table_entry_disabled",
                    Severity::Info,
                    format!("{} is disabled in {name}; not converted", entry.name),
                ));
                continue;
            }
            if entry.kind != "KiCad" {
                let message = format!(
                    "{}: library type {:?} is not supported; not converted",
                    entry.name, entry.kind
                );
                self.error(
                    ItemKind::Library,
                    &source,
                    "library.table_type_unsupported",
                    message,
                    &name,
                );
                continue;
            }
            let resolved = match resolve_entry(entry, dir, search, &env) {
                Ok(p) => p,
                Err(why) => {
                    let message = format!("{}: {why}; not converted", entry.name);
                    self.error(
                        ItemKind::Library,
                        &source,
                        "library.table_entry_unresolved",
                        message,
                        &name,
                    );
                    continue;
                }
            };
            self.read.sources.push(Source {
                path: resolved.clone(),
                sha256: sha256_of(&resolved)?,
            });
            match table.kind {
                TableKind::Symbol => self.symbol_library(&resolved, &entry.name),
                TableKind::Footprint => self.footprint_library(&resolved, &entry.name),
            }
        }
        Ok(())
    }
}

/// The uuid of a footprint (or of its envelope) in library `lib`.
pub fn footprint_id(library_id: Uuid, lib: &str, is_file: bool, name: &str) -> String {
    let kind = if is_file {
        "footprint-file"
    } else {
        "footprint"
    };
    item_id(library_id, kind, lib, name).to_string()
}
