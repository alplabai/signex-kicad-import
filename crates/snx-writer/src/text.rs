// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Text primitives of the format: TOML values, nanometre cells and TSV
//! blocks embedded in TOML multi-line strings.

use std::cell::Cell;
use std::fmt::Write as _;

use crate::model::Point;

thread_local! {
    /// How many numbers that are not finite this thread has written as 0
    /// since the last [`take_non_finite`].
    static NON_FINITE: Cell<usize> = const { Cell::new(0) };
}

fn note_non_finite() {
    NON_FINITE.with(|n| n.set(n.get() + 1));
}

/// The count of numbers written as 0 because they were not finite, reset
/// to 0. The writer takes it before and after a file, so the file's count
/// reaches the caller's report.
pub(crate) fn take_non_finite() -> usize {
    NON_FINITE.with(|n| n.replace(0))
}

/// A TOML basic string, quoted.
pub(crate) fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        push_escaped(&mut out, c);
    }
    out.push('"');
    out
}

/// One character of a TOML basic string body.
fn push_escaped(out: &mut String, c: char) {
    match c {
        '\\' => out.push_str("\\\\"),
        '"' => out.push_str("\\\""),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        c if (c as u32) < 0x20 || c as u32 == 0x7f => {
            let _ = write!(out, "\\u{:04X}", c as u32);
        }
        c => out.push(c),
    }
}

/// A TOML float. Non-finite input writes 0 and is counted (see
/// [`take_non_finite`]): the format refuses `nan` and `inf`.
pub(crate) fn toml_float(v: f64) -> String {
    if !v.is_finite() {
        note_non_finite();
        return "0.0".to_string();
    }
    if v == 0.0 {
        return "0.0".to_string();
    }
    // `{:?}` always carries a `.` or an exponent, so TOML reads a float.
    format!("{v:?}")
}

/// An inline `{ x = .., y = .. }` point in millimetres.
pub(crate) fn toml_point(p: Point) -> String {
    format!("{{ x = {}, y = {} }}", toml_float(p.x), toml_float(p.y))
}

/// An inline array of points.
pub(crate) fn toml_points(points: &[Point]) -> String {
    let items: Vec<String> = points.iter().map(|&p| toml_point(p)).collect();
    format!("[{}]", items.join(", "))
}

/// Millimetres as whole nanometres, half away from zero. Non-finite input
/// writes 0 and is counted.
pub(crate) fn nm(mm: f64) -> i64 {
    if !mm.is_finite() {
        note_non_finite();
        return 0;
    }
    (mm * 1.0e6).round() as i64
}

/// A plain number cell: no trailing `.0` for whole values. Non-finite
/// input writes 0 and is counted.
pub(crate) fn number_cell(v: f64) -> String {
    if !v.is_finite() {
        note_non_finite();
        return "0".to_string();
    }
    if v == 0.0 {
        return "0".to_string();
    }
    if v.fract() == 0.0 && v.abs() < 1.0e15 {
        return format!("{}", v as i64);
    }
    format!("{v}")
}

/// One TSV cell, by the rules in `docs/SNX_FORMAT_NOTES.md` ("Cells").
///
/// A reader splits a row on runs of whitespace and treats a lone `-` as an
/// empty cell, so a value that is empty, is `-`, or holds whitespace, a
/// quote or a backslash goes between double quotes. Between quotes a quote
/// is written twice and a backslash, line feed, carriage return or tab is
/// written as its backslash escape.
pub(crate) fn tsv_cell(value: &str) -> String {
    let plain = !value.is_empty()
        && value != "-"
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\');
    if plain {
        return value.to_owned();
    }
    let body: String = value.chars().map(escaped_in_quotes).collect();
    format!("\"{body}\"")
}

/// How one character is written between a cell's quotes.
fn escaped_in_quotes(c: char) -> String {
    match c {
        '"' => "\"\"".to_owned(),
        '\\' => "\\\\".to_owned(),
        '\n' => "\\n".to_owned(),
        '\r' => "\\r".to_owned(),
        '\t' => "\\t".to_owned(),
        other => other.to_string(),
    }
}

/// A TSV table as a `[name]` section with its rows in a TOML multi-line
/// basic string. `header` and every row are already encoded cells; columns
/// are padded to line up and parted by two spaces.
pub(crate) fn tsv_section(name: &str, header: &[String], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut body = String::new();
    push_tsv_line(&mut body, header, &widths);
    for row in rows {
        push_tsv_line(&mut body, row, &widths);
    }
    let mut out = format!("\n[{name}]\ncontent = \"\"\"\n");
    for c in body.chars() {
        match c {
            // Newlines and tabs stay literal inside a multi-line string.
            '\n' | '\t' => out.push(c),
            c => push_escaped(&mut out, c),
        }
    }
    out.push_str("\"\"\"\n");
    out
}

fn push_tsv_line(out: &mut String, cells: &[String], widths: &[usize]) {
    let last = cells.len().saturating_sub(1);
    for (i, cell) in cells.iter().enumerate() {
        out.push_str(cell);
        if i < last {
            let pad = widths[i].saturating_sub(cell.chars().count()) + 2;
            out.extend(std::iter::repeat_n(' ', pad));
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_quote_only_when_needed() {
        assert_eq!(tsv_cell(""), "\"\"");
        assert_eq!(tsv_cell("-"), "\"-\"");
        assert_eq!(tsv_cell("GND"), "GND");
        assert_eq!(tsv_cell("a b"), "\"a b\"");
        assert_eq!(tsv_cell("a\"b"), "\"a\"\"b\"");
        assert_eq!(tsv_cell("C:\\x"), "\"C:\\\\x\"");
        assert_eq!(tsv_cell("l1\nl2"), "\"l1\\nl2\"");
    }

    #[test]
    fn floats_always_read_as_toml_floats() {
        assert_eq!(toml_float(1.0), "1.0");
        assert_eq!(toml_float(0.0), "0.0");
        assert_eq!(toml_float(-0.25), "-0.25");
    }

    #[test]
    fn numbers_that_are_not_finite_write_0_and_are_counted() {
        take_non_finite();
        assert_eq!(toml_float(f64::NAN), "0.0");
        assert_eq!(nm(f64::INFINITY), 0);
        assert_eq!(number_cell(f64::NEG_INFINITY), "0");
        assert_eq!(toml_float(0.0), "0.0");
        assert_eq!(take_non_finite(), 3);
        assert_eq!(take_non_finite(), 0);
    }

    #[test]
    fn nanometres_round_half_away_from_zero() {
        assert_eq!(nm(1.0), 1_000_000);
        assert_eq!(nm(-0.0000005), -1);
        assert_eq!(nm(0.1234564), 123_456);
    }

    #[test]
    fn number_cells_drop_a_whole_fraction() {
        assert_eq!(number_cell(90.0), "90");
        assert_eq!(number_cell(-12.5), "-12.5");
        assert_eq!(number_cell(0.0), "0");
    }

    #[test]
    fn a_section_survives_quotes_and_backslashes() {
        let header = vec!["uuid".to_string(), "name".to_string()];
        let rows = vec![vec!["x".to_string(), tsv_cell("a\"\"\"b\\c")]];
        let text = tsv_section("t", &header, &rows);
        let value: toml::Value = toml::from_str(&text).expect("valid TOML");
        let content = value["t"]["content"].as_str().expect("a string");
        assert_eq!(content, "uuid  name\nx     \"a\"\"\"\"\"\"b\\\\c\"\n");
    }
}
