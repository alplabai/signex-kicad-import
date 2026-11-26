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

/// The variables KiCad itself sets for its libraries: `KICAD<n>_SYMBOL_DIR`,
/// `KICAD<n>_FOOTPRINT_DIR` and `KICAD<n>_3DMODEL_DIR`, any version `<n>`.
pub fn is_kicad_library_variable(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("KICAD") else {
        return false;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    matches!(
        &rest[digits..],
        "_SYMBOL_DIR" | "_FOOTPRINT_DIR" | "_3DMODEL_DIR"
    )
}

/// Expand `${VAR}` (and `$(VAR)`) in `uri`, for an allowed set of names only:
/// `KIPRJMOD` (the table's folder), the variables the user names explicitly in
/// `named` (`--search-path NAME=DIR`), and KiCad's own library variables read
/// from `env`. Any other name, `HOME` and secrets included, is never looked up
/// and stays as written. The names that did not resolve are returned.
pub fn expand_uri(
    uri: &str,
    table_dir: &Path,
    env: &dyn Fn(&str) -> Option<String>,
    named: &[(String, String)],
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
                } else if let Some((_, dir)) = named.iter().find(|(n, _)| n == name) {
                    Some(dir.clone())
                } else if is_kicad_library_variable(name) {
                    env(name)
                } else {
                    None
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

/// A table entry's library found on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub path: PathBuf,
    /// Set when the file is not where the uri says but was found below a
    /// `--search-path` folder: the uri as written.
    pub substituted_for: Option<String>,
}

/// Find the library a table entry names: the expanded uri when it exists
/// (relative ones against the table's folder). Only when a variable of the uri
/// did not resolve is the rest of the path (or just its file name) looked for
/// below each folder of `search`, in order; that is reported through
/// [`Resolved::substituted_for`]. An error names only the entry's own,
/// unexpanded uri, never a value taken from the environment.
pub fn resolve_entry_noted(
    entry: &TableEntry,
    table_dir: &Path,
    search: &[PathBuf],
    env: &dyn Fn(&str) -> Option<String>,
    named: &[(String, String)],
) -> Result<Resolved, String> {
    let (expanded, missing) = expand_uri(&entry.uri, table_dir, env, named);
    if missing.is_empty() {
        let path = Path::new(&expanded);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            table_dir.join(path)
        };
        if path.exists() {
            return Ok(Resolved {
                path,
                substituted_for: None,
            });
        }
        return Err(format!("{} does not exist", entry.uri));
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
                return Ok(Resolved {
                    path: dir.join(candidate),
                    substituted_for: Some(entry.uri.clone()),
                });
            }
        }
    }
    let names: Vec<String> = missing.iter().map(|n| format!("${{{n}}}")).collect();
    Err(format!(
        "{} uses {}, which is not set or not allowed (KIPRJMOD, KiCad's KICAD<n>_*_DIR, or NAME=DIR given with --search-path); give a folder with --search-path",
        entry.uri,
        names.join(", ")
    ))
}

