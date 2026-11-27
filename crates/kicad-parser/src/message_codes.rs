// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Stable codes for the board conversion's free-text report messages.
//!
//! The board conversion counts messages (`Report`); the library converter
//! contract wants a code per kind of loss. This catalogue maps each message
//! to a `<area>.<what>` code and a severity without changing the message
//! text, so the legacy `--json` summary stays as it was. A code never
//! changes meaning; the list is documented in `docs/REPORT_CODES.md`.

use serde_json::{Value, json};

use crate::items::Severity;
use crate::items::Severity::{Approximated, Dropped, Info};

enum Pattern {
    /// The whole message.
    Exact(&'static str),
    /// A message that starts and ends with these, the variable part between.
    Affix(&'static str, &'static str),
}

use Pattern::{Affix, Exact};

/// First match wins, so an exact message precedes the broader affix it
/// would also fit.
const CATALOGUE: &[(Pattern, &str, Severity)] = &[
    (
        Exact("zone without an outline dropped"),
        "zone.outline_missing",
        Dropped,
    ),
    (
        Exact("zone that leaves pads unconnected written with direct pad connections"),
        "zone.pad_connection_approximated",
        Approximated,
    ),
    (
        Exact("zone on a non-copper layer dropped from that layer"),
        "zone.layer_dropped",
        Dropped,
    ),
    (
        Exact("zone corner that does not read dropped"),
        "zone.corner_unreadable",
        Dropped,
    ),
    (
        Exact("zone area apart from its first outline written as its own zone"),
        "zone.area_split",
        Approximated,
    ),
    (
        Exact("hatched zone written as a solid pour"),
        "zone.hatched_approximated",
        Approximated,
    ),
    (
        Exact("rule area that allows some items written as a full copper keepout"),
        "zone.rule_area_approximated",
        Approximated,
    ),
    (
        Exact("footprint zone written as a board zone"),
        "footprint.zone_moved",
        Approximated,
    ),
    (
        Exact("via without a readable position dropped"),
        "via.position_missing",
        Dropped,
    ),
    (
        Exact("via without a positive drill and size dropped"),
        "via.size_missing",
        Dropped,
    ),
    (
        Exact("track without its ends dropped"),
        "track.ends_missing",
        Dropped,
    ),
    (
        Exact("unfilled copper drawing written as tracks"),
        "copper_drawing.written_as_tracks",
        Approximated,
    ),
    (
        Exact("copper drawing without its points dropped"),
        "copper_drawing.points_missing",
        Dropped,
    ),
    (
        Exact("copper drawing without a positive width dropped"),
        "copper_drawing.width_missing",
        Dropped,
    ),
    (
        Exact("copper arc without its three points dropped"),
        "copper_arc.points_missing",
        Dropped,
    ),
    (
        Exact("trapezoid pad narrowing on both axes written as a custom outline"),
        "pad.trapezoid_both_axes",
        Approximated,
    ),
    (
        Exact("trapezoid pad delta wider than the pad written as a custom outline"),
        "pad.trapezoid_delta_wide",
        Approximated,
    ),
    (
        Exact("slot drill kept as a round hole of its width, marked oval"),
        "pad.slot_approximated",
        Approximated,
    ),
    (
        Exact("pad outline not a simple polygon; anchor shape kept"),
        "pad.outline_not_simple",
        Approximated,
    ),
    (
        Exact("hole in custom pad copper filled"),
        "pad.custom_hole_filled",
        Approximated,
    ),
    (
        Exact("drill offset from the pad centre ignored"),
        "pad.drill_offset_ignored",
        Dropped,
    ),
    (
        Exact("custom pad polygon outline width ignored"),
        "pad.custom_outline_width_ignored",
        Dropped,
    ),
    (
        Exact("custom pad polygon corner that does not read dropped"),
        "pad.custom_corner_unreadable",
        Dropped,
    ),
    (
        Exact("custom pad copper that does not merge written as its anchor shape alone"),
        "pad.custom_copper_unmerged",
        Approximated,
    ),
    (
        Exact("custom pad copper apart from the rest of the pad dropped"),
        "pad.custom_copper_detached",
        Dropped,
    ),
    (
        Exact("chamfered pad corners written as round corners"),
        "pad.chamfer_approximated",
        Approximated,
    ),
    (
        Exact("pad without a readable position dropped"),
        "pad.position_missing",
        Dropped,
    ),
    (
        Exact("pad without a positive size dropped"),
        "pad.size_missing",
        Dropped,
    ),
    (
        Exact("stackup with a negative thickness or permittivity not written"),
        "stackup.negative_value",
        Dropped,
    ),
    (
        Exact("stackup layer listed again ignored"),
        "stackup.layer_repeated",
        Dropped,
    ),
    (
        Exact("stackup layer colour dropped"),
        "stackup.colour_dropped",
        Dropped,
    ),
    (
        Exact("stackup dielectric without a permittivity written without one"),
        "stackup.permittivity_missing",
        Approximated,
    ),
    (
        Exact("stackup dielectric sublayers written as one row"),
        "stackup.sublayers_merged",
        Approximated,
    ),
    (
        Exact("stackup copper does not match the board's copper layers; stackup not written"),
        "stackup.copper_mismatch",
        Dropped,
    ),
    (
        Exact("net class with no nets and no patterns dropped"),
        "net_class.empty",
        Dropped,
    ),
    (
        Exact("net class pattern without a class or pattern dropped"),
        "net_class.pattern_incomplete",
        Dropped,
    ),
    (
        Exact("net class pattern for a class the project does not define dropped"),
        "net_class.pattern_unknown_class",
        Dropped,
    ),
    (
        Exact("net class without a name dropped"),
        "net_class.name_missing",
        Dropped,
    ),
    (
        Exact("net class assignment to a class the project does not define dropped"),
        "net_class.assignment_unknown_class",
        Dropped,
    ),
    (
        Exact("footprint without a readable position dropped"),
        "footprint.position_missing",
        Dropped,
    ),
    (
        Exact("footprint text without a readable position dropped"),
        "footprint.text_position_missing",
        Dropped,
    ),
    (
        Exact("footprint polygon corner that does not read dropped"),
        "footprint.polygon_corner_unreadable",
        Dropped,
    ),
    (
        Exact("footprint arc in the old angle form dropped"),
        "footprint.arc_old_form",
        Dropped,
    ),
    (
        Exact("footprint 3D model dropped; the board file has no place for it"),
        "footprint.model_dropped",
        Dropped,
    ),
    (
        Exact("hidden footprint text dropped; a drawing has no hidden state"),
        "footprint.hidden_text_dropped",
        Dropped,
    ),
    (
        Exact("board text without a readable position dropped"),
        "board.text_position_missing",
        Dropped,
    ),
    (
        Exact("board polygon corner that does not read dropped"),
        "board.polygon_corner_unreadable",
        Dropped,
    ),
    (
        Exact("board edge does not close; no outline written"),
        "board.edge_open",
        Dropped,
    ),
    (
        Exact("board drawing without its points dropped"),
        "board.drawing_points_missing",
        Dropped,
    ),
    (
        Exact("field position that does not read dropped; the field is kept"),
        "field.position_unreadable",
        Approximated,
    ),
    (
        Exact("source-tool metadata field dropped"),
        "field.source_metadata_dropped",
        Dropped,
    ),
    (
        Exact("imported-library value field dropped"),
        "field.imported_value_dropped",
        Dropped,
    ),
    (
        Exact("imported-library value field used as the empty Value"),
        "field.imported_value_used",
        Info,
    ),
    (
        Exact("repeated uuid replaced with a fresh one"),
        "id.repeated",
        Info,
    ),
    (
        Exact("number that is not finite written as 0"),
        "number.non_finite",
        Approximated,
    ),
    (
        Affix("pad shape ", " written as a rectangle"),
        "pad.shape_approximated",
        Approximated,
    ),
    (
        Affix("pad layer ", " dropped"),
        "pad.layer_dropped",
        Dropped,
    ),
    (
        Affix("custom pad primitive ", " dropped"),
        "pad.custom_primitive_dropped",
        Dropped,
    ),
    (
        Affix("footprint drawing on layer ", " dropped"),
        "footprint.drawing_layer_dropped",
        Dropped,
    ),
    (
        Affix("board drawing on layer ", " dropped"),
        "board.drawing_layer_dropped",
        Dropped,
    ),
    (
        Affix("board text on layer ", " dropped"),
        "board.text_layer_dropped",
        Dropped,
    ),
    (
        Affix("footprint ", " dropped"),
        "footprint.item_dropped",
        Dropped,
    ),
    (Affix("board ", " dropped"), "board.item_dropped", Dropped),
    (
        Affix("stackup copper layer ", " has no Signex layer; dropped"),
        "stackup.layer_unmapped",
        Dropped,
    ),
    (
        Affix("stackup layer type ", " dropped"),
        "stackup.type_dropped",
        Dropped,
    ),
    (
        Affix("copper layer ", " past 32 layers dropped"),
        "layer.copper_past_limit",
        Dropped,
    ),
    (
        Affix("layer ", " has no Signex layer; its items are dropped"),
        "layer.unmapped",
        Dropped,
    ),
    (
        Affix("", " without a positive width dropped"),
        "item.width_missing",
        Dropped,
    ),
];

/// The code used for a message the catalogue does not list.
pub const UNCLASSIFIED: &str = "board.unclassified";

/// `(code, severity, detail)` for a report message. An unlisted message is
/// `board.unclassified` / `approximated`, with the text in `detail`.
pub fn classify(message: &str) -> (&'static str, Severity, Value) {
    // `{what} on non-copper layer {name} dropped` has two variable parts.
    if let Some((what, rest)) = message.split_once(" on non-copper layer ")
        && let Some(name) = rest.strip_suffix(" dropped")
    {
        return (
            "copper_item.layer_not_copper",
            Dropped,
            json!({ "what": what, "layer": name }),
        );
    }
    for (pattern, code, severity) in CATALOGUE {
        match pattern {
            Exact(text) if *text == message => return (code, *severity, Value::Null),
            Affix(prefix, suffix) => {
                if let Some(middle) = message
                    .strip_prefix(prefix)
                    .and_then(|m| m.strip_suffix(suffix))
                    && !middle.is_empty()
                {
                    return (code, *severity, json!({ "what": middle }));
                }
            }
            Exact(_) => {}
        }
    }
    (UNCLASSIFIED, Approximated, json!({ "text": message }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_affix_messages_get_their_codes() {
        let (code, severity, _) = classify("via without a readable position dropped");
        assert_eq!((code, severity), ("via.position_missing", Dropped));
        let (code, _, detail) = classify("pad shape bogus written as a rectangle");
        assert_eq!(code, "pad.shape_approximated");
        assert_eq!(detail["what"], "bogus");
        let (code, _, detail) = classify("via on non-copper layer F.Cu dropped");
        assert_eq!(code, "copper_item.layer_not_copper");
        assert_eq!(detail["layer"], "F.Cu");
    }

    #[test]
    fn specific_affixes_win_over_broad_ones() {
        assert_eq!(
            classify("board text on layer X dropped").0,
            "board.text_layer_dropped"
        );
        assert_eq!(classify("board dimension dropped").0, "board.item_dropped");
        assert_eq!(
            classify("footprint dimension dropped").0,
            "footprint.item_dropped"
        );
    }

    #[test]
    fn an_unknown_message_is_unclassified_not_lost() {
        let (code, _, detail) = classify("something new");
        assert_eq!(code, UNCLASSIFIED);
        assert_eq!(detail["text"], "something new");
    }

    #[test]
    fn every_note_literal_in_the_sources_is_classified() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).expect("src") {
            let path = entry.expect("entry").path();
            if path.extension().is_none_or(|e| e != "rs")
                || path.ends_with("message_codes.rs")
                || path.ends_with("footprint_lib.rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read");
            let mut rest = text.as_str();
            while let Some(at) = rest.find(".note(") {
                rest = &rest[at + ".note(".len()..];
                let body = rest.trim_start();
                let body = body.strip_prefix("format!(").map_or(body, str::trim_start);
                let Some(body) = body.strip_prefix('"') else {
                    continue;
                };
                let Some(end) = body.find('"') else { continue };
                let mut message = body[..end].to_string();
                while let (Some(open), Some(close)) = (message.find('{'), message.find('}')) {
                    message.replace_range(open..=close, "X");
                }
                seen += 1;
                assert_ne!(
                    classify(&message).0,
                    UNCLASSIFIED,
                    "{}: {message}",
                    path.display()
                );
            }
        }
        assert!(seen > 40, "only {seen} note literals found");
    }
}
