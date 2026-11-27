// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Canonical text of the library files, from `docs/LIBRARY_FORMAT.md`
//! (the Signex library format, sections 5.2 and 5.3): numbers, strings and
//! the TOML layout the envelopes use. Not used by the board writer.

use std::fmt::Write as _;

/// A number in a TSV cell or a shape token: `0` for zero (also negative
/// zero), else the shortest decimal that reads back, never an exponent.
/// Non-finite input writes `0`.
pub fn cell_number(v: f64) -> String {
    if !v.is_finite() || v == 0.0 {
        return "0".to_string();
    }
    format!("{v}")
}

/// A TOML float: the same digits with `.0` appended to an integral value;
/// negative zero is `-0.0`. Non-finite input writes `0.0`.
pub fn toml_float(v: f64) -> String {
    if !v.is_finite() {
        return "0.0".to_string();
    }
    let s = format!("{v}");
    if v == 0.0 && v.is_sign_negative() {
        return "-0.0".to_string();
    }
    if s.contains('.') { s } else { format!("{s}.0") }
}

/// The 64-bit value of the nearest 32-bit float, as a TOML float.
pub fn toml_f32(v: f32) -> String {
    toml_float(f64::from(v))
}

/// A key is bare when it uses only `A-Z a-z 0-9 _ -`.
pub fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_string()
    } else {
        basic_string(key)
    }
}

fn is_control(c: char) -> bool {
    (c as u32) < 0x20 || c as u32 == 0x7f
}

fn basic_escape(out: &mut String, c: char, keep_newline: bool) {
    match c {
        '\\' => out.push_str("\\\\"),
        '"' => out.push_str("\\\""),
        '\n' if keep_newline => out.push('\n'),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        c if is_control(c) => {
            let _ = write!(out, "\\u{:04X}", c as u32);
        }
        c => out.push(c),
    }
}

fn basic_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        basic_escape(&mut out, c, false);
    }
    out.push('"');
    out
}

/// A TOML string value, in the form the printer the format was fixed with
/// chooses by content (section 5.3).
pub fn toml_string(s: &str) -> String {
    let (quote, double, backslash) = (s.contains('\''), s.contains('"'), s.contains('\\'));
    if s.contains('\n') {
        let mut out = String::from("\"\"\"\n");
        for c in s.chars() {
            basic_escape(&mut out, c, true);
        }
        // A run of quotes at the end would merge with the closing ones.
        out.push_str("\"\"\"");
        return out.replace("\"\"\"\"", "\"\"\\\"");
    }
    if s.chars().any(is_control) {
        return basic_string(s);
    }
    match (quote, double, backslash) {
        (_, false, false) => basic_string(s),
        (false, _, _) => format!("'{s}'"),
        (true, true, false) if !s.contains("\"\"\"") && !s.ends_with('"') => {
            format!("\"\"\"{s}\"\"\"")
        }
        (true, _, true) if !s.contains("'''") && !s.ends_with('\'') => format!("'''{s}'''"),
        _ => basic_string(s),
    }
}

/// A number array, one element per line, indented four spaces, trailing comma.
pub fn float_array(key: &str, values: &[f64]) -> String {
    array(key, values.iter().map(|&v| toml_float(v)))
}

pub fn int_array(key: &str, values: &[u8]) -> String {
    array(key, values.iter().map(|v| v.to_string()))
}

pub fn f32_array(key: &str, values: &[f32]) -> String {
    array(key, values.iter().map(|&v| toml_f32(v)))
}

fn array(key: &str, items: impl Iterator<Item = String>) -> String {
    let mut out = format!("{key} = [\n");
    for item in items {
        let _ = writeln!(out, "    {item},");
    }
    out.push_str("]\n");
    out
}

/// An array of points: the inner arrays keep the printer's shape, their
/// closing brackets at column 0.
pub fn point_array(key: &str, points: &[[f64; 2]]) -> String {
    let mut out = format!("{key} = [\n");
    for p in points {
        let _ = writeln!(
            out,
            "    [\n    {},\n    {},\n],",
            toml_float(p[0]),
            toml_float(p[1])
        );
    }
    out.push_str("]\n");
    out
}

/// Replace what a TSV cell may not hold (control characters, `'''`) and say
/// whether anything was replaced.
pub fn clean_cell(s: &str) -> (String, bool) {
    let mut changed = false;
    let mut out: String = s
        .chars()
        .map(|c| {
            if is_control(c) {
                changed = true;
                ' '
            } else {
                c
            }
        })
        .collect();
    while out.contains("'''") {
        changed = true;
        out = out.replace("'''", "' ' '");
    }
    (out, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_the_spec_forms() {
        assert_eq!(cell_number(-0.0), "0");
        assert_eq!(cell_number(2.0), "2");
        assert_eq!(cell_number(0.0000001), "0.0000001");
        assert_eq!(cell_number(1e21), "1000000000000000000000");
        assert_eq!(cell_number(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(toml_float(0.0), "0.0");
        assert_eq!(toml_float(-0.0), "-0.0");
        assert_eq!(toml_float(90.0), "90.0");
        assert_eq!(toml_float(1e21), "1000000000000000000000.0");
        assert_eq!(toml_f32(0.6), "0.6000000238418579");
    }

    #[test]
    fn strings_pick_the_form_the_spec_lists() {
        assert_eq!(toml_string("plain"), "\"plain\"");
        assert_eq!(toml_string("a'b"), "\"a'b\"");
        assert_eq!(toml_string("a\"b"), "'a\"b'");
        assert_eq!(toml_string("a\\b"), "'a\\b'");
        assert_eq!(toml_string("a'b\"c"), "\"\"\"a'b\"c\"\"\"");
        assert_eq!(toml_string("a\\b'c"), "'''a\\b'c'''");
        assert_eq!(toml_string("a\nb"), "\"\"\"\na\nb\"\"\"");
        assert_eq!(toml_string("tab\tx"), "\"tab\\tx\"");
        assert_eq!(toml_string("x\u{7f}y"), "\"x\\u007Fy\"");
    }

    #[test]
    fn cells_lose_control_characters_and_triple_quotes() {
        assert_eq!(clean_cell("a\tb"), ("a b".to_string(), true));
        assert_eq!(clean_cell("ok"), ("ok".to_string(), false));
        assert!(!clean_cell("a'''b").0.contains("'''"));
    }
}
