// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Derived ids of the library converter: nothing random, so the same input
//! gives the same library (contract section 5). The library id comes from
//! the source's identity (its name and its input file names, never absolute
//! paths); an item id from the library id plus the item's source name.

use uuid::Uuid;

/// The converter's own namespace for library ids (a v5 UUID of its name
/// under the DNS namespace would also do; this one is fixed text).
const LIBRARY_NAMESPACE: Uuid = Uuid::from_u128(0x6b4f_0c1e_5a2d_5e7b_9c3a_0d1f_7e2b_4a60);

/// The library id for display name `name` and the inputs' file names.
pub fn library_id(name: &str, input_names: &[&str]) -> Uuid {
    let mut text = format!("library\0{name}");
    for n in input_names {
        text.push('\0');
        text.push_str(n);
    }
    Uuid::new_v5(&LIBRARY_NAMESPACE, text.as_bytes())
}

/// The id of an item: `kind` is `symbol`, `symbol-file`, `footprint` or
/// `footprint-file`; `library` and `name` are the source's names.
pub fn item_id(library_id: Uuid, kind: &str, library: &str, name: &str) -> Uuid {
    Uuid::new_v5(&library_id, format!("{kind}\0{library}\0{name}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_depend_on_every_part() {
        let lib = library_id("Parts", &["a.kicad_sym"]);
        assert_eq!(lib, library_id("Parts", &["a.kicad_sym"]));
        assert_ne!(lib, library_id("Parts", &["b.kicad_sym"]));
        assert_ne!(lib, library_id("Other", &["a.kicad_sym"]));
        let id = item_id(lib, "symbol", "a", "R");
        assert_eq!(id, item_id(lib, "symbol", "a", "R"));
        assert_ne!(id, item_id(lib, "symbol", "a", "C"));
        assert_ne!(id, item_id(lib, "footprint", "a", "R"));
        assert_eq!(id.get_version_num(), 5);
    }
}
