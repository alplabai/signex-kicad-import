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
    /// passes another one; Windows ignores it and compares owner SIDs). A
    /// leftover staging folder that is not provably ours is neither restored
    /// nor removed: the error says to inspect it.
    pub fn new_as(out: &Path, uid: Option<u32>) -> Result<Self> {
        let name = out
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let dir = out.with_file_name(format!(".{name}.tmp-import"));
        if let Ok(meta) = std::fs::symlink_metadata(&dir) {
            if !meta.is_dir() {
                anyhow::bail!("{} is not a plain folder; remove it", dir.display());
            }
            if !owned_by(&dir, &meta, uid) {
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
        // What we made must be a real folder of ours, not a link someone put there.
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() && (!cfg!(unix) || owned_by(&dir, &meta, uid)) => {}
            _ => anyhow::bail!(
                "{} is not a plain folder of the current user; remove it",
                dir.display()
            ),
        }
        Ok(Staging {
            dir,
            out: out.to_path_buf(),
            files: Vec::new(),
        })
    }

    /// Stage `bytes` as `rel` (a path relative to the library root).
    pub fn write(&mut self, rel: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir.join(rel);
        create_parents_checked(&self.dir, Path::new(rel))?;
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
                if let Ok(rel) = p.strip_prefix(&self.out) {
                    let _ = remove_file_at(&self.out, rel);
                }
            }
            for rel in &saved {
                let _ = move_file(&backup, rel, &self.out, rel);
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
        std::fs::create_dir_all(&self.out)
            .with_context(|| format!("creating {}", self.out.display()))?;
        std::fs::create_dir_all(backup)
            .with_context(|| format!("creating {}", backup.display()))?;
        let save = |rel: &Path, saved: &mut Vec<PathBuf>| -> Result<()> {
            if !plain_relative(rel) {
                anyhow::bail!("refusing the path {}", rel.display());
            }
            // Only a regular file is moved; a link, a folder or nothing is left alone.
            if !is_regular(&self.out, rel) {
                return Ok(());
            }
            std::fs::create_dir_all(backup)?;
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
            move_file(&self.out, rel, backup, rel)
                .with_context(|| format!("backing up {}", self.out.join(rel).display()))?;
            saved.push(rel.to_path_buf());
            Ok(())
        };
        for rel in stale {
            save(rel, saved)?;
        }
        for rel in &self.files {
            save(rel, saved)?;
            let target = self.out.join(rel);
            move_file(&self.dir, rel, &self.out, rel)
                .with_context(|| format!("placing {}", target.display()))?;
            placed.push(target);
        }
        Ok(())
    }
}

const MANIFEST: &str = "MANIFEST";

/// Whether `root/rel` is a regular file, found without following a link in
/// any component.
#[cfg(unix)]
fn is_regular(root: &Path, rel: &Path) -> bool {
    use rustix::fs::{AtFlags, FileType, statat};
    let Some((dir, name)) = unix::parent_fd(root, rel, false).ok().flatten() else {
        return false;
    };
    statat(&dir, &name, AtFlags::SYMLINK_NOFOLLOW)
        .is_ok_and(|st| FileType::from_raw_mode(st.st_mode) == FileType::RegularFile)
}

/// Move `src_root/src_rel` to `dst_root/dst_rel`, creating the folders below
/// `dst_root`. Every component is opened from the folder before it with
/// `O_NOFOLLOW`, and the move is a `renameat` between those folder handles, so
/// a link put in the way after a check cannot redirect it.
#[cfg(unix)]
fn move_file(src_root: &Path, src_rel: &Path, dst_root: &Path, dst_rel: &Path) -> Result<()> {
    let (sdir, sname) = unix::parent_fd(src_root, src_rel, false)?.context("no such folder")?;
    let (ddir, dname) = unix::parent_fd(dst_root, dst_rel, true)?.context("no folder")?;
    rustix::fs::renameat(&sdir, &sname, &ddir, &dname).map_err(std::io::Error::from)?;
    Ok(())
}

