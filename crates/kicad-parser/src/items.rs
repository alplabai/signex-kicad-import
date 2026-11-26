// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Structured report items of the library converter contract: one entry
//! per thing a run read, wrote or could not carry over, with a stable
//! machine code. The vocabulary (kinds, severities, item fields) is the
//! contract's loss report, `docs/LIBRARY_CONVERTER_CONTRACT.md` section 4
//! in the Signex repository.

use serde_json::{Value, json};

/// What a report item is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Symbol,
    Footprint,
    Part,
    Model,
    Row,
    Library,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Symbol => "symbol",
            ItemKind::Footprint => "footprint",
            ItemKind::Part => "part",
            ItemKind::Model => "model",
            ItemKind::Row => "row",
            ItemKind::Library => "library",
        }
    }
}

/// How much an item lost; the order is least to most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Nothing lost.
    Info,
    /// Written, with a documented approximation.
    Approximated,
    /// Written, but part of it was not.
    Dropped,
    /// The item was not written at all.
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Approximated => "approximated",
            Severity::Dropped => "dropped",
            Severity::Error => "error",
        }
    }

    /// `dropped` and `error` make a run `partial` (exit code 2).
    pub fn is_loss(self) -> bool {
        self >= Severity::Dropped
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    /// The item's name in the source, `<library>:<item>` where it has one.
    pub source: String,
    /// The Signex id written for it; `None` when it was not written.
    pub id: Option<String>,
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub detail: Value,
}

impl Item {
    pub fn new(
        kind: ItemKind,
        source: impl Into<String>,
        code: &str,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Item {
            kind,
            source: source.into(),
            id: None,
            code: code.to_string(),
            severity,
            message: message.into(),
            detail: Value::Null,
        }
    }

    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "kind": self.kind.as_str(),
            "source": self.source,
            "id": self.id,
            "code": self.code,
            "severity": self.severity.as_str(),
            "message": self.message,
        });
        if !self.detail.is_null() {
            value["detail"] = self.detail.clone();
        }
        value
    }
}
