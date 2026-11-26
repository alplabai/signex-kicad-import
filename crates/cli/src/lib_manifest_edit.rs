// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Updating an existing `.snxlib` the way Signex edits one (library format
//! section 1, "Unknown keys"): the text that was read is edited, so everything
//! the converter does not own survives byte for byte. The converter owns the
//! `[library]` keys `name`, `license`, `source`, `converted_by` and
//! `requires`, and the row lines it wrote itself (its ledgered row ids).
//! Kept as they are: the description and unknown keys of `[library]`, `[mode]`,
//! `[workflow]`, `[users]` and its tables, every class (user labels too), every
//! table of the user's, rows the user added, column types, prices of rows that
//! still exist, the `alternate_symbols` column, and unknown keys anywhere.

use std::collections::HashSet;

use snx_writer::LibraryMeta;
use snx_writer::part_table::Class;

pub struct Edit<'a> {
    pub name: &'a str,
    pub meta: &'a LibraryMeta,
    /// The rows this run built, by table name: `(row_id, line)`.
    pub generated: Vec<(String, Vec<(String, String)>)>,
    /// Row ids the converter wrote earlier and that are to go (every ledgered
    /// row but those kept because their library failed to read).
    pub remove_ids: &'a HashSet<String>,
    pub classes: &'a [Class],
}

#[derive(Clone)]
struct Block {
    header: String,
    lead_blanks: usize,
    lines: Vec<String>,
}

enum Part {
    Plain(Block),
    /// A table's blocks: the table itself, then its column types and prices.
    Table(String, Vec<Block>),
}

fn split(text: &str) -> Vec<Block> {
    let mut blocks = vec![Block {
        header: String::new(),
        lead_blanks: 0,
        lines: Vec::new(),
    }];
    let mut in_string = false;
    for line in text.split('\n') {
        if !in_string && line.starts_with('[') {
            let cur = blocks.last_mut().expect("a block");
            let mut blanks = 0;
            while cur.lines.last().is_some_and(String::is_empty) {
                cur.lines.pop();
                blanks += 1;
            }
            blocks.push(Block {
                header: line.to_string(),
                lead_blanks: blanks,
                lines: Vec::new(),
            });
            continue;
        }
        if line.matches("'''").count() % 2 == 1 {
            in_string = !in_string;
        }
        blocks
            .last_mut()
            .expect("a block")
            .lines
            .push(line.to_string());
    }
    let last = blocks.last_mut().expect("a block");
    while last.lines.last().is_some_and(String::is_empty) {
        last.lines.pop();
    }
    blocks
}

fn join(blocks: &[Block]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for b in blocks {
        if !b.header.is_empty() {
            out.extend(std::iter::repeat_n("", b.lead_blanks));
            out.push(&b.header);
        }
        out.extend(b.lines.iter().map(String::as_str));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// `(table name, rest of the header)` of a `[tables.…]` header.
fn table_header(header: &str) -> Option<(String, String)> {
    let inner = header.strip_prefix("[tables.")?.strip_suffix(']')?;
    if let Some(quoted) = inner.strip_prefix('"') {
        let mut name = String::new();
        let mut chars = quoted.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    if let Some((_, n)) = chars.next() {
                        name.push(match n {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    }
                }
                '"' => return Some((name, quoted[i + 1..].to_string())),
                c => name.push(c),
            }
        }
        None
    } else {
        let (name, rest) = inner.split_once('.').map_or((inner, ""), |(n, r)| (n, r));
        Some((
            name.to_string(),
            if rest.is_empty() {
                String::new()
            } else {
                format!(".{rest}")
            },
        ))
    }
}

fn key_text(name: &str) -> String {
    snx_writer::canon::toml_key(name)
}

fn value_of(line: &str, key: &str) -> Option<String> {
    let rest = line
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    let v = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| rest.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))?;
    Some(v.to_string())
}

