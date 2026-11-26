// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! KiCad library tables: `sym-lib-table` and `fp-lib-table`.
//!
//! A table is `(sym_lib_table (version N) (lib (name "N") (type "KiCad")
//! (uri "...") (options "...") (descr "...")) ...)` (`fp_lib_table` for
//! footprints), from the KiCad manual's library table description and the
//! table files themselves. A newer file may add `(disabled)` to an entry.
//! The uri may use `${VAR}` variables; `${KIPRJMOD}` is the folder of the
//! table, the others come from the environment or a folder the user names.

use std::path::{Path, PathBuf};

use crate::error::ParseError;
use crate::sexpr::{self, Atom, SExpr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    Symbol,
    Footprint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableEntry {
    /// The library's nickname: what `Lib:Item` names refer to.
    pub name: String,
    /// `KiCad` for a `.kicad_sym` / `.pretty`; `Legacy`, `Database`, ... otherwise.
    pub kind: String,
    pub uri: String,
    pub options: String,
    pub descr: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibTable {
    pub kind: TableKind,
    pub entries: Vec<TableEntry>,
}

pub fn parse_lib_table(text: &str) -> Result<LibTable, ParseError> {
    let root = sexpr::parse(text)?;
    let kind = match root.keyword() {
        Some("sym_lib_table") => TableKind::Symbol,
        Some("fp_lib_table") => TableKind::Footprint,
        other => {
            return Err(ParseError::InvalidValue(format!(
                "not a library table (starts with {})",
                other.unwrap_or("a value")
            )));
        }
    };
    let entries = root
        .find_all("lib")
        .into_iter()
        .map(|lib| {
            let text = |key: &str| {
                lib.find(key)
                    .and_then(SExpr::first_arg)
                    .unwrap_or("")
                    .to_string()
            };
            let disabled = lib.children().iter().any(|c| match c {
                SExpr::Atom(Atom::Raw(w)) => w == "disabled",
                list => list.keyword() == Some("disabled") && list.first_arg() != Some("no"),
            });
            TableEntry {
                name: text("name"),
                kind: text("type"),
                uri: text("uri"),
                options: text("options"),
                descr: text("descr"),
                disabled,
            }
        })
        .collect();
    Ok(LibTable { kind, entries })
}

/// Expand `${VAR}` (and `$(VAR)`) in `uri`. `KIPRJMOD` is `table_dir`; other
/// names come from `env`. The names that did not resolve are returned.
pub fn expand_uri(
    uri: &str,
    table_dir: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut missing = Vec::new();
    let mut rest = uri;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let close = match tail.chars().next() {
            Some('{') => Some('}'),
            Some('(') => Some(')'),
            _ => None,
        };
        let name_end = close.and_then(|c| tail.find(c));
        match name_end {
            Some(end) => {
                let name = &tail[1..end];
                let value = if name == "KIPRJMOD" {
                    Some(table_dir.display().to_string())
                } else {
                    env(name)
                };
                match value {
                    Some(v) => out.push_str(&v),
                    None => {
                        missing.push(name.to_string());
                        out.push_str(&rest[at..at + 1 + end + 1]);
                    }
                }
                rest = &tail[end + 1..];
            }
            None => {
                out.push('$');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    (out, missing)
}

/// Find the library a table entry names: the expanded uri when it exists
/// (relative ones against the table's folder), else, for a uri that starts
/// with an unknown variable, the rest of the path (or just its file name)
/// below each folder of `search`, in order.
pub fn resolve_entry(
    entry: &TableEntry,
    table_dir: &Path,
    search: &[PathBuf],
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, String> {
    let (expanded, missing) = expand_uri(&entry.uri, table_dir, env);
    if missing.is_empty() {
        let path = Path::new(&expanded);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            table_dir.join(path)
        };
        if path.exists() {
            return Ok(path);
        }
    }
    // Below a folder the user gave: what follows the last variable, or the file name.
    let tail = entry
        .uri
        .rsplit_once('}')
        .map_or(entry.uri.as_str(), |(_, t)| t)
        .trim_start_matches(['/', '\\']);
    let base = entry.uri.rsplit(['/', '\\']).next().unwrap_or("");
    for dir in search {
        for candidate in [tail, base] {
            if !candidate.is_empty() && dir.join(candidate).exists() {
                return Ok(dir.join(candidate));
            }
        }
    }
    Err(if missing.is_empty() {
        format!("{expanded} does not exist")
    } else {
        format!(
            "{} needs {}, which is not set; give a folder with --search-path",
            entry.uri,
            missing.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = r#"(sym_lib_table
  (version 7)
  (lib (name "Device")(type "KiCad")(uri "${KICAD8_SYMBOL_DIR}/Device.kicad_sym")(options "")(descr "Passives"))
  (lib (name "Mine")(type "KiCad")(uri "${KIPRJMOD}/mine.kicad_sym")(options "")(descr ""))
  (lib (name "Old")(type "Legacy")(uri "old.lib")(options "")(descr "") (disabled))
)"#;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("snx-table-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    #[test]
    fn a_table_lists_its_libraries() {
        let t = parse_lib_table(TABLE).expect("parses");
        assert_eq!(t.kind, TableKind::Symbol);
        let names: Vec<&str> = t.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Device", "Mine", "Old"]);
        assert_eq!(
            (t.entries[0].kind.as_str(), t.entries[0].descr.as_str()),
            ("KiCad", "Passives")
        );
        assert!(t.entries[2].disabled && !t.entries[0].disabled);
        assert!(parse_lib_table("(footprint \"A\")").is_err());
        assert!(parse_lib_table("(sym_lib_table (lib").is_err());
        assert_eq!(
            parse_lib_table("(fp_lib_table)").expect("empty").kind,
            TableKind::Footprint
        );
    }

    #[test]
    fn variables_expand_from_the_table_folder_and_the_environment() {
        let env = |n: &str| (n == "HOME_LIBS").then(|| "/libs".to_string());
        let (s, missing) = expand_uri(
            "${KIPRJMOD}/a/${HOME_LIBS}/b$(HOME_LIBS)",
            Path::new("/proj"),
            &env,
        );
        assert_eq!(s, "/proj/a//libs/b/libs");
        assert!(missing.is_empty());
        let (s, missing) = expand_uri("${NOPE}/x $ y", Path::new("/p"), &env);
        assert_eq!(s, "${NOPE}/x $ y");
        assert_eq!(missing, ["NOPE"]);
    }

    #[test]
    fn entries_resolve_by_table_folder_environment_or_a_search_folder() {
        let t = parse_lib_table(TABLE).expect("parses");
        let proj = dir("resolve");
        std::fs::write(proj.join("mine.kicad_sym"), "").expect("f");
        let none = |_: &str| None;
        // ${KIPRJMOD} is the table's folder.
        assert_eq!(
            resolve_entry(&t.entries[1], &proj, &[], &none),
            Ok(proj.join("mine.kicad_sym"))
        );
        // An unset variable is reported, with the way out.
        let err = resolve_entry(&t.entries[0], &proj, &[], &none).expect_err("unset");
        assert!(
            err.contains("KICAD8_SYMBOL_DIR") && err.contains("--search-path"),
            "{err}"
        );
        // A folder the user names stands in for the variable.
        let shared = dir("shared");
        std::fs::write(shared.join("Device.kicad_sym"), "").expect("f");
        assert_eq!(
            resolve_entry(&t.entries[0], &proj, std::slice::from_ref(&shared), &none),
            Ok(shared.join("Device.kicad_sym"))
        );
        // The environment works too.
        let env = |n: &str| (n == "KICAD8_SYMBOL_DIR").then(|| shared.display().to_string());
        assert_eq!(
            resolve_entry(&t.entries[0], &proj, &[], &env),
            Ok(shared.join("Device.kicad_sym"))
        );
        // A path that does not exist is an error naming it.
        assert!(
            resolve_entry(&t.entries[2], &proj, &[], &none)
                .expect_err("missing")
                .contains("old.lib")
        );
    }
}
