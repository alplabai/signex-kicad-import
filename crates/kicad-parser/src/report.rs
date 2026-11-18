// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! What a conversion could not carry over exactly, counted by kind.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    counts: BTreeMap<String, usize>,
}

impl Report {
    /// Count one occurrence of `message`.
    pub fn note(&mut self, message: impl Into<String>) {
        *self.counts.entry(message.into()).or_insert(0) += 1;
    }

    /// Add every count of `other` to this report.
    pub fn merge(&mut self, other: &Report) {
        for (message, count) in &other.counts {
            *self.counts.entry(message.clone()).or_insert(0) += count;
        }
    }

    /// `(message, count)` pairs, by message.
    pub fn entries(&self) -> impl Iterator<Item = (&str, usize)> {
        self.counts.iter().map(|(m, &n)| (m.as_str(), n))
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// One line per kind: `<count> x <message>`.
    pub fn summary(&self) -> String {
        let lines: Vec<String> = self
            .entries()
            .map(|(message, count)| format!("{count} x {message}"))
            .collect();
        lines.join("; ")
    }
}
