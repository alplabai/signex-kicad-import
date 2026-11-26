// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's read phase: every input is hashed for the
//! report's `sources`, and symbol libraries are read into the model (the
//! symbol count and the reader's report items).

use std::path::PathBuf;

use anyhow::Result;
use kicad_parser::symlib::{SymbolLibRead, read_symbol_dir, read_symbol_file};
use kicad_parser::{Item, ItemKind, Severity};
use serde_json::json;

use crate::contract::Counts;
use crate::lib_inputs::InputKind;
use crate::lib_report::{Source, sha256_of};
use crate::progress::Emitter;

pub struct Read {
    pub sources: Vec<Source>,
    pub counts: Counts,
    pub items: Vec<Item>,
    /// The symbol libraries read, in input order.
    pub symbols: Vec<SymbolLibRead>,
}

pub fn read_inputs(inputs: &[(PathBuf, InputKind)], emitter: &Emitter) -> Result<Read> {
    let mut read = Read {
        sources: Vec::new(),
        counts: Counts::default(),
        items: Vec::new(),
        symbols: Vec::new(),
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
        let result = match kind {
            InputKind::SymbolFile => Some(read_symbol_file(path).map_err(|e| e.to_string())),
            InputKind::SymbolDir => Some(read_symbol_dir(path).map_err(|e| e.to_string())),
            _ => None,
        };
        match result {
            Some(Ok(symbols)) => {
                read.counts.symbols += symbols.lib.symbols.len();
                read.items.extend(symbols.items.iter().cloned());
                read.symbols.push(symbols);
            }
            Some(Err(message)) => read.items.push(
                Item::new(
                    ItemKind::Library,
                    &lib_name,
                    "symlib.unreadable",
                    Severity::Error,
                    format!("{name}: {message}; its symbols are not read"),
                )
                .with_detail(json!({ "file": name })),
            ),
            None => {}
        }
        emitter.progress(done + 1, Some(inputs.len()), &name);
    }
    Ok(read)
}