/// [`resolve_entry_noted`] without the note.
pub fn resolve_entry(
    entry: &TableEntry,
    table_dir: &Path,
    search: &[PathBuf],
    env: &dyn Fn(&str) -> Option<String>,
    named: &[(String, String)],
) -> Result<PathBuf, String> {
    resolve_entry_noted(entry, table_dir, search, env, named).map(|r| r.path)
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
        let env = |n: &str| (n == "KICAD8_SYMBOL_DIR").then(|| "/libs".to_string());
        let named = [("HOME_LIBS".to_string(), "/named".to_string())];
        let (s, missing) = expand_uri(
            "${KIPRJMOD}/a/${HOME_LIBS}/b$(KICAD8_SYMBOL_DIR)",
            Path::new("/proj"),
            &env,
            &named,
        );
        assert_eq!(s, "/proj/a//named/b/libs");
        assert!(missing.is_empty());
        let (s, missing) = expand_uri("${NOPE}/x $ y", Path::new("/p"), &env, &named);
        assert_eq!(s, "${NOPE}/x $ y");
        assert_eq!(missing, ["NOPE"]);
    }

    #[test]
    fn only_allowed_variables_expand_and_errors_never_show_a_looked_up_value() {
        // An environment that holds secrets the table must not reach.
        let env = |n: &str| match n {
            "SECRET_TOKEN" => Some("hunter2".to_string()),
            "HOME" => Some("/home/secret".to_string()),
            "KICAD9_3DMODEL_DIR" => Some("/models".to_string()),
            _ => None,
        };
        let (s, missing) = expand_uri(
            "${HOME}/x/${SECRET_TOKEN}/y/${KICAD9_3DMODEL_DIR}",
            Path::new("/p"),
            &env,
            &[],
        );
        assert_eq!(s, "${HOME}/x/${SECRET_TOKEN}/y//models");
        assert_eq!(missing, ["HOME", "SECRET_TOKEN"]);
        for name in [
            "KICAD8_SYMBOL_DIR",
            "KICAD_FOOTPRINT_DIR",
            "KICAD10_3DMODEL_DIR",
        ] {
            assert!(is_kicad_library_variable(name), "{name}");
        }
        for name in [
            "KICAD8_SECRET",
            "XKICAD8_SYMBOL_DIR",
            "KICAD8_SYMBOL_DIRX",
            "HOME",
            "KICAD",
        ] {
            assert!(!is_kicad_library_variable(name), "{name}");
        }
        // A user-named variable is the explicit way to supply another one; the
        // environment is not consulted for it.
        let named = [("MYLIBS".to_string(), "/mine".to_string())];
        assert_eq!(
            expand_uri("${MYLIBS}/a", Path::new("/p"), &env, &named).0,
            "/mine/a"
        );
        // The error for an entry names the uri as written and the variable, not a value.
        let proj = dir("leak");
        let entry = |uri: &str| TableEntry {
            name: "L".into(),
            kind: "KiCad".into(),
            uri: uri.into(),
            options: String::new(),
            descr: String::new(),
            disabled: false,
        };
        let err = resolve_entry(
            &entry("${SECRET_TOKEN}/lib.kicad_sym"),
            &proj,
            &[],
            &env,
            &[],
        )
        .expect_err("unresolved");
        assert!(
            err.contains("${SECRET_TOKEN}/lib.kicad_sym") && !err.contains("hunter2"),
            "{err}"
        );
        let err = resolve_entry(&entry("${HOME}/lib.kicad_sym"), &proj, &[], &env, &[])
            .expect_err("unresolved");
        assert!(!err.contains("/home/secret"), "{err}");
        // A path that expands from an allowed variable but is missing: the uri as written.
        let err = resolve_entry(
            &entry("${KICAD9_3DMODEL_DIR}/nope.kicad_sym"),
            &proj,
            &[],
            &env,
            &[],
        )
        .expect_err("missing");
        assert!(
            err.contains("${KICAD9_3DMODEL_DIR}/nope.kicad_sym") && !err.contains("/models"),
            "{err}"
        );
    }

    #[test]
    fn the_search_folder_stands_in_only_for_an_unresolved_variable() {
        let proj = dir("subst");
        let shared = dir("subst-shared");
        std::fs::write(shared.join("lib.kicad_sym"), "").expect("f");
        let none = |_: &str| None;
        let entry = |uri: &str| TableEntry {
            name: "L".into(),
            kind: "KiCad".into(),
            uri: uri.into(),
            options: String::new(),
            descr: String::new(),
            disabled: false,
        };
        let search = std::slice::from_ref(&shared);
        // An unresolved variable: found below the folder, and said so.
        let r = resolve_entry_noted(&entry("${NOPE}/lib.kicad_sym"), &proj, search, &none, &[])
            .expect("found");
        assert_eq!(r.path, shared.join("lib.kicad_sym"));
        assert_eq!(r.substituted_for.as_deref(), Some("${NOPE}/lib.kicad_sym"));
        // A plain path that is missing is not looked for elsewhere.
        assert!(
            resolve_entry_noted(&entry("gone/lib.kicad_sym"), &proj, search, &none, &[]).is_err()
        );
    }

    #[test]
    fn entries_resolve_by_table_folder_environment_or_a_search_folder() {
        let t = parse_lib_table(TABLE).expect("parses");
        let proj = dir("resolve");
        std::fs::write(proj.join("mine.kicad_sym"), "").expect("f");
        let none = |_: &str| None;
        // ${KIPRJMOD} is the table's folder.
        assert_eq!(
            resolve_entry(&t.entries[1], &proj, &[], &none, &[]),
            Ok(proj.join("mine.kicad_sym"))
        );
        // An unset variable is reported, with the way out.
        let err = resolve_entry(&t.entries[0], &proj, &[], &none, &[]).expect_err("unset");
        assert!(
            err.contains("KICAD8_SYMBOL_DIR") && err.contains("--search-path"),
            "{err}"
        );
        // A folder the user names stands in for the variable.
        let shared = dir("shared");
        std::fs::write(shared.join("Device.kicad_sym"), "").expect("f");
        assert_eq!(
            resolve_entry(
                &t.entries[0],
                &proj,
                std::slice::from_ref(&shared),
                &none,
                &[]
            ),
            Ok(shared.join("Device.kicad_sym"))
        );
        // The environment works too.
        let env = |n: &str| (n == "KICAD8_SYMBOL_DIR").then(|| shared.display().to_string());
        assert_eq!(
            resolve_entry(&t.entries[0], &proj, &[], &env, &[]),
            Ok(shared.join("Device.kicad_sym"))
        );
        // A path that do