// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's read phase: every input is hashed for the
//! report's `sources`; symbol libraries are read into the model and
//! footprints are converted (their envelopes are built here, since a
//! footprint converts on its own).

use std::path::PathBuf;

use anyhow::Result;
use kicad_parser::footprint_lib::{read_footprint_file, read_pretty_dir};
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

pub fn read_inputs(
    inputs: &[(PathBuf, InputKind)],
    library_id: Uuid,
    emitter: &Emitter,
) -> Result<Read> {
    let mut read = Read {
        sources: Vec::new(),
        counts: Counts::default(),
        items: Vec::new(),
        symbols: Vec::new(),
        footprints: Vec::new(),
    };
    for (done, (path, kind)) in inputs.iter().enumerate() {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        emitter.progress(done, Some(inputs.len()), &name);
        read.sources.push(Source {
            path: path.clone(),
            sha256: sha256_of(path)?,
        });
        let lib_name = path
            .file_stem()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let unreadable = |message: String| {
            Item::new(
                ItemKind::Library,
                &lib_name,
                "symlib.unreadable",
                Severity::Error,
                format!("{name}: {message}; its contents are not read"),
            )
            .with_detail(json!({ "file": name }))
        };
        let ids = |lib: String| {
            move |is_file: bool, fp: &str| {
                let kind = if is_file {
                    "footprint-file"
                } else {
                    "footprint"
                };
                item_id(library_id, kind, &lib, fp).to_string()
            }
        };
        match kind {
            InputKind::SymbolFile | InputKind::SymbolDir => {
                let result = if *kind == InputKind::SymbolFile {
                    read_symbol_file(path)
                } else {
                    read_symbol_dir(path)
                };
                match result {
                    Ok(symbols) => {
                        read.counts.symbols += symbols.lib.symbols.len();
                        read.items.extend(symbols.items.iter().cloned());
                        read.symbols.push((lib_name.clone(), symbols));
                    }
                    Err(e) => read.items.push(unreadable(e.to_string())),
                }
            }
            InputKind::FootprintDir | InputKind::FootprintFile => {
                let (library, fps) = if *kind == InputKind::FootprintDir {
                    (lib_name.clone(), read_pretty_dir(path, &ids(lib_name.clone())).map_err(|e| e.to_string()))
                } else {
                    let library = path
                        .parent()
                        .and_then(|p| p.file_stem())
                        .map_or_else(|| "footprints".to_string(), |s| s.to_string_lossy().into_owned());
                    (library.clone(), Ok(read_footprint_file(path, &library, &ids(library.clone()))))
                };
                match fps {
                    Ok(fps) => {
                        read.counts.footprints += fps.found;
                        read.items.extend(fps.items);
                        read.footprints.extend(fps.files);
                        let _ = library;
                    }
                    Err(e) => read.items.push(unreadable(e)),
                }
            }
            InputKind::SymLibTable | InputKind::FpLibTable => read.items.push(Item::new(
                ItemKind::Library,
                &name,
                "library.table_not_read",
                Severity::Error,
                format!("{name}: library tables are not read yet; the table's libraries are not converted"),
            )),
        }
        emitter.progress(done + 1, Some(inputs.len()), &name);
    }
    Ok(read)
}
