// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The library converter contract's JSON documents: `describe`, the summary
//! event and the exit code vocabulary. The contract is
//! `docs/LIBRARY_CONVERTER_CONTRACT.md` in the Signex repository (public
//! document); its schema is copied for the tests under `tests/fixtures`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kicad_parser::{Item, ItemKind, Severity};
use serde_json::{Value, json};

pub const CONTRACT_VERSION: &str = "1.0";

pub fn converter_name() -> &'static str {
    "signex-kicad-import"
}

/// The `describe --json` document (schema `#/$defs/describe`).
///
/// The two library tables have no extension, so their full file names stand
/// in the `extensions` list (see `docs/LIBRARY_CONVERTER.md`).
pub fn describe() -> Value {
    json!({
        "contract": CONTRACT_VERSION,
        "converter": converter_name(),
        "version": env!("CARGO_PKG_VERSION"),
        "inputs": [
            { "label": "KiCad symbol library", "extensions": ["kicad_sym"], "kind": "library" },
            { "label": "KiCad symbol directory", "extensions": ["kicad_symdir"], "kind": "library" },
            { "label": "KiCad footprint library", "extensions": ["pretty", "kicad_mod"], "kind": "library" },
            {
                "label": "KiCad library table",
                "extensions": ["sym-lib-table", "fp-lib-table"],
                "kind": "library"
            },
        ],
    })
}

// `Ok` and `Partial` are produced once the library writers exist.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Ok,
    Partial,
    Failed,
}

impl Exit {
    pub fn code(self) -> i32 {
        match self {
            Exit::Ok => 0,
            Exit::Partial => 2,
            Exit::Failed => 1,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Exit::Ok => "ok",
            Exit::Partial => "partial",
            Exit::Failed => "failed",
        }
    }

    #[allow(dead_code)]
    /// A finished run is `partial` when any item was dropped or not written.
    pub fn of_items(items: &[Item]) -> Exit {
        if items.iter().any(|i| i.severity.is_loss()) {
            Exit::Partial
        } else {
            Exit::Ok
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub symbols: usize,
    pub footprints: usize,
    pub parts: usize,
    pub models: usize,
}

impl Counts {
    pub fn to_json(self) -> Value {
        json!({
            "symbols": self.symbols,
            "footprints": self.footprints,
            "parts": self.parts,
            "models": self.models,
        })
    }
}

/// `{message, count}` for the non-`info` items, aggregated by message.
pub fn warnings(items: &[Item]) -> Vec<Value> {
    let mut by_message: BTreeMap<&str, usize> = BTreeMap::new();
    for item in items.iter().filter(|i| i.severity != Severity::Info) {
        let count = item
            .detail
            .get("count")
            .and_then(Value::as_u64)
            .map_or(1, |n| n as usize);
        *by_message.entry(item.message.as_str()).or_insert(0) += count;
    }
    by_message
        .into_iter()
        .map(|(message, count)| json!({ "message": message, "count": count }))
        .collect()
}

/// Source items that were not written: error items other than the library's own.
pub fn skipped(items: &[Item]) -> usize {
    items
        .iter()
        .filter(|i| i.severity == Severity::Error && i.kind != ItemKind::Library)
        .count()
}

/// What the summary event needs from a run.
pub struct Outcome {
    pub exit: Exit,
    pub error: Option<String>,
    pub library: Option<PathBuf>,
    pub report: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub written: Counts,
    pub items: Vec<Item>,
}

impl Outcome {
    /// A run that stopped before it read anything.
    pub fn failed(error: impl Into<String>) -> Self {
        Outcome {
            exit: Exit::Failed,
            error: Some(error.into()),
            library: None,
            report: None,
            files: Vec::new(),
            written: Counts::default(),
            items: Vec::new(),
        }
    }

    /// The summary event (schema `#/$defs/summary`).
    pub fn summary(&self) -> Value {
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string());
        let mut counts = self.written.to_json();
        counts["skipped"] = json!(skipped(&self.items));
        json!({
            "event": "summary",
            "ok": self.exit != Exit::Failed,
            "error": self.error,
            "exit": self.exit.as_str(),
            "library": path(&self.library),
            "report": path(&self.report),
            "files": self.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
            "counts": counts,
            "warnings": warnings(&self.items),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(severity: Severity, message: &str, count: Option<u64>) -> Item {
        let mut i = Item::new(ItemKind::Symbol, "L:S", "a.b", severity, message);
        if let Some(n) = count {
            i.detail = json!({ "count": n });
        }
        i
    }

    #[test]
    fn exit_codes_follow_the_worst_item() {
        assert_eq!(Exit::of_items(&[]).code(), 0);
        let approx = [item(Severity::Approximated, "a", None)];
        assert_eq!(Exit::of_items(&approx).code(), 0);
        let dropped = [
            item(Severity::Info, "i", None),
            item(Severity::Dropped, "d", None),
        ];
        assert_eq!(Exit::of_items(&dropped).code(), 2);
        assert_eq!(Exit::Failed.code(), 1);
    }

    #[test]
    fn warnings_skip_info_and_sum_counts_by_message() {
        let items = [
            item(Severity::Info, "quiet", None),
            item(Severity::Dropped, "loud", Some(3)),
            item(Severity::Error, "loud", None),
        ];
        assert_eq!(
            warnings(&items),
            vec![json!({ "message": "loud", "count": 4 })]
        );
    }

    #[test]
    fn a_failed_summary_has_the_failure_shape() {
        let s = Outcome::failed("bad input").summary();
        assert_eq!(s["ok"], false);
        assert_eq!(s["exit"], "failed");
        assert_eq!(s["error"], "bad input");
        assert_eq!(s["files"], json!([]));
        assert_eq!(s["event"], "summary");
    }
}
