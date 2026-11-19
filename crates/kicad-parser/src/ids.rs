// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Item identities. Every item keeps its source uuid when it has a unique
//! one; a missing or repeated uuid gets a fresh one drawn from a sequence
//! seeded by the input text, so the same input always gives the same file.

use std::collections::HashSet;

use uuid::{Builder, Uuid};

use crate::report::Report;
use crate::sexpr::SExpr;

pub(crate) struct Ids {
    state: u64,
    seen: HashSet<Uuid>,
}

impl Ids {
    /// A sequence seeded by `text` (FNV-1a).
    pub(crate) fn seeded(text: &str) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        Self {
            state: hash,
            seen: HashSet::new(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        // splitmix64
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uuid no item has yet.
    pub(crate) fn fresh(&mut self) -> Uuid {
        loop {
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&self.next_u64().to_le_bytes());
            bytes[8..].copy_from_slice(&self.next_u64().to_le_bytes());
            let id = Builder::from_random_bytes(bytes).into_uuid();
            if self.seen.insert(id) {
                return id;
            }
        }
    }

    /// The uuid of `node` (`uuid` or the older `tstamp`) when it is unique
    /// so far, else a fresh one; a repeat is reported.
    pub(crate) fn of(&mut self, node: &SExpr, report: &mut Report) -> Uuid {
        let parsed = ["uuid", "tstamp"]
            .iter()
            .filter_map(|key| node.find(key))
            .filter_map(|n| n.first_arg())
            .find_map(|s| Uuid::parse_str(s).ok());
        match parsed {
            Some(id) if !id.is_nil() && self.seen.insert(id) => id,
            Some(_) => {
                report.note("repeated uuid replaced with a fresh one");
                self.fresh()
            }
            None => self.fresh(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    #[test]
    fn the_same_text_gives_the_same_ids() {
        let mut a = Ids::seeded("board");
        let mut b = Ids::seeded("board");
        assert_eq!(a.fresh(), b.fresh());
        assert_ne!(Ids::seeded("other").fresh(), Ids::seeded("board").fresh());
    }

    #[test]
    fn a_repeated_uuid_is_replaced_and_reported() {
        let node = sexpr::parse("(pad (uuid \"0e3f5a1c-6a4e-4f7b-9d2b-1a2b3c4d5e6f\"))").unwrap();
        let mut ids = Ids::seeded("x");
        let mut report = Report::default();
        let first = ids.of(&node, &mut report);
        let second = ids.of(&node, &mut report);
        assert_eq!(first.to_string(), "0e3f5a1c-6a4e-4f7b-9d2b-1a2b3c4d5e6f");
        assert_ne!(first, second);
        assert!(!report.is_empty());
    }
}
