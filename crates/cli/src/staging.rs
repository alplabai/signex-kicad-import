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
        Self::new_as(out, current_uid())
    }

    /// `new`, with the user id the leftover folder must belong to (a test
    /// passes another one). A leftover staging folder that is not provably
    /// ours is neither restored nor removed: the error says to inspect it.
    pub fn new_as(out: &Path, uid: Option<u32>) -> Result<Self> {
        let name = out
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let dir = out.with_file_name(format!(".{name}.tmp-import"));
        if let Ok(meta) = std::fs::symlink_metadata(&dir) {
            if !meta.is_dir() {
                anyhow::bail!("{} is not a plain folder; remove it", dir.display());
            }
            if !owned_by(&meta, uid) {
                anyhow::bail!(
                    "{} was left by another user or its owner cannot be verified on this system; inspect it and remove it by hand",
                    dir.display()
                );
            }
            // A run that died while committing left what it had moved aside
            // in `.backup`: put it back before clearing the folder.
            restore_backup(&dir.join(".backup"), out)?;
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
            if !plain_relative(rel) {
                anyhow::bail!("refusing the path {}", rel.display());
            }
            let target = self.out.join(rel);
            if !no_links_below(&self.out, rel) {
                anyhow::bail!("{} lies behind a link; not moved", target.display());
            }
            // Only a regular file is moved; a link, a folder or nothing is left alone.
            match std::fs::symlink_metadata(&target) {
                Ok(m) if m.is_file() => {}
                _ => return Ok(()),
            }
            let to = backup.join(rel);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Written before the move, so a crash can always find what was moved.
            {
                use std::io::Write as _;
                let mut m = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(backup.join(MANIFEST))?;
                writeln!(m, "{}", rel.display())?;
                m.sync_all()?;
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
            if !no_links_below(&self.out, rel) {
                anyhow::bail!("{} lies behind a link; not written", target.display());
            }
            std::fs::rename(self.dir.join(rel), &target)
                .with_context(|| format!("placing {}", target.display()))?;
            placed.push(target);
        }
        Ok(())
    }
}

const MANIFEST: &str = "MANIFEST";

/// A relative path made only of plain names: no `..`, root or prefix.
fn plain_relative(rel: &Path) -> bool {
    rel.components().count() > 0
        && rel
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// The user id this process runs as, found without a system call crate: a
/// file just created is owned by it. `None` when it cannot be told, which the
/// callers treat as "not ours".
#[cfg(unix)]
pub fn current_uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    let marker = std::env::temp_dir().join(format!(".snx-uid-{}", std::process::id()));
    let uid = std::fs::File::create(&marker)
        .and_then(|f| f.metadata())
        .map(|m| m.uid())
        .ok();
    let _ = std::fs::remove_file(&marker);
    uid
}

#[cfg(not(unix))]
pub fn current_uid() -> Option<u32> {
    None
}

/// Whether `meta` is a folder of the user `uid`. Fails closed: without a
/// known user, or off unix, nothing is "ours".
#[cfg(unix)]
fn owned_by(meta: &std::fs::Metadata, uid: Option<u32>) -> bool {
    use std::os::unix::fs::MetadataExt;
    uid.is_some_and(|u| u == meta.uid())
}

#[cfg(not(unix))]
fn owned_by(_meta: &std::fs::Metadata, _uid: Option<u32>) -> bool {
    false
}

/// No component of `rel` below `out` is a link: checked just before a move.
/// The check and the move are not atomic; closing that needs `*at` calls
/// (cap-std), and a race needs an attacker who can already write into the
/// output folder.
fn no_links_below(out: &Path, rel: &Path) -> bool {
    let mut cur = out.to_path_buf();
    let comps: Vec<_> = rel.components().collect();
    for (i, c) in comps.iter().enumerate() {
        cur.push(c);
        match std::fs::symlink_metadata(&cur) {
            Ok(m) if m.file_type().is_symlink() => return false,
            Ok(m) if i + 1 < comps.len() && !m.is_dir() => return false,
            _ => {}
        }
    }
    true
}

/// Move back the files a crashed commit moved aside: only those its own
/// manifest lists, only as plain relative paths, and only regular files (a
/// link, even one planted in the folder, is never followed or moved).
fn restore_backup(backup: &Path, out: &Path) -> Result<()> {
    let Ok(list) = std::fs::read_to_string(backup.join(MANIFEST)) else {
        return Ok(());
    };
    for line in list.lines() {
        let rel = Path::new(line);
        if !plain_relative(rel) {
            continue;
        }
        let from = backup.join(rel);
        if !std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file()) {
            continue;
        }
        let target = out.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !no_links_below(out, rel) {
            continue;
        }
        std::fs::rename(&from, &target)
            .with_context(|| format!("restoring {}", target.display()))?;
    }
    Ok(())
}