#[cfg(unix)]
fn remove_file_at(root: &Path, rel: &Path) -> Result<()> {
    let (dir, name) = unix::parent_fd(root, rel, false)?.context("no such folder")?;
    rustix::fs::unlinkat(&dir, &name, rustix::fs::AtFlags::empty())
        .map_err(std::io::Error::from)?;
    Ok(())
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::os::fd::OwnedFd;
    use std::path::Path;

    use anyhow::Result;
    use rustix::fs::{Mode, OFlags, mkdirat, openat};
    use rustix::io::Errno;

    /// The folder holding `rel` below `root` as a handle, and `rel`'s file name.
    /// With `create`, missing folders are made; a component that is a link or
    /// not a folder is an error either way (`None` when a folder is missing).
    pub fn parent_fd(root: &Path, rel: &Path, create: bool) -> Result<Option<(OwnedFd, OsString)>> {
        let flags = OFlags::DIRECTORY | OFlags::RDONLY | OFlags::CLOEXEC;
        let mut cur = rustix::fs::open(root, flags, Mode::empty()).map_err(std::io::Error::from)?;
        let comps: Vec<_> = rel.components().collect();
        let Some((last, dirs)) = comps.split_last() else {
            return Ok(None);
        };
        for c in dirs {
            let name = c.as_os_str();
            cur = match openat(&cur, name, flags | OFlags::NOFOLLOW, Mode::empty()) {
                Ok(fd) => fd,
                Err(Errno::NOENT) if create => {
                    mkdirat(
                        &cur,
                        name,
                        Mode::RWXU | Mode::RGRP | Mode::XGRP | Mode::ROTH | Mode::XOTH,
                    )
                    .map_err(std::io::Error::from)?;
                    openat(&cur, name, flags | OFlags::NOFOLLOW, Mode::empty())
                        .map_err(std::io::Error::from)?
                }
                Err(Errno::NOENT) => return Ok(None),
                Err(e) => anyhow::bail!(
                    "{} is a link or not a folder: {}",
                    name.to_string_lossy(),
                    std::io::Error::from(e)
                ),
            };
        }
        Ok(Some((cur, last.as_os_str().to_os_string())))
    }
}

#[cfg(not(unix))]
fn is_regular(root: &Path, rel: &Path) -> bool {
    no_links_below(root, rel)
        && std::fs::symlink_metadata(root.join(rel)).is_ok_and(|m| m.is_file())
}

#[cfg(not(unix))]
fn move_file(src_root: &Path, src_rel: &Path, dst_root: &Path, dst_rel: &Path) -> Result<()> {
    create_parents_checked(dst_root, dst_rel)?;
    if !no_links_below(src_root, src_rel) || !no_links_below(dst_root, dst_rel) {
        anyhow::bail!("a link lies in the way");
    }
    std::fs::rename(src_root.join(src_rel), dst_root.join(dst_rel))?;
    Ok(())
}

#[cfg(not(unix))]
fn remove_file_at(root: &Path, rel: &Path) -> Result<()> {
    if no_links_below(root, rel) {
        std::fs::remove_file(root.join(rel))?;
    }
    Ok(())
}

/// A relative path made only of plain names: no `..`, root or prefix.
fn plain_relative(rel: &Path) -> bool {
    rel.components().count() > 0
        && rel
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// The user id this process runs as (`None` off unix; Windows checks owners
/// by SID instead).
#[cfg(unix)]
pub fn current_uid() -> Option<u32> {
    Some(rustix::process::geteuid().as_raw())
}

#[cfg(not(unix))]
pub fn current_uid() -> Option<u32> {
    None
}

/// Create the folders of `rel`'s parent below `base` one at a time, refusing
/// any component that exists as a link or as something other than a folder.
fn create_parents_checked(base: &Path, rel: &Path) -> Result<()> {
    let mut cur = base.to_path_buf();
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    for c in parent.components() {
        cur.push(c);
        match std::fs::symlink_metadata(&cur) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => anyhow::bail!(
                "{} is a link or not a folder; refusing to write below it",
                cur.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&cur).with_context(|| format!("creating {}", cur.display()))?;
            }
            Err(e) => return Err(e).with_context(|| format!("checking {}", cur.display())),
        }
    }
    Ok(())
}

/// Whether the folder `path` (its `meta`) belongs to the user `uid`. Fails
/// closed: without a known user, or where the owner cannot be read, nothing
/// is "ours".
#[cfg(unix)]
fn owned_by(_path: &Path, meta: &std::fs::Metadata, uid: Option<u32>) -> bool {
    use std::os::unix::fs::MetadataExt;
    owner_is_ours(Some(meta.uid()), uid.as_slice(), |a, b| a == b)
}

/// Windows: the owner SID of `path` must be this process's user, or the
/// default owner its token gives new files (an elevated administrator's
/// files are owned by the Administrators group, not by the user).
#[cfg(windows)]
fn owned_by(path: &Path, _meta: &std::fs::Metadata, _uid: Option<u32>) -> bool {
    win::owned_by_current_user(path)
}

#[cfg(not(any(unix, windows)))]
fn owned_by(_path: &Path, _meta: &std::fs::Metadata, _uid: Option<u32>) -> bool {
    false
}

