// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Library files are written to a staging folder beside `--output` and moved
//! into it only when the run succeeds. A failed or panicking run drops the
//! staging folder, so it leaves no library file behind and a library that
//! existed before an `--update` run stays as it was (contract section 3.3):
//! files an update replaces or deletes are first moved into the staging
//! folder's `.backup` and put back if any step fails.
//! `import-report.*` is not staged: it stays after a failure.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub struct Staging {
    dir: PathBuf,
    out: PathBuf,
    files: Vec<PathBuf>,
}

impl Staging {
    pub fn new(out: &Path) -> Result<Self> {
        let name = out
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let dir = out.with_file_name(format!(".{name}.tmp-import"));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Staging {
            dir,
            out: out.to_path_buf(),
            files: Vec::new(),
        })
    }

    /// Stage `bytes` as `rel` (a path relative to the library root).
    pub fn write(&mut self, rel: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        self.files.push(PathBuf::from(rel));
        Ok(())
    }

    /// Move every staged file into the output and remove the `stale` files
    /// (paths relative to the output; an update's items that left the
    /// source). A file that is replaced or removed is first moved to the
    /// backup. When any step fails, everything is put back: the new files
    /// removed, the backups restored, so the library stays as it was.
    pub fn commit(self, stale: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let backup = self.dir.join(".backup");
        let mut placed: Vec<PathBuf> = Vec::new();
        let mut saved: Vec<PathBuf> = Vec::new();
        let result = self.apply(stale, &backup, &mut placed, &mut saved);
        if result.is_err() {
            for p in &placed {
                let _ = std::fs::remove_file(p);
            }
            for rel in &saved {
                let target = self.out.join(rel);
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(backup.join(rel), target);
            }
        }
        result.map(|()| placed)
    }

    fn apply(
        &self,
        stale: &[PathBuf],
        backup: &Path,
        placed: &mut Vec<PathBuf>,
        saved: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let save = |rel: &Path, saved: &mut Vec<PathBuf>| -> Result<()> {
            let target = self.out.join(rel);
            if !target.exists() {
                return Ok(());
            }
            let to = backup.join(rel);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&target, &to)
                .with_context(|| format!("backing up {}", target.display()))?;
            saved.push(rel.to_path_buf());
            Ok(())
        };
        for rel in stale {
            save(rel, saved)?;
        }
        for rel in &self.files {
            save(rel, saved)?;
            let target = self.out.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::rename(self.dir.join(rel), &target)
                .with_context(|| format!("placing {}", target.display()))?;
            placed.push(target);
        }
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // Best effort: the folder may already be gone.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The library files of an earlier run that the new run no longer writes:
/// converter-owned files (`symbols/*.snxsym`, `footprints/*.snxfpt`, a
/// `.snxlib` in the root) not among `keep` (relative paths).
pub fn stale_files(out: &Path, keep: &[String]) -> Vec<PathBuf> {
    let mut stale = Vec::new();
    let mut scan = |dir: &Path, ext: &str, prefix: &str| {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_file() && name.ends_with(ext) && !keep.contains(&rel) {
                stale.push(PathBuf::from(rel));
            }
        }
    };
    scan(&out.join("symbols"), ".snxsym", "symbols");
    scan(&out.join("footprints"), ".snxfpt", "footprints");
    scan(out, ".snxlib", "");
    stale.sort();
    stale
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("snx-stage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn commit_moves_files_and_dropping_removes_them() {
        let base = tmp("commit");
        let out = base.join("lib");
        std::fs::create_dir(&out).expect("out");
        let mut stage = Staging::new(&out).expect("stage");
        stage.write("symbols/a.snxsym", b"a").expect("write");
        assert!(!out.join("symbols/a.snxsym").exists());
        let placed = stage.commit(&[]).expect("commit");
        assert_eq!(placed, [out.join("symbols/a.snxsym")]);
        assert_eq!(
            std::fs::read(out.join("symbols/a.snxsym")).expect("file"),
            b"a"
        );
        assert!(!base.join(".lib.tmp-import").exists());

        let mut stage = Staging::new(&out).expect("stage");
        stage.write("symbols/b.snxsym", b"b").expect("write");
        drop(stage);
        assert!(!out.join("symbols/b.snxsym").exists());
        assert!(!base.join(".lib.tmp-import").exists());
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[test]
    fn an_update_replaces_and_deletes_and_a_failure_puts_everything_back() {
        let base = tmp("update");
        let out = base.join("lib");
        std::fs::create_dir_all(out.join("symbols")).expect("out");
        std::fs::write(out.join("symbols/old.snxsym"), "old").expect("f");
        std::fs::write(out.join("symbols/keep.snxsym"), "v1").expect("f");

        // Success: keep.snxsym replaced, old.snxsym deleted, new.snxsym added.
        let mut stage = Staging::new(&out).expect("stage");
        stage.write("symbols/keep.snxsym", b"v2").expect("write");
        stage.write("symbols/new.snxsym", b"new").expect("write");
        let keep = [
            "symbols/keep.snxsym".to_string(),
            "symbols/new.snxsym".to_string(),
        ];
        let stale = stale_files(&out, &keep);
        assert_eq!(stale, [PathBuf::from("symbols/old.snxsym")]);
        stage.commit(&stale).expect("commit");
        assert_eq!(
            std::fs::read(out.join("symbols/keep.snxsym")).expect("f"),
            b"v2"
        );
        assert!(!out.join("symbols/old.snxsym").exists());

        // Failure on the second file: its staged copy has gone missing.
        let mut stage = Staging::new(&out).expect("stage");
        stage.write("symbols/keep.snxsym", b"v3").expect("write");
        stage.write("symbols/zzz.snxsym", b"z").expect("write");
        std::fs::remove_file(stage.dir.join("symbols/zzz.snxsym")).expect("remove");
        let result = stage.commit(&[PathBuf::from("symbols/new.snxsym")]);
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(out.join("symbols/keep.snxsym")).expect("f"),
            b"v2",
            "restored"
        );
        assert_eq!(
            std::fs::read(out.join("symbols/new.snxsym")).expect("f"),
            b"new",
            "undeleted"
        );
        assert!(!out.join("symbols/zzz.snxsym").exists());
        std::fs::remove_dir_all(&base).expect("cleanup");
    }
}
