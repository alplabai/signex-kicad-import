// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! `LICENSE` and `NOTICE.md` of a converted library, and the manifest keys
//! that say the same (`license`, `source`, `converted_by`).
//!
//! The converter never guesses a licence: it writes the one the user states
//! with `--license`. For `CC-BY-SA-4.0` (the KiCad standard libraries) the
//! full legal code and the libraries' design exception are bundled; any other
//! identifier is recorded in the manifest and the notice only, since the
//! converter has no text to bundle.

use snx_writer::LibraryMeta;

/// The legal code of CC BY-SA 4.0, from https://creativecommons.org/licenses/by-sa/4.0/legalcode.txt
pub const CC_BY_SA_4_0: &str = include_str!("../assets/CC-BY-SA-4.0.txt");
/// The exception of the KiCad libraries, verbatim from https://www.kicad.org/libraries/license/
pub const KICAD_EXCEPTION: &str = include_str!("../assets/KICAD-LIBRARIES-EXCEPTION.txt");

pub const LICENSE_FILE: &str = "LICENSE";
pub const NOTICE_FILE: &str = "NOTICE.md";

pub fn converter() -> String {
    format!("signex-kicad-import {}", env!("CARGO_PKG_VERSION"))
}

pub fn meta(license: Option<&str>, source: Option<&str>, requires: Vec<String>) -> LibraryMeta {
    LibraryMeta {
        description: None,
        license: license.map(str::to_string),
        source: source.map(str::to_string),
        converted_by: Some(converter()),
        requires,
    }
}

/// The text of `LICENSE`, when the converter has one for `license`.
pub fn license_text(license: Option<&str>) -> Option<String> {
    (license? == "CC-BY-SA-4.0").then(|| {
        format!(
            "This library is licensed under the Creative Commons Attribution-ShareAlike 4.0\n\
             International licence (CC-BY-SA-4.0), with the following exception of the KiCad\n\
             libraries (https://www.kicad.org/libraries/license/):\n\n{}\n\n\
             Licence text\n------------\n\n{}",
            KICAD_EXCEPTION.trim_end(),
            CC_BY_SA_4_0
        )
    })
}

/// `NOTICE.md`: what this library is, where it comes from, who converted it.
/// `inputs` are the source files' names with their SHA-256.
pub fn notice(
    license: Option<&str>,
    source: Option<&str>,
    inputs: &[(String, String)],
    libraries: &[String],
    contents: &str,
) -> String {
    let mut o = String::from("# Notice\n\n");
    o.push_str(
        "This library is a format conversion of KiCad libraries into the Signex library format. ",
    );
    o.push_str(&format!(
        "It holds {contents}, taken from the upstream libraries; nothing was redrawn.\n\n"
    ));
    o.push_str(&format!("- Converted by: {}\n", converter()));
    o.push_str(&format!(
        "- Source: {}\n",
        source.unwrap_or("not stated (pass --source)")
    ));
    match license {
        Some(l) => o.push_str(&format!("- Licence of the source libraries: {l}\n")),
        None => o.push_str("- Licence of the source libraries: not stated (pass --license); the licence of the source applies\n"),
    }
    if license == Some("CC-BY-SA-4.0") {
        o.push_str("  - `LICENSE` holds the legal code (https://creativecommons.org/licenses/by-sa/4.0/legalcode) and the KiCad libraries' design exception (https://www.kicad.org/libraries/license/).\n");
    } else if license.is_some() {
        o.push_str("  - The converter has no text for this licence; add it as `LICENSE` and keep the upstream's attribution.\n");
    }
    if license == Some("CC-BY-SA-4.0") {
        o.push_str("\nNon-binding note: the exception in `LICENSE` is the KiCad libraries' own; as its publisher explains, it lets designs made with the library data stay free of the licence, while a redistributed collection of the libraries stays under CC-BY-SA-4.0. Read `LICENSE` and the publisher's page, not this note, for what applies.\n");
    }
    o.push_str("\nRedistribute this library under the same licence as the source and keep this notice and `LICENSE`.\n");
    if !libraries.is_empty() {
        o.push_str("\n## Libraries\n\n");
        for l in libraries {
            o.push_str(&format!("- {l}\n"));
        }
    }
    if !inputs.is_empty() {
        o.push_str("\n## Input files\n\n");
        for (name, sha) in inputs {
            o.push_str(&format!("- `{name}` (sha256 `{sha}`)\n"));
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cc_licence_comes_with_the_kicad_exception_and_other_licences_do_not_bundle_text() {
        let text = license_text(Some("CC-BY-SA-4.0")).expect("text");
        assert!(text.contains("Attribution-ShareAlike 4.0 International"));
        assert!(text.contains("waives article 3 of the license"));
        assert!(
            !text.contains("This exception applies"),
            "no interpretation in the licence file"
        );
        assert!(license_text(Some("MIT")).is_none() && license_text(None).is_none());
        let n = notice(
            Some("CC-BY-SA-4.0"),
            Some("https://x/y@9.0.0"),
            &[("A.kicad_sym".into(), "ab".into())],
            &["A".into()],
            "symbols and part rows",
        );
        assert!(n.contains("holds symbols and part rows, taken"));
        assert!(!n.contains("3D"));
        assert!(
            n.contains("format conversion")
                && n.contains("https://x/y@9.0.0")
                && n.contains("signex-kicad-import")
        );
        assert!(n.contains("legalcode") && n.contains("kicad.org/libraries/license"));
        assert!(notice(None, None, &[], &[], "footprints").contains("not stated"));
    }
}