impl Drop for Staging {
    fn drop(&mut self) {
        // Best effort: the folder may already be gone.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
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
        let stale = [PathBuf::from("symbols/old.snxsym")];
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

    #[test]
    fn a_backup_left_by_a_crashed_run_is_put_back_first() {
        let base = tmp("crash");
        let out = base.join("lib");
        std::fs::create_dir_all(out.join("symbols")).expect("out");
        // The crashed run moved a.snxsym aside and died before placing its new one.
        let leftover = base.join(".lib.tmp-import/.backup/symbols");
        std::fs::create_dir_all(&leftover).expect("backup");
        std::fs::write(leftover.join("a.snxsym"), "precious").expect("f");
        std::fs::write(
            base.join(".lib.tmp-import/.backup/MANIFEST"),
            "symbols/a.snxsym\n",
        )
        .expect("m");
        let _stage = Staging::new(&out).expect("stage");
        assert_eq!(
            std::fs::read(out.join("symbols/a.snxsym")).expect("restored"),
            b"precious"
        );
        assert!(!base.join(".lib.tmp-import/.backup").exists());
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_link_or_an_unlisted_file_in_a_backup_is_never_moved() {
        let base = tmp("planted");
        let out = base.join("lib");
        std::fs::create_dir_all(&out).expect("out");
        std::fs::write(base.join("outside"), "outside").expect("f");
        let backup = base.join(".lib.tmp-import/.backup/symbols");
        std::fs::create_dir_all(&backup).expect("backup");
        std::os::unix::fs::symlink(base.join("outside"), backup.join("link.snxsym")).expect("link");
        std::fs::write(backup.join("unlisted.snxsym"), "x").expect("f");
        std::fs::write(
            base.join(".lib.tmp-import/.backup/MANIFEST"),
            "symbols/link.snxsym\n../outside\n/etc/passwd\n",
        )
        .expect("m");
        let _stage = Staging::new(&out).expect("stage");
        assert_eq!(std::fs::read(base.join("outside")).expect("f"), b"outside");
        assert!(!out.join("symbols/link.snxsym").exists());
        assert!(!out.join("symbols/unlisted.snxsym").exists());
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn a_staging_path_that_is_a_link_is_refused() {
        let base = tmp("linkdir");
        let out = base.join("lib");
        std::fs::create_dir_all(&out).expect("out");
        std::fs::create_dir_all(base.join("elsewhere")).expect("d");
        std::os::unix::fs::symlink(base.join("elsewhere"), base.join(".lib.tmp-import"))
            .expect("link");
        assert!(Staging::new(&out).is_err());
        assert!(base.join("elsewhere").is_dir());
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn a_leftover_folder_of_another_user_is_not_restored_or_removed() {
        let base = tmp("foreign");
        let out = base.join("lib");
        std::fs::create_dir_all(out.join("symbols")).expect("out");
        let backup = base.join(".lib.tmp-import/.backup/symbols");
        std::fs::create_dir_all(&backup).expect("backup");
        std::fs::write(backup.join("a.snxsym"), "x").expect("f");
        std::fs::write(
            base.join(".lib.tmp-import/.backup/MANIFEST"),
            "symbols/a.snxsym\n",
        )
        .expect("m");
        // The owner check is injected: pretend we are a different user, then an unknown one.
        for uid in [Some(current_uid().expect("uid").wrapping_add(1)), None] {
            let err = Staging::new_as(&out, uid).err().expect("refused");
            assert!(err.to_string().contains("inspect"), "{err}");
            assert!(backup.join("a.snxsym").is_file(), "left in place");
            assert!(!out.join("symbols/a.snxsym").exists());
        }
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn a_move_behind_a_linked_folder_is_refused() {
        let base = tmp("behindlink");
        let out = base.join("lib");
        std::fs::create_dir_all(&out).expect("out");
        std::fs::create_dir_all(base.join("elsewhere")).expect("d");
        std::fs::write(base.join("elsewhere/a.snxsym"), "victim").expect("f");
        std::os::unix::fs::symlink(base.join("elsewhere"), out.join("symbols")).expect("link");
        let mut stage = Staging::new(&out).expect("stage");
        stage.write("symbols/a.snxsym", b"new").expect("write");
        assert!(stage.commit(&[PathBuf::from("symbols/a.snxsym")]).is_err());
        assert_eq!(
            std::fs::read(base.join("elsewhere/a.snxsym")).expect("f"),
            b"victim"
        );
        std::fs::remove_dir_all(&base).expect("cleanup");
    }
}