fn edit_library(block: &mut Block, name: &str, meta: &LibraryMeta) {
    const OWNED: [&str; 5] = ["name", "license", "source", "converted_by", "requires"];
    let own = |line: &str| {
        OWNED.iter().any(|k| {
            line.strip_prefix(k)
                .is_some_and(|r| r.trim_start().starts_with('='))
        })
    };
    let mut lines: Vec<String> = Vec::new();
    let mut i = 0;
    let mut name_at = None;
    let mut description_at = None;
    while i < block.lines.len() {
        let line = &block.lines[i];
        if line
            .strip_prefix("description")
            .is_some_and(|r| r.trim_start().starts_with('='))
        {
            description_at = Some(lines.len());
        }
        if own(line) {
            if line.starts_with("name") {
                name_at = Some(lines.len());
            }
            // A multi-line array runs to its closing bracket.
            if line.trim_end().ends_with('[') {
                while i < block.lines.len() && block.lines[i].trim() != "]" {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        lines.push(line.clone());
        i += 1;
    }
    let name_line = format!("name = {}", snx_writer::canon::toml_string(name));
    let at = match name_at {
        Some(at) => {
            lines.insert(at, name_line);
            at
        }
        None => {
            lines.insert(0, name_line);
            0
        }
    };
    let after = description_at.map_or(at, |d| if d >= at { d + 1 } else { d });
    let owned = snx_writer::library_block_lines(meta);
    for (k, l) in owned.into_iter().enumerate() {
        lines.insert(after + 1 + k, l);
    }
    block.lines = lines;
}

fn class_block(c: &Class) -> Block {
    Block {
        header: "[[classes]]".into(),
        lead_blanks: 1,
        lines: vec![
            format!("key = {}", snx_writer::canon::toml_string(&c.key)),
            format!("label = {}", snx_writer::canon::toml_string(&c.label)),
        ],
    }
}

const OUR_HEADER: &str = "row_id\tinternal_pn\tclass\tdatasheet\t";

/// Edit `old` (a manifest this converter or Signex wrote). Returns the new text
/// and notes about what could not be done.
pub fn edit_manifest(old: &str, e: &Edit<'_>) -> Result<(String, Vec<String>), String> {
    let blocks = split(old);
    if blocks.iter().any(|b| {
        b.header == "[mode]"
            && b.lines
                .iter()
                .any(|l| value_of(l, "kind").as_deref() == Some("database"))
    }) {
        return Err(
            "the library is a database library; the converter updates local libraries only".into(),
        );
    }
    let mut notes: Vec<String> = Vec::new();
    // Group the blocks.
    let mut parts: Vec<Part> = Vec::new();
    for mut b in blocks {
        if b.header == "[library]" {
            edit_library(&mut b, e.name, e.meta);
        }
        match table_header(&b.header) {
            Some((name, rest)) if rest.is_empty() => parts.push(Part::Table(name, vec![b])),
            Some((name, _)) => match parts.iter_mut().rev().find_map(|p| match p {
                Part::Table(n, bs) if *n == name => Some(bs),
                _ => None,
            }) {
                Some(bs) => bs.push(b),
                None => parts.push(Part::Table(name, vec![b])),
            },
            None => parts.push(Part::Plain(b)),
        }
    }
    // The tables.
    let mut done: HashSet<String> = HashSet::new();
    for part in &mut parts {
        let Part::Table(name, blocks) = part else {
            continue;
        };
        let generated = e.generated.iter().find(|(n, _)| n == name);
        let main = &mut blocks[0];
        let Some(open) = main.lines.iter().position(|l| l.starts_with("tsv = '''")) else {
            continue;
        };
        let Some(close) = main.lines[open + 1..]
            .iter()
            .position(|l| l == "'''")
            .map(|c| c + open + 1)
        else {
            continue;
        };
        let header = main.lines.get(open + 1).cloned().unwrap_or_default();
        let ours = close > open + 1
            && header.starts_with(OUR_HEADER)
            && matches!(header.split('\t').count(), 22 | 23);
        if !ours {
            if generated.is_some() {
                notes.push(name.clone());
            }
            continue;
        }
        let wide = header.split('\t').count() == 23;
        let mut rows: Vec<String> = main.lines[open + 2..close].to_vec();
        let before = rows.len();
        rows.retain(|r| !e.remove_ids.contains(r.split('\t').next().unwrap_or("")));
        let removed = before - rows.len();
        if let Some((_, lines)) = generated {
            done.insert(name.clone());
            rows.extend(
                lines
                    .iter()
                    .map(|(_, l)| if wide { format!("{l}\t") } else { l.clone() }),
            );
        }
        if generated.is_some() || removed > 0 {
            rows.sort_by(|a, b| {
                a.split('\t')
                    .next()
                    .unwrap_or("")
                    .as_bytes()
                    .cmp(b.split('\t').next().unwrap_or("").as_bytes())
            });
        }
        let mut lines = main.lines[..=open].to_vec();
        lines.push(header);
        lines.extend(rows.iter().cloned());
        lines.extend(main.lines[close..].iter().cloned());
        main.lines = lines;
        let ids: HashSet<&str> = rows
            .iter()
            .map(|r| r.split('\t').next().unwrap_or(""))
            .collect();
        let has_types = blocks.iter().any(|b| b.header.ends_with(".column_types]"));
        // A price record goes only when its row is gone (section 2.6).
        blocks.retain(|b| match table_header(&b.header) {
            Some((_, rest)) if rest.starts_with(".pricing.") => {
                let id = rest[".pricing.".len()..]
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim_matches('"');
                ids.contains(id)
            }
            _ => true,
        });
        if rows.is_empty() && removed > 0 && !has_types {
            blocks.clear();
        }
    }
    parts.retain(|p| !matches!(p, Part::Table(_, bs) if bs.is_empty()));
    // New tables, in byte order of their names among the existing ones.
    for (name, lines) in &e.generated {
        if done.contains(name)
            || parts
                .iter()
                .any(|p| matches!(p, Part::Table(n, _) if n == name))
        {
            continue;
        }
        let mut body = vec![
            "tsv = '''".to_string(),
            snx_writer::part_table::header_line(),
        ];
        body.extend(lines.iter().map(|(_, l)| l.clone()));
        body.push("'''".into());
        let block = Block {
            header: format!("[tables.{}]", key_text(name)),
            lead_blanks: 1,
            lines: body,
        };
        let at = parts
            .iter()
            .position(|p| matches!(p, Part::Table(n, _) if n.as_bytes() > name.as_bytes()))
            .or_else(|| {
                parts
                    .iter()
                    .rposition(|p| matches!(p, Part::Table(..)))
                    .map(|i| i + 1)
            })
            .unwrap_or(parts.len());
        parts.insert(at, Part::Table(name.clone(), vec![block]));
    }
    // New classes after the last existing one.
    let have: HashSet<String> = parts
        .iter()
        .filter_map(|p| match p {
            Part::Plain(b) if b.header == "[[classes]]" => {
                b.lines.iter().find_map(|l| value_of(l, "key"))
            }
            _ => None,
        })
        .collect();
    let at = parts
        .iter()
        .rposition(|p| matches!(p, Part::Plain(b) if b.header == "[[classes]]"))
        .map(|i| i + 1)
        .or_else(|| parts.iter().position(|p| matches!(p, Part::Table(..))))
        .unwrap_or(parts.len());
    for (n, c) in e
        .classes
        .iter()
        .filter(|c| !have.contains(&c.key))
        .enumerate()
    {
        parts.insert(at + n, Part::Plain(class_block(c)));
    }
    // `classes = []` and `[[classes]]` tables cannot stand together.
    if !e.classes.iter().all(|c| have.contains(&c.key)) || !have.is_empty() {
        for p in &mut parts {
            if let Part::Plain(b) = p
                && b.header.is_empty()
            {
                b.lines.retain(|l| l.trim() != "classes = []");
            }
        }
    }
    let flat: Vec<Block> = parts
        .into_iter()
        .flat_map(|p| match p {
            Part::Plain(b) => vec![b],
            Part::Table(_, bs) => bs,
        })
        .collect();
    Ok((join(&flat), notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW_A: &str = "00000000-0000-4000-8000-00000000000a\tparts:R\tresistor\t{}\tReleased\t{}\t\t\t{}\t[]\t[]\t{}\t[]\tc\tu\th\t1.0.0\ttrue\t1.0.0\t\t\td";
    const USER: &str = "00000000-0000-4000-8000-0000000000b0\tR-10K\tresistor\t{}\tDraft\t{}\t\t\t{}\t[]\t[]\t{}\t[]\tc\tu\th\t0.0.1\tfalse\t1.0.0\t\t\td";

    fn old() -> String {
        format!(
            "format = \"snxlib/2\"\nlibrary_id = \"x\"\n\n[library]\nname = \"Old\"\ndescription = \"Mine, hand edited\"\nlicense = \"MIT\"\nrequires = [\n    \"a\",\n    \"b\",\n]\nfuture_key = 7\n\n[mode]\nkind = \"local_git\"\n\n[workflow]\nmode = \"team\"\nreview_required = true\nreviewers_required = 2\nauto_lifecycle_promote = \"Released\"\n\n[users]\ndefault_role = \"Designer\"\nallow_self_signup = false\n\n[users.\"ada@example.com\"]\ndisplay_name = \"Ada\"\nroles = [\"Librarian\"]\n\n[[classes]]\nkey = \"resistor\"\nlabel = \"Resistors (mine)\"\n\n[[classes]]\nkey = \"custom\"\nlabel = \"Custom\"\n\n[tables.Mine]\ntsv = '''\nrow_id\tinternal_pn\n00000000-0000-4000-8000-0000000000c1\tMINE-1\n'''\n\n[tables.Parts]\ntsv = '''\n{h}\n{ROW_A}\n{USER}\n'''\n\n[tables.Parts.column_types]\nreleased = \"bool\"\n\n[tables.Parts.pricing.00000000-0000-4000-8000-00000000000a.Dist.SKU]\nsku = \"SKU\"\n\n[tables.Parts.pricing.00000000-0000-4000-8000-0000000000b0.Dist.SKU]\nsku = \"SKU\"\n",
            h = snx_writer::part_table::header_line()
        )
    }

    #[test]
    fn only_the_converters_own_parts_change() {
        let old = old();
        let meta = LibraryMeta {
            license: Some("CC-BY-SA-4.0".into()),
            source: Some("src".into()),
            converted_by: Some("conv 1".into()),
            requires: vec!["only".into()],
            ..LibraryMeta::default()
        };
        let remove: HashSet<String> = ["00000000-0000-4000-8000-00000000000a".to_string()].into();
        let new_row = "00000000-0000-4000-8000-0000000000a1\tparts:C\tcapacitor\t{}\tReleased\t{}\t\t\t{}\t[]\t[]\t{}\t[]\tc\tu\th\t1.0.0\ttrue\t1.0.0\t\t\td".to_string();
        let new_table = "00000000-0000-4000-8000-0000000000a2\tz:Q\ttransistor\t{}\tReleased\t{}\t\t\t{}\t[]\t[]\t{}\t[]\tc\tu\th\t1.0.0\ttrue\t1.0.0\t\t\td".to_string();
        let classes = [
            Class {
                key: "capacitor".into(),
                label: "Capacitor".into(),
            },
            Class {
                key: "resistor".into(),
                label: "Resistor".into(),
            },
        ];
        let (text, notes) = edit_manifest(
            &old,
            &Edit {
                name: "New",
                meta: &meta,
                generated: vec![
                    (
                        "Parts".into(),
                        vec![(
                            "00000000-0000-4000-8000-0000000000a1".into(),
                            new_row.clone(),
                        )],
                    ),
                    (
                        "Zed".into(),
                        vec![(
                            "00000000-0000-4000-8000-0000000000a2".into(),
                            new_table.clone(),
                        )],
                    ),
                ],
                remove_ids: &remove,
                classes: &classes,
            },
        )
        .expect("edits");
        assert!(notes.is_empty());
        toml::from_str::<toml::Value>(&text).expect("still valid TOML");
        // Owned parts changed.
        assert!(text.contains("name = \"New\"\ndescription = \"Mine, hand edited\"\nlicense = \"CC-BY-SA-4.0\"\nsource = \"src\"\nconverted_by = \"conv 1\"\nrequires = [\"only\"]\nfuture_key = 7\n"));
        assert!(!text.contains(ROW_A) && text.contains(&new_row) && text.contains(&new_table));
        assert!(
            text.contains("[tables.Zed]")
                && text.find("[tables.Zed]") > text.find("[tables.Parts.pricing")
        );
        assert!(
            text.contains("key = \"capacitor\"\nlabel = \"Capacitor\"")
                && text.matches("[[classes]]").count() == 3
        );
        // Everything else is the old text, byte for byte: cut the owned parts out of both.
        let user_parts = [
            USER,
            "[mode]\nkind = \"local_git\"",
            "[workflow]\nmode = \"team\"\nreview_required = true\nreviewers_required = 2",
            "[users.\"ada@example.com\"]\ndisplay_name = \"Ada\"",
            "label = \"Resistors (mine)\"",
            "key = \"custom\"",
            "[tables.Mine]\ntsv = '''\nrow_id\tinternal_pn\n00000000-0000-4000-8000-0000000000c1\tMINE-1\n'''",
            "[tables.Parts.column_types]\nreleased = \"bool\"",
            "[tables.Parts.pricing.00000000-0000-4000-8000-0000000000b0.Dist.SKU]\nsku = \"SKU\"",
        ];
        for p in user_parts {
            assert!(text.contains(p), "lost: {p}");
        }
        // The price of the removed row went with it (section 2.6); the user row's stayed.
        assert!(!text.contains("pricing.00000000-0000-4000-8000-00000000000a"));
        // Nothing to change: the text is the same.
        let (same, _) = edit_manifest(
            &old,
            &Edit {
                name: "Old",
                meta: &LibraryMeta {
                    license: Some("MIT".into()),
                    requires: vec!["a".into(), "b".into()],
                    ..LibraryMeta::default()
                },
                generated: vec![],
                remove_ids: &HashSet::new(),
                classes: &[],
            },
        )
        .expect("edits");
        assert_eq!(same, old);
    }

    #[test]
    fn a_database_library_is_refused_and_an_emptied_table_goes() {
        let db = "format = \"snxlib/2\"\n\n[mode]\nkind = \"database\"\nurl = \"x\"\n";
        assert!(
            edit_manifest(
                db,
                &Edit {
                    name: "n",
                    meta: &LibraryMeta::default(),
                    generated: vec![],
                    remove_ids: &HashSet::new(),
                    classes: &[]
                }
            )
            .is_err()
        );
        let h = snx_writer::part_table::header_line();
        let only = format!(
            "format = \"snxlib/2\"\n\n[tables.A]\ntsv = '''\n{h}\n{ROW_A}\n'''\n\n[tables.A.pricing.00000000-0000-4000-8000-00000000000a.D.S]\nsku = \"S\"\n"
        );
        let remove: HashSet<String> = ["00000000-0000-4000-8000-00000000000a".to_string()].into();
        let (text, _) = edit_manifest(
            &only,
            &Edit {
                name: "n",
                meta: &LibraryMeta::default(),
                generated: vec![],
                remove_ids: &remove,
                classes: &[],
            },
        )
        .expect("edits");
        assert!(!text.contains("[tables"), "{text}");
    }
}