/// The owner check's decision: `owner` must be one of `ours`. No owner, or no
/// one to compare with, is "not ours".
#[cfg_attr(not(any(unix, windows)), allow(dead_code))]
fn owner_is_ours<T>(owner: Option<T>, ours: &[T], same: impl Fn(&T, &T) -> bool) -> bool {
    owner.is_some_and(|o| ours.iter().any(|u| same(&o, u)))
}

/// Owner SIDs through the Win32 security API. Any call that fails means the
/// owner cannot be verified.
#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, IsValidSid, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, TOKEN_INFORMATION_CLASS, TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER,
        TokenOwner, TokenUser,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub fn owned_by_current_user(path: &Path) -> bool {
        let Some(file) = Owner::of(path) else {
            return false;
        };
        let Some(token) = Token::open() else {
            return false;
        };
        let user = token.info(TokenUser);
        let default_owner = token.info(TokenOwner);
        let mut ours: Vec<PSID> = Vec::new();
        if let Some(buf) = &user {
            // SAFETY: GetTokenInformation(TokenUser) filled `buf`, which is
            // 8-aligned, with a TOKEN_USER whose Sid points into `buf`.
            ours.push(unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid });
        }
        if let Some(buf) = &default_owner {
            // SAFETY: as above, a TOKEN_OWNER whose Owner points into `buf`.
            ours.push(unsafe { (*buf.as_ptr().cast::<TOKEN_OWNER>()).Owner });
        }
        ours.retain(|s| valid(*s));
        let owner = Some(file.sid).filter(|s| valid(*s));
        // SAFETY: both SIDs are valid (checked) and outlive the call: the
        // file's lives in `file`'s descriptor, the token's in `user` and
        // `default_owner`, all dropped after this.
        super::owner_is_ours(owner, &ours, |a, b| unsafe { EqualSid(*a, *b) != 0 })
    }

    fn valid(sid: PSID) -> bool {
        // SAFETY: IsValidSid only reads the SID, which is non-null here and
        // points into a buffer the system filled.
        !sid.is_null() && unsafe { IsValidSid(sid) != 0 }
    }

    /// A file's owner SID and the security descriptor holding it.
    struct Owner {
        sid: PSID,
        descriptor: PSECURITY_DESCRIPTOR,
    }

    impl Owner {
        fn of(path: &Path) -> Option<Owner> {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            if wide[..wide.len() - 1].contains(&0) {
                return None;
            }
            let mut sid: PSID = std::ptr::null_mut();
            let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            // SAFETY: `wide` is a NUL-terminated UTF-16 path; the out
            // pointers are valid locals and the unwanted ones are null, as
            // the API allows. On success `descriptor` is ours to LocalFree.
            let err = unsafe {
                GetNamedSecurityInfoW(
                    wide.as_ptr(),
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION,
                    &mut sid,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut descriptor,
                )
            };
            if err != ERROR_SUCCESS || descriptor.is_null() {
                return None;
            }
            // Built first so the descriptor is freed even with no owner (FAT).
            let owner = Owner { sid, descriptor };
            (!owner.sid.is_null()).then_some(owner)
        }
    }

    impl Drop for Owner {
        fn drop(&mut self) {
            // SAFETY: GetNamedSecurityInfoW allocated `descriptor` with
            // LocalAlloc; it is freed once, here.
            unsafe { LocalFree(self.descriptor) };
        }
    }

    /// This process's access token, opened for queries.
    struct Token(HANDLE);

    impl Token {
        fn open() -> Option<Token> {
            let mut handle: HANDLE = std::ptr::null_mut();
            // SAFETY: GetCurrentProcess returns a pseudo handle that needs no
            // closing; `handle` is a valid out pointer.
            let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) };
            (ok != 0 && !handle.is_null()).then_some(Token(handle))
        }

        /// The token information of `class`, in an 8-aligned buffer.
        fn info(&self, class: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
            let mut len = 0u32;
            // SAFETY: a null buffer of length 0 asks only for the size,
            // written to `len`.
            unsafe { GetTokenInformation(self.0, class, std::ptr::null_mut(), 0, &mut len) };
            if len == 0 {
                return None;
            }
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            let cap = u32::try_from(buf.len() * 8).ok()?;
            // SAFETY: `buf` is writable for `cap` >= `len` bytes and lives
            // as long as the returned value.
            let ok = unsafe {
                GetTokenInformation(self.0, class, buf.as_mut_ptr().cast(), cap, &mut len)
            };
            (ok != 0).then_some(buf)
        }
    }

    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenProcessToken and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// No component of `rel` below `out` is a link: checked just before a move.
