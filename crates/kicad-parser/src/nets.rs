// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The board's nets. Older files number nets in a root table and items name
//! a net by number; newer ones name the net only. Both resolve to one
//! number per name here; net 0 is "no net" and is not listed.

use std::collections::BTreeMap;

use snx_writer::model::Net;

use crate::sexpr::{Atom, SExpr};

#[derive(Default)]
pub(crate) struct NetTable {
    by_number: BTreeMap<u32, String>,
    by_name: BTreeMap<String, u32>,
}

impl NetTable {
    /// The root `(net <n> "<name>")` entries.
    pub(crate) fn read(root: &SExpr) -> Self {
        let mut table = Self::default();
        for node in root.find_all("net") {
            if let (Some(number), Some(name)) = (raw_number(node), node.arg(1))
                && number != 0
                && !name.is_empty()
            {
                table.by_number.insert(number, name.to_string());
                table.by_name.entry(name.to_string()).or_insert(number);
            }
        }
        table
    }

    fn number_for_name(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        if let Some(&n) = self.by_name.get(name) {
            return n;
        }
        let n = self.by_number.keys().next_back().map_or(1, |last| last + 1);
        self.by_number.insert(n, name.to_string());
        self.by_name.insert(name.to_string(), n);
        n
    }

    /// The net an item's `(net ..)` child names: `(net 3)`, `(net 3 "GND")`
    /// or `(net "GND")`. 0 for none.
    pub(crate) fn of(&mut self, item: &SExpr) -> u32 {
        self.net_of(item).map_or(0, |net| net.number)
    }

    /// Like [`of`](Self::of), with the name; `None` for no net.
    pub(crate) fn net_of(&mut self, item: &SExpr) -> Option<Net> {
        let node = item.find("net")?;
        let number = match raw_number(node) {
            Some(n) => n,
            None => self.number_for_name(node.first_arg()?),
        };
        if number == 0 {
            return None;
        }
        let name = self.by_number.get(&number).cloned().unwrap_or_default();
        Some(Net { number, name })
    }

    /// The number of the net called `name`, adding it when new; 0 for "".
    pub(crate) fn named(&mut self, name: &str) -> u32 {
        self.number_for_name(name)
    }

    /// Every net, by number.
    pub(crate) fn nets(&self) -> Vec<Net> {
        self.by_number
            .iter()
            .map(|(&number, name)| Net {
                number,
                name: name.clone(),
            })
            .collect()
    }
}

/// The first argument as a net number when it is an unquoted integer.
fn raw_number(node: &SExpr) -> Option<u32> {
    match node.children().first()? {
        SExpr::Atom(atom @ Atom::Raw(_)) => atom.as_str().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    #[test]
    fn numbered_and_named_nets_resolve_to_one_table() {
        let root = sexpr::parse(
            r#"(kicad_pcb (net 0 "") (net 1 "GND") (net 2 "VCC")
               (segment (net 2)) (pad (net "SDA")) (pad (net "GND")) (pad (net "7")))"#,
        )
        .unwrap();
        let mut nets = NetTable::read(&root);
        let items = root.find_all("segment");
        assert_eq!(nets.of(items[0]), 2);
        let pads = root.find_all("pad");
        assert_eq!(nets.of(pads[0]), 3);
        assert_eq!(nets.of(pads[1]), 1);
        let quoted_digit = nets.net_of(pads[2]).expect("a net");
        assert_eq!((quoted_digit.number, quoted_digit.name.as_str()), (4, "7"));
        assert_eq!(nets.nets().len(), 4);
    }
}
