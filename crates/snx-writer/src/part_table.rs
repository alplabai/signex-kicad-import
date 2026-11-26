// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Part tables and classes of the `.snxlib` (`LIBRARY_FORMAT.md` sections
//! 2.2 to 2.4 and 6): the row model, the tab-separated block, the compact
//! JSON cells and the `content_hash` of a row.

use std::fmt::Write as _;

use serde_json::to_string as json_string;
use sha2::{Digest, Sha256};

use crate::canon::{clean_cell, toml_key, toml_string};

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub key: String,
    pub label: String,
}

/// A reference to a primitive of a library.
#[derive(Debug, Clone, PartialEq)]
pub struct Ref {
    pub library_id: String,
    pub uuid: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Document {
    Url(String),
    HashPinned { hash: String, filename: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelledDocument {
    pub label: String,
    pub document: Document,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FootprintBinding {
    pub footprint: Ref,
    /// Empty for none; omitted from the cell then.
    pub version: String,
    pub pin_map_overrides: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mpn {
    pub manufacturer: String,
    pub mpn: String,
    /// `Primary`, `Approved`, `Conditional` or `Disqualified`.
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Supply {
    pub distributor: String,
    pub sku: String,
    pub url: Option<String>,
    pub moq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Text(String),
    Number(f64),
    Bool(bool),
    Measurement { value: f64, unit: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternateSymbol {
    pub symbol: Ref,
    pub symbol_version: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartRow {
    pub row_id: String,
    pub internal_pn: String,
    pub class: String,
    /// The primary document first; empty for none.
    pub datasheets: Vec<LabelledDocument>,
    /// `Draft`, `InReview`, `Released`, `Deprecated` or `Obsolete`.
    pub state: String,
    pub symbol: Ref,
    /// The default footprint, and the alternates after it.
    pub footprint: Option<Ref>,
    pub alternate_footprints: Vec<FootprintBinding>,
    pub sim: Option<Ref>,
    pub primary_mpn: Mpn,
    pub alternates: Vec<Mpn>,
    pub supply: Vec<Supply>,
    /// Written sorted by key.
    pub parameters: Vec<(String, Param)>,
    pub pin_map_overrides: Vec<(String, String)>,
    pub created: String,
    pub updated: String,
    pub version: String,
    pub released: bool,
    pub symbol_version: String,
    pub footprint_version: String,
    pub sim_version: String,
    pub description: String,
    pub alternate_symbols: Vec<AlternateSymbol>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub name: String,
    pub rows: Vec<PartRow>,
    /// Rows kept as an earlier run wrote them: `(row_id, the line of cells)`.
    /// An update keeps the bytes of a row whose content did not change.
    pub kept: Vec<(String, String)>,
}

/// A JSON number as section 5.2 gives it: shortest round trip, integral
/// values with `.0`, plain decimal from 1e-5 up to below 1e16, else an
/// exponent with a sign.
pub fn json_number(v: f64) -> String {
    if !v.is_finite() {
        return "0.0".to_string();
    }
    let abs = v.abs();
    if v == 0.0 || (1e-5..1e16).contains(&abs) {
        let s = format!("{v}");
        return if s.contains('.') { s } else { format!("{s}.0") };
    }
    let s = format!("{v:e}");
    match s.split_once('e') {
        Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
        _ => s,
    }
}

fn q(s: &str) -> String {
    json_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn ref_json(r: &Ref) -> String {
    format!(
        "{{\"library_id\":{},\"uuid\":{}}}",
        q(&r.library_id),
        q(&r.uuid)
    )
}

fn doc_fields(d: &Document) -> String {
    match d {
        Document::Url(u) => format!("\"kind\":\"url\",\"url\":{}", q(u)),
        Document::HashPinned { hash, filename } => {
            format!(
                "\"kind\":\"hash_pinned\",\"hash\":{},\"filename\":{}",
                q(hash),
                q(filename)
            )
        }
    }
}

/// A document in the element form of the array cell: `label` first, omitted
/// when empty.
fn doc_element(d: &LabelledDocument) -> String {
    if d.label.is_empty() {
        format!("{{{}}}", doc_fields(&d.document))
    } else {
        format!("{{\"label\":{},{}}}", q(&d.label), doc_fields(&d.document))
    }
}

fn empty_doc() -> String {
    "{\"kind\":\"url\",\"url\":\"\"}".to_string()
}

fn datasheet_cell(docs: &[LabelledDocument]) -> String {
    match docs {
        [] => empty_doc(),
        [one] if one.label.is_empty() => format!("{{{}}}", doc_fields(&one.document)),
        many => format!(
            "[{}]",
            many.iter().map(doc_element).collect::<Vec<_>>().join(",")
        ),
    }
}

fn pin_map_json(map: &[(String, String)]) -> String {
    let items: Vec<String> = map
        .iter()
        .map(|(s, f)| {
            format!(
                "{{\"symbol_pin_number\":{},\"footprint_pad_number\":{}}}",
                q(s),
                q(f)
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// A binding as an array element: `version` and `pin_map_overrides` omitted when empty.
fn binding_json(b: &FootprintBinding) -> String {
    let mut o = format!("{{\"footprint\":{}", ref_json(&b.footprint));
    if !b.version.is_empty() {
        let _ = write!(o, ",\"version\":{}", q(&b.version));
    }
    if !b.pin_map_overrides.is_empty() {
        let _ = write!(
            o,
            ",\"pin_map_overrides\":{}",
            pin_map_json(&b.pin_map_overrides)
        );
    }
    o.push('}');
    o
}

fn mpn_json(m: &Mpn) -> String {
    format!(
        "{{\"manufacturer\":{},\"mpn\":{},\"status\":{},\"notes\":{}}}",
        q(&m.manufacturer),
        q(&m.mpn),
        q(&m.status),
        m.notes.as_deref().map_or_else(|| "null".to_string(), q)
    )
}

fn supply_json(s: &Supply) -> String {
    format!(
        "{{\"distributor\":{},\"sku\":{},\"url\":{},\"moq\":{}}}",
        q(&s.distributor),
        q(&s.sku),
        s.url.as_deref().map_or_else(|| "null".to_string(), q),
        s.moq.map_or_else(|| "null".to_string(), |m| m.to_string())
    )
}

fn param_json(p: &Param) -> String {
    match p {
        Param::Text(t) => format!("{{\"kind\":\"Text\",\"value\":{}}}", q(t)),
        Param::Number(n) => format!("{{\"kind\":\"Number\",\"value\":{}}}", json_number(*n)),
        Param::Bool(b) => format!("{{\"kind\":\"Bool\",\"value\":{b}}}"),
        Param::Measurement { value, unit } => format!(
            "{{\"kind\":\"Measurement\",\"value\":{{\"value\":{},\"unit\":{}}}}}",
            json_number(*value),
            q(unit)
        ),
    }
}

fn parameters_json(params: &[(String, Param)]) -> String {
    let mut sorted: Vec<&(String, Param)> = params.iter().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let items: Vec<String> = sorted
        .iter()
        .map(|(k, p)| format!("{}:{}", q(k), param_json(p)))
        .collect();
    format!("{{{}}}", items.join(","))
}

fn alt_symbols_json(alts: &[AlternateSymbol]) -> String {
    let items: Vec<String> = alts
        .iter()
        .map(|a| {
            format!(
                "{{\"symbol_ref\":{},\"symbol_version\":{},\"label\":{}}}",
                ref_json(&a.symbol),
                q(&a.symbol_version),
                q(&a.label)
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn array<T>(items: &[T], f: impl Fn(&T) -> String) -> String {
    format!("[{}]", items.iter().map(f).collect::<Vec<_>>().join(","))
}

fn footprint_cell(row: &PartRow) -> String {
    match (&row.footprint, row.alternate_footprints.is_empty()) {
        (None, _) => String::new(),
        (Some(r), true) => ref_json(r),
        (Some(r), false) => {
            let default = FootprintBinding {
                footprint: r.clone(),
                version: String::new(),
                pin_map_overrides: vec![],
            };
            let mut items = vec![binding_json(&default)];
            items.extend(row.alternate_footprints.iter().map(binding_json));
            format!("[{}]", items.join(","))
        }
    }
}

/// The canonical view the `content_hash` is computed over (section 6).
pub fn hash_document(row: &PartRow) -> String {
    let mut o = String::new();
    let _ = write!(
        o,
        "{{\"row_id\":{},\"internal_pn\":{},\"class\":{},\"state\":{}",
        q(&row.row_id),
        q(&row.internal_pn),
        q(&row.class),
        q(&row.state)
    );
    let primary = row.datasheets.first();
    let _ = write!(
        o,
        ",\"datasheet\":{}",
        primary.map_or_else(empty_doc, |d| format!("{{{}}}", doc_fields(&d.document)))
    );
    if let Some(d) = primary.filter(|d| !d.label.is_empty()) {
        let _ = write!(o, ",\"datasheet_label\":{}", q(&d.label));
    }
    if row.datasheets.len() > 1 {
        let _ = write!(
            o,
            ",\"more_datasheets\":{}",
            array(&row.datasheets[1..], doc_element)
        );
    }
    if !row.description.is_empty() {
        let _ = write!(o, ",\"description\":{}", q(&row.description));
    }
    let _ = write!(o, ",\"symbol_ref\":{}", ref_json(&row.symbol));
    let _ = write!(
        o,
        ",\"footprint_ref\":{}",
        row.footprint
            .as_ref()
            .map_or_else(|| "null".to_string(), ref_json)
    );
    if !row.alternate_footprints.is_empty() {
        let _ = write!(
            o,
            ",\"alternate_footprints\":{}",
            array(&row.alternate_footprints, binding_json)
        );
    }
    let _ = write!(
        o,
        ",\"sim_ref\":{}",
        row.sim
            .as_ref()
            .map_or_else(|| "null".to_string(), ref_json)
    );
    let _ = write!(
        o,
        ",\"pin_map_overrides\":{}",
        pin_map_json(&row.pin_map_overrides)
    );
    let _ = write!(o, ",\"primary_mpn\":{}", mpn_json(&row.primary_mpn));
    let _ = write!(o, ",\"alternates\":{}", array(&row.alternates, mpn_json));
    if !row.alternate_symbols.is_empty() {
        let _ = write!(
            o,
            ",\"alternate_symbols\":{}",
            alt_symbols_json(&row.alternate_symbols)
        );
    }
    let _ = write!(o, ",\"supply\":{}", array(&row.supply, supply_json));
    let _ = write!(o, ",\"parameters\":{}", parameters_json(&row.parameters));
    o.push_str(",\"plm\":{\"plm_part_id\":null,\"eco_refs\":[],\"compliance\":{}}}");
    o
}

/// The row's `content_hash`: lower-case hex SHA-256 of [`hash_document`].
pub fn content_hash(row: &PartRow) -> String {
    let digest = Sha256::digest(hash_document(row).as_bytes());
    digest.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

const COLUMNS: [&str; 22] = [
    "row_id",
    "internal_pn",
    "class",
    "datasheet",
    "state",
    "symbol_ref",
    "footprint_ref",
    "sim_ref",
    "primary_mpn",
    "alternates",
    "supply",
    "parameters",
    "pin_map_overrides",
    "created",
    "updated",
    "content_hash",
    "version",
    "released",
    "symbol_version",
    "footprint_version",
    "sim_version",
    "description",
];

fn cells(row: &PartRow, with_alternates: bool) -> Vec<String> {
    let mut c = vec![
        row.row_id.clone(),
        row.internal_pn.clone(),
        row.class.clone(),
        datasheet_cell(&row.datasheets),
        row.state.clone(),
        ref_json(&row.symbol),
        footprint_cell(row),
        row.sim.as_ref().map_or_else(String::new, ref_json),
        mpn_json(&row.primary_mpn),
        array(&row.alternates, mpn_json),
        array(&row.supply, supply_json),
        parameters_json(&row.parameters),
        pin_map_json(&row.pin_map_overrides),
        row.created.clone(),
        row.updated.clone(),
        content_hash(row),
        row.version.clone(),
        row.released.to_string(),
        row.symbol_version.clone(),
        row.footprint_version.clone(),
        row.sim_version.clone(),
        row.description.clone(),
    ];
    if with_alternates {
        c.push(if row.alternate_symbols.is_empty() {
            String::new()
        } else {
            alt_symbols_json(&row.alternate_symbols)
        });
    }
    c
}

/// The `[[classes]]` blocks, in list order.
pub fn write_classes(classes: &[Class]) -> String {
    let mut o = String::new();
    for c in classes {
        let _ = write!(
            o,
            "\n[[classes]]\nkey = {}\nlabel = {}\n",
            toml_string(&c.key),
            toml_string(&c.label)
        );
    }
    o
}

fn clean(s: &str) -> String {
    clean_cell(s).0
}

fn clean_ref(r: &Ref) -> Ref {
    Ref {
        library_id: clean(&r.library_id),
        uuid: clean(&r.uuid),
    }
}

fn clean_mpn(m: &Mpn) -> Mpn {
    Mpn {
        manufacturer: clean(&m.manufacturer),
        mpn: clean(&m.mpn),
        status: clean(&m.status),
        notes: m.notes.as_deref().map(clean),
    }
}

/// A copy of the row in which no text holds a control character or `'''`.
///
/// The format defines no escape for a table cell (section 2.2): a cell with a
/// tab, a line break, another control character or `'''` is refused on save.
/// JSON cells escape control characters themselves but would still carry a raw
/// `'''`, which ends the `tsv` block. So every text is cleaned (control
/// characters and `'''` become spaces) before a cell, or the `content_hash`
/// over it, is made; callers that want to report the change clean first.
pub fn sanitize_row(row: &PartRow) -> PartRow {
    PartRow {
        row_id: clean(&row.row_id),
        internal_pn: clean(&row.internal_pn),
        class: clean(&row.class),
        datasheets: row
            .datasheets
            .iter()
            .map(|d| LabelledDocument {
                label: clean(&d.label),
                document: match &d.document {
                    Document::Url(u) => Document::Url(clean(u)),
                    Document::HashPinned { hash, filename } => Document::HashPinned {
                        hash: clean(hash),
                        filename: clean(filename),
                    },
                },
            })
            .collect(),
        state: clean(&row.state),
        symbol: clean_ref(&row.symbol),
        footprint: row.footprint.as_ref().map(clean_ref),
        alternate_footprints: row
            .alternate_footprints
            .iter()
            .map(|b| FootprintBinding {
                footprint: clean_ref(&b.footprint),
                version: clean(&b.version),
                pin_map_overrides: b
                    .pin_map_overrides
                    .iter()
                    .map(|(a, b)| (clean(a), clean(b)))
                    .collect(),
            })
            .collect(),
        sim: row.sim.as_ref().map(clean_ref),
        primary_mpn: clean_mpn(&row.primary_mpn),
        alternates: row.alternates.iter().map(clean_mpn).collect(),
        supply: row
            .supply
            .iter()
            .map(|s| Supply {
                distributor: clean(&s.distributor),
                sku: clean(&s.sku),
                url: s.url.as_deref().map(clean),
                moq: s.moq,
            })
            .collect(),
        parameters: row
            .parameters
            .iter()
            .map(|(k, p)| {
                let p = match p {
                    Param::Text(t) => Param::Text(clean(t)),
                    Param::Measurement { value, unit } => Param::Measurement {
                        value: *value,
                        unit: clean(unit),
                    },
                    other => other.clone(),
                };
                (clean(k), p)
            })
            .collect(),
        pin_map_overrides: row
            .pin_map_overrides
            .iter()
            .map(|(a, b)| (clean(a), clean(b)))
            .collect(),
        created: clean(&row.created),
        updated: clean(&row.updated),
        version: clean(&row.version),
        released: row.released,
        symbol_version: clean(&row.symbol_version),
        footprint_version: clean(&row.footprint_version),
        sim_version: clean(&row.sim_version),
        description: clean(&row.description),
        alternate_symbols: row
            .alternate_symbols
            .iter()
            .map(|a| AlternateSymbol {
                symbol: clean_ref(&a.symbol),
                symbol_version: clean(&a.symbol_version),
                label: clean(&a.label),
            })
            .collect(),
    }
}

/// One `[tables.<name>]` block, rows sorted by `row_id` in byte order. Every
/// row goes through [`sanitize_row`].
pub fn write_table(table: &Table) -> String {
    let clean_rows: Vec<PartRow> = table.rows.iter().map(sanitize_row).collect();
    let with_alternates = clean_rows.iter().any(|r| !r.alternate_symbols.is_empty());
    let mut rows: Vec<&PartRow> = clean_rows.iter().collect();
    rows.sort_by(|a, b| a.row_id.as_bytes().cmp(b.row_id.as_bytes()));
    let mut header = COLUMNS.join("\t");
    if with_alternates {
        header.push_str("\talternate_symbols");
    }
    let mut o = format!(
        "\n[tables.{}]\ntsv = '''\n{header}\n",
        toml_key(&table.name)
    );
    let mut lines: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.row_id.clone(), cells(r, with_alternates).join("\t")))
        .collect();
    lines.extend(table.kept.iter().cloned());
    lines.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    for (_, line) in lines {
        o.push_str(&line);
        o.push('\n');
    }
    o.push_str("'''\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cell_of_a_table_holds_a_control_character_or_triple_quotes() {
        let bad = "a'''b\tc\nd\u{7f}e\r";
        let row = PartRow {
            row_id: "id".into(),
            internal_pn: bad.into(),
            class: bad.into(),
            datasheets: vec![LabelledDocument {
                label: bad.into(),
                document: Document::Url(bad.into()),
            }],
            state: "Draft".into(),
            symbol: Ref {
                library_id: bad.into(),
                uuid: bad.into(),
            },
            footprint: None,
            alternate_footprints: vec![],
            sim: None,
            primary_mpn: Mpn {
                manufacturer: bad.into(),
                mpn: bad.into(),
                status: "Primary".into(),
                notes: Some(bad.into()),
            },
            alternates: vec![],
            supply: vec![],
            parameters: vec![(bad.into(), Param::Text(bad.into()))],
            pin_map_overrides: vec![],
            created: String::new(),
            updated: String::new(),
            version: "1.0.0".into(),
            released: true,
            symbol_version: String::new(),
            footprint_version: String::new(),
            sim_version: String::new(),
            description: bad.into(),
            alternate_symbols: vec![],
        };
        let text = write_table(&Table {
            name: "t".into(),
            rows: vec![row],
            kept: vec![],
        });
        let body = text
            .strip_prefix("\n[tables.t]\ntsv = '''\n")
            .and_then(|t| t.strip_suffix("'''\n"))
            .expect("block");
        assert!(!body.contains("'''"), "{body}");
        assert!(
            !body
                .chars()
                .any(|c| (c as u32) < 0x20 && c != '\n' && c != '\t' || c == '\u{7f}')
        );
        assert_eq!(body.lines().count(), 2, "header and one row");
        assert_eq!(body.lines().nth(1).map(|l| l.split('\t').count()), Some(22));
    }

    #[test]
    fn json_numbers_follow_the_spec_forms() {
        assert_eq!(json_number(10000.0), "10000.0");
        assert_eq!(json_number(0.1), "0.1");
        assert_eq!(json_number(0.00001), "0.00001");
        assert_eq!(json_number(1e-6), "1e-6");
        assert_eq!(json_number(1e16), "1e+16");
        assert_eq!(json_number(1.5e16), "1.5e+16");
        assert_eq!(json_number(1_000_000_000_000_000.0), "1000000000000000.0");
    }
}
