// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! KiCad symbol library reader: a `.kicad_sym` file (many symbols), or a
//! `.kicad_symdir` folder (one symbol per file), into a `SymbolLib` plus the
//! report items for what the reader skipped or did not understand.
//!
//! Format facts and their sources are in `docs/KICAD_SYMBOL_FORMAT.md`.

mod model;
mod parts;
mod symbol;

use std::collections::HashSet;
use std::path::Path;

use serde_json::json;

pub use model::*;

use crate::error::ParseError;
use crate::items::{Item, Severity};
use crate::sexpr::{self, SExpr};
use symbol::{Sink, parse_symbol};

/// A library read, with the report items of the read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SymbolLibRead {
    pub lib: SymbolLib,
    pub items: Vec<Item>,
}

/// Read the text of a `.kicad_sym`. `library` names it in report items (the
/// file stem). An unreadable file is an error; an unreadable symbol or an
/// unknown token is a report item.
pub fn read_symbol_lib(text: &str, library: &str) -> Result<SymbolLibRead, ParseError> {
    let mut read = SymbolLibRead::default();
    read_into(text, library, &mut read)?;
    finish(&mut read, library);
    Ok(read)
}

/// Read a `.kicad_sym` file. Bytes that are not UTF-8 are replaced by U+FFFD
/// and reported once (`symlib.invalid_utf8`, info).
pub fn read_symbol_file(path: &Path) -> Result<SymbolLibRead, ParseError> {
    let library = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("symbols")
        .to_string();
    let mut read = SymbolLibRead::default();
    read_file_into(path, &library, &mut read)?;
    finish(&mut read, &library);
    Ok(read)
}

fn read_file_into(path: &Path, library: &str, read: &mut SymbolLibRead) -> Result<(), ParseError> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    if matches!(text, std::borrow::Cow::Owned(_)) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let mut sink = Sink {
            library,
            items: &mut read.items,
        };
        sink.add(
            "",
            "symlib.invalid_utf8",
            Severity::Info,
            format!("{name} is not valid UTF-8; unreadable bytes replaced"),
            json!({ "file": name }),
        );
    }
    read_into(&text, library, read)
}

/// Read a `.kicad_symdir` folder: every `*.kicad_sym` file in it, by file
/// name, joined into one library. A file that does not read is a report item
/// (`symlib.file_unreadable`, error); other files are ignored with an `info`.
pub fn read_symbol_dir(dir: &Path) -> Result<SymbolLibRead, ParseError> {
    let library = dir
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("symbols")
        .to_string();
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut read = SymbolLibRead::default();
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let is_sym = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("kicad_sym"));
        let mut sink = Sink {
            library: &library,
            items: &mut read.items,
        };
        if !is_sym {
            sink.add(
                "",
                "symlib.file_ignored",
                Severity::Info,
                format!("{name} is not a .kicad_sym file; ignored"),
                json!({ "file": name }),
            );
            continue;
        }
        let result = read_file_into(&path, &library, &mut read);
        if let Err(e) = result {
            let mut sink = Sink {
                library: &library,
                items: &mut read.items,
            };
            sink.add(
                "",
                "symlib.file_unreadable",
                Severity::Error,
                format!("{name}: {e}; its symbols are not read"),
                json!({ "file": name }),
            );
        }
    }
    finish(&mut read, &library);
    Ok(read)
}

/// Parse `text` and add its symbols and items to `read`.
fn read_into(text: &str, library: &str, read: &mut SymbolLibRead) -> Result<(), ParseError> {
    let root = sexpr::parse(text)?;
    let nodes: Vec<&SExpr> = match root.keyword() {
        Some("kicad_symbol_lib") => {
            if let Some(v) = root.find("version").and_then(SExpr::first_arg) {
                read.lib.version.get_or_insert_with(|| v.to_string());
            }
            if let Some(g) = root.find("generator").and_then(SExpr::first_arg) {
                read.lib.generator.get_or_insert_with(|| g.to_string());
            }
            root.children()
                .iter()
                .filter(|c| c.keyword() == Some("symbol"))
                .collect()
        }
        // A file holding one bare symbol (the per-symbol form).
        Some("symbol") => vec![&root],
        other => {
            return Err(ParseError::InvalidValue(format!(
                "not a kicad_symbol_lib (starts with {})",
                other.unwrap_or("a value")
            )));
        }
    };
    let mut sink = Sink {
        library,
        items: &mut read.items,
    };
    if root.keyword() == Some("kicad_symbol_lib") {
        for child in root.children() {
            if let Some(k) = child.keyword()
                && !matches!(k, "version" | "generator" | "generator_version" | "symbol")
            {
                sink.add(
                    "",
                    "symlib.token_unknown",
                    Severity::Info,
                    format!("unknown library token {k} ignored"),
                    json!({ "token": k }),
                );
            }
        }
    }
    for node in nodes {
        if let Some(symbol) = parse_symbol(node, &mut sink) {
            read.lib.symbols.push(symbol);
        }
    }
    Ok(())
}

/// Checks that need every symbol: repeated names and `extends` chains.
fn finish(read: &mut SymbolLibRead, library: &str) {
    let mut sink = Sink {
        library,
        items: &mut read.items,
    };
    let mut seen = HashSet::new();
    let lib = &mut read.lib;
    lib.symbols.retain(|s| {
        let fresh = seen.insert(s.name.clone());
        if !fresh {
            sink.add(
                &s.name,
                "symbol.duplicate_name",
                Severity::Error,
                format!("symbol {} appears again; the later one is not read", s.name),
                json!({}),
            );
        }
        fresh
    });
    for s in &lib.symbols {
        if let Some(parent) = &s.extends {
            if lib.symbol(parent).is_none() {
                sink.add(
                    &s.name,
                    "symbol.extends_missing",
                    Severity::Error,
                    format!("parent {parent} not found in the library; symbol not usable"),
                    json!({ "parent": parent }),
                );
            } else if lib.effective_units(s).is_none() {
                sink.add(
                    &s.name,
                    "symbol.extends_cycle",
                    Severity::Error,
                    "extends chain loops or breaks; symbol not usable".into(),
                    json!({ "parent": parent }),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
