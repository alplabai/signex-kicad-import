// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! `import-report.json` and `import-report.md` (contract section 4).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use kicad_parser::Item;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::contract::{CONTRACT_VERSION, Counts, converter_name};

pub struct Source {
    pub path: PathBuf,
    pub sha256: String,
}

pub struct LibraryReport {
    pub started: String,
    pub sources: Vec<Source>,
    pub library_id: Option<String>,
    pub source_counts: Counts,
    pub written_counts: Counts,
    pub items: Vec<Item>,
    /// The library files this report's library holds, and the source library
    /// each came from (an update deletes only files it finds listed here).
    pub files: Vec<String>,
    pub file_libraries: Vec<(String, String)>,
}

impl LibraryReport {
    pub fn to_json(&self) -> Value {
        json!({
            "contract": CONTRACT_VERSION,
            "converter": converter_name(),
            "converter_version": env!("CARGO_PKG_VERSION"),
            "started": self.started,
            "sources": self.sources.iter().map(|s| json!({
                "path": s.path.display().to_string(),
                "sha256": s.sha256,
            })).collect::<Vec<_>>(),
            "library_id": self.library_id,
            "counts": {
                "source": self.source_counts.to_json(),
                "written": self.written_counts.to_json(),
            },
            "items": self.items.iter().map(Item::to_json).collect::<Vec<_>>(),
            "files": self.files,
            "file_libraries": self.file_libraries.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
        })
    }

    /// The same content for a reader: counts, then items by severity.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        let _ = writeln!(md, "# Import report\n");
        let _ = writeln!(
            md,
            "Converter: {} {}  \nContract: {}  \nStarted: {}\n",
            converter_name(),
            env!("CARGO_PKG_VERSION"),
            CONTRACT_VERSION,
            self.started
        );
        let _ = writeln!(md, "## Sources\n");
        for s in &self.sources {
            let _ = writeln!(md, "- `{}` (sha256 `{}`)", s.path.display(), s.sha256);
        }
        let _ = writeln!(
            md,
            "\n## Counts\n\n| | symbols | footprints | parts | models |\n|---|---|---|---|---|"
        );
        for (label, c) in [
            ("source", self.source_counts),
            ("written", self.written_counts),
        ] {
            let _ = writeln!(
                md,
                "| {label} | {} | {} | {} | {} |",
                c.symbols, c.footprints, c.parts, c.models
            );
        }
        let _ = writeln!(md, "\n## Items\n");
        if self.items.is_empty() {
            let _ = writeln!(md, "None.");
        }
        for severity in ["error", "dropped", "approximated", "info"] {
            let of: Vec<&Item> = self
                .items
                .iter()
                .filter(|i| i.severity.as_str() == severity)
                .collect();
            if of.is_empty() {
                continue;
            }
            let _ = writeln!(md, "### {severity} ({})\n", of.len());
            for i in of {
                let _ = writeln!(
                    md,
                    "- `{}` {} `{}`: {}",
                    i.code,
                    i.kind.as_str(),
                    i.source,
                    i.message
                );
            }
            md.push('\n');
        }
        md
    }

    /// Write `import-report.json` and `.md` into `dir`; returns the JSON path.
    pub fn write(&self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let json_path = dir.join("import-report.json");
        let mut text = serde_json::to_string_pretty(&self.to_json())?;
        text.push('\n');
        write_file(&json_path, &text)?;
        write_file(&dir.join("import-report.md"), &self.to_markdown())?;
        Ok(json_path)
    }
}

/// Write beside the name, then rename, so a reader never sees half a report.
fn write_file(path: &Path, text: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp-import");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

/// SHA-256 of a file, or of a folder: every file in it, by relative path,
/// as (path, length, bytes).
pub fn sha256_of(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    if path.is_dir() {
        let mut files = Vec::new();
        collect_files(path, &mut files)?;
        files.sort();
        for file in files {
            let rel = file.strip_prefix(path).unwrap_or(&file);
            let bytes =
                std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
            hasher.update(rel.to_string_lossy().as_bytes());
            hasher.update([0]);
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(&bytes);
        }
    } else {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    }))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(&path, out)?;
        } else if kind.is_file()
            || (kind.is_symlink() && path.metadata().is_ok_and(|m| m.is_file()))
        {
            // A link to a file counts as that file; a link to a folder is
            // not followed, so a link loop cannot recurse.
            out.push(path);
        }
    }
    Ok(())
}

/// The current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_utc(secs)
}

fn format_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a civil date (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formatting_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(format_utc(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_loop_in_a_folder_does_not_recurse() {
        let dir = std::env::temp_dir().join(format!("snx-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("a.txt"), "a").expect("file");
        std::os::unix::fs::symlink(&dir, dir.join("loop")).expect("link");
        assert!(sha256_of(&dir).is_ok());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn sha256_of_a_file_matches_the_known_digest() {
        let dir = std::env::temp_dir().join(format!("snx-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("abc.txt");
        std::fs::write(&file, "abc").expect("file");
        assert_eq!(
            sha256_of(&file).expect("hash"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