/// The check and the move are not atomic; closing that needs `*at` calls
/// (cap-std), and a race needs an attacker who can already write into the
/// output folder.
#[cfg_attr(unix, allow(dead_code))]
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
        if !is_regular(backup, rel) {
            continue;
        }
        let _ = move_file(backup, rel, out, rel);
    }
    Ok(())
}

impl Drop for Staging {
    fn drop(&mut self) {
        // Best effort: the folder may already be gone.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A folder junction at `link` pointing to `target`, made with `mklink /J`
/// (no privilege needed, unlike a symlink). False, with a note, when it cannot
/// be made, so the calling test skips.
#[cfg(all(test, windows))]
pub(crate) fn make_junction(link: &Path, target: &Path) -> bool {
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output();
    match made {
        Ok(o) if o.status.success() && link.exists() => true,
        other => {
            eprintln!("note: no junction could be made ({other:?}); the test is skipped");
            false
        }
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

    #[cfg(unix)]
    #[test]
    fn parents_are_created_one_at_a_time_and_a_link_among_them_is_refused() {
        let base = tmp("parents");
        let out = base.join("lib");
        std::fs::create_dir_all(&out).expect("out");
        std::fs::create_dir_all(base.join("elsewhere")).expect("d");
        std::os::unix::fs::symlink(base.join("elsewhere"), out.join("a")).expect("link");
        assert!(create_parents_checked(&out, Path::new("a/b/c.snxsym")).is_err());
        assert!(
            !base.join("elsewhere/b").exists(),
            "nothing created behind the link"
        );
        std::fs::write(out.join("file"), "x").expect("f");
        assert!(create_parents_checked(&out, Path::new("file/x.snxsym")).is_err());
        create_parents_checked(&out, Path::new("n/m/o.snxsym")).expect("plain folders");
        assert!(out.join("n/m").is_dir());
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    #[test]
    fn the_owner_check_fails_closed() {
        let same = |a: &u32, b: &u32| a == b;
        assert!(owner_is_ours(Some(7), &[3, 7], same));
        assert!(!owner_is_ours(Some(7), &[3], same));
        assert!(!owner_is_ours(None, &[7], same));
        assert!(!owner_is_ours(Some(7), &[], same));
    }

    #[cfg(windows)]
    #[test]
    fn a_file_this_process_made_is_ours_and_a_system_file_is_not() {
        let base = tmp("sid");
        std::fs::write(base.join("mine"), "x").expect("f");
        assert!(win::owned_by_current_user(&base));
        assert!(win::owned_by_current_user(&base.join("mine")));
        assert!(!win::owned_by_current_user(&base.join("missing")));
        // System32 belongs to TrustedInstaller, never to a test run.
        let root = std::env::var_os("SystemRoot").expect("SystemRoot");
        assert!(!win::owned_by_current_user(
            &Path::new(&root).join("System32")
        ));
        std::fs::remove_dir_all(&base).expect("cleanup");
    }

    /// std reports a junction as a link, so the checks that refuse a unix
    /// link refuse a junction in the same place.
    #[cfg(windows)]
    #[test]
    fn a_move_behind_a_junction_is_refused() {
        let base = tmp("behindjunction");
        let out = base.join("lib");
        std::fs::create_dir_all(&out).expect("out");
        std::fs::create_dir_all(base.join("elsewhere")).expect("d");
        std::fs::write(base.join("elsewhere/a.snxsym"), "victim").expect("f");
        if !make_junction(&out.join("symbols"), &base.join("elsewhere")) {
            std::fs::remove_dir_all(&base).expect("cleanup");
            return;
        }
        assert!(
            std::fs::symlink_metadata(out.join("symbols"))
                .is_ok_and(|m| m.file_type().is_symlink())
        );
        assert!(!is_regular(&out, Path::new("symbols/a.snxsym")));
        assert!(create_parents_checked(&out, Path::new("symbols/b.snxsym")).is_err());
        let mut stage = Staging::new(&out).expect("stage");
        stage.write("symbols/a.snxsym", b"new").expect("write");
        assert!(stage.commit(&[PathBuf::from("symbols/a.snxsym")]).is_err());
        assert_eq!(
            std::fs::read(base.join("elsewhere/a.snxsym")).expect("f"),
            b"victim"
        );
        assert!(!base.join("elsewhere/b.snxsym").exists());
        std::fs::remove_dir(out.join("symbols")).expect("unlink");
        std::fs::remove_dir_all(&base).expect("cleanup");
    }
}
