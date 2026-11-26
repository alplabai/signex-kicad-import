// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! 3D models of a footprint: `(model "PATH" (offset (xyz X Y Z)) (scale ..)
//! (rotate ..))` resolved to a STEP file and embedded (library format
//! section 8).
//!
//! A model path in a source library is untrusted input. It is expanded only
//! with the variables a library table may use (`KIPRJMOD`, KiCad's
//! `KICAD<n>_3DMODEL_DIR`, names the user gave), it may only end up in a
//! folder the user or KiCad's own variables named (`--models-dir`,
//! `--search-path`, the footprint's own folder), it must be a regular file
//! (never a link) and at most 256 MiB. Messages show the path as written.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};
use snx_writer::footprint_file::{MAX_MODEL_BYTES, Model, StepAttachment};

use crate::items::{Item, ItemKind, Severity};
use crate::lib_table::{expand_uri, is_kicad_library_variable};
use crate::sexpr::SExpr;

/// How models are handled in a run.
#[derive(Debug, Clone, Default)]
pub struct ModelCx {
    /// `--models skip`: write none and report each.
    pub skip: bool,
    /// `--models-dir`.
    pub models_dir: Option<PathBuf>,
    /// `--search-path` folders.
    pub search: Vec<PathBuf>,
    /// `--search-path NAME=DIR` variables.
    pub named: Vec<(String, String)>,
    /// The environment's `KICAD<n>_*_DIR` variables, as a snapshot.
    pub env: Vec<(String, String)>,
}

impl ModelCx {
    /// A context that reads KiCad's library variables from the environment.
    pub fn from_environment(
        skip: bool,
        models_dir: Option<PathBuf>,
        search: Vec<PathBuf>,
        named: Vec<(String, String)>,
    ) -> Self {
        let env = std::env::vars()
            .filter(|(k, _)| is_kicad_library_variable(k))
            .collect();
        ModelCx {
            skip,
            models_dir,
            search,
            named,
            env,
        }
    }

    /// Folders a model file may live in.
    fn roots(&self, fp_dir: &Path) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = vec![fp_dir.to_path_buf()];
        roots.extend(self.models_dir.iter().cloned());
        roots.extend(self.search.iter().cloned());
        roots.extend(self.named.iter().map(|(_, d)| PathBuf::from(d)));
        roots.extend(
            self.env
                .iter()
                .filter(|(k, _)| k.ends_with("_3DMODEL_DIR"))
                .map(|(_, v)| PathBuf::from(v)),
        );
        roots
            .into_iter()
            .filter_map(|r| r.canonicalize().ok())
            .collect()
    }
}

fn plain(rel: &str) -> bool {
    !rel.is_empty()
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn is_step(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp"))
}

/// The candidate files for a model path, most direct first.
fn candidates(uri: &str, fp_dir: &Path, cx: &ModelCx) -> (Vec<PathBuf>, Vec<String>) {
    let env = |name: &str| {
        cx.env
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let (expanded, missing) = expand_uri(uri, fp_dir, &env, &cx.named);
    let mut out = Vec::new();
    if missing.is_empty() {
        let p = Path::new(&expanded);
        out.push(if p.is_absolute() {
            p.to_path_buf()
        } else {
            fp_dir.join(p)
        });
    }
    // Below the folders the user named: the part after the last variable, or the file name.
    let tail = uri
        .rsplit_once('}')
        .map_or(uri, |(_, t)| t)
        .trim_start_matches(['/', '\\'])
        .to_string();
    let base = uri.rsplit(['/', '\\']).next().unwrap_or("").to_string();
    for dir in cx.models_dir.iter().chain(&cx.search) {
        for c in [&tail, &base] {
            if plain(c) {
                out.push(dir.join(c));
            }
        }
    }
    (out, missing)
}

/// The STEP file for `candidate`: itself, or for another format a STEP file beside it.
fn step_file(candidate: &Path) -> Option<(PathBuf, bool)> {
    let regular = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file());
    if is_step(candidate) {
        return regular(candidate).then(|| (candidate.to_path_buf(), false));
    }
    for ext in ["step", "stp", "STEP", "STP"] {
        let p = candidate.with_extension(ext);
        if regular(&p) {
            return Some((p, true));
        }
    }
    None
}

fn xyz(node: &SExpr, key: &str) -> Option<[f64; 3]> {
    let n = node.find(key)?.find("xyz")?;
    Some([n.arg_f64(0)?, n.arg_f64(1)?, n.arg_f64(2)?])
}

/// Embed the first usable model of a footprint. `source` names the footprint
/// in items; returns the attachment and the model to embed.
pub fn attach(
    models: &[&SExpr],
    fp_dir: &Path,
    cx: &ModelCx,
    source: &str,
    items: &mut Vec<Item>,
) -> Option<(StepAttachment, Model)> {
    let item = |code: &str, sev: Severity, msg: String, path: &str| {
        Item::new(ItemKind::Model, source, code, sev, msg).with_detail(json!({ "path": path }))
    };
    let mut result = None;
    for (i, node) in models.iter().enumerate() {
        let uri = node.first_arg().unwrap_or("");
        if cx.skip {
            items.push(item(
                "model.skipped",
                Severity::Dropped,
                format!("3D model {uri} not written (--models skip)"),
                uri,
            ));
            continue;
        }
        if result.is_some() {
            items.push(item(
                "model.extra_dropped",
                Severity::Dropped,
                format!("3D model {uri} dropped; a footprint holds one"),
                uri,
            ));
            continue;
        }
        let _ = i;
        match load(node, uri, fp_dir, cx, items, &item) {
            Some(r) => result = Some(r),
            None => continue,
        }
    }
    result
}

type MakeItem<'a> = dyn Fn(&str, Severity, String, &str) -> Item + 'a;

fn load(
    node: &SExpr,
    uri: &str,
    fp_dir: &Path,
    cx: &ModelCx,
    items: &mut Vec<Item>,
    item: &MakeItem<'_>,
) -> Option<(StepAttachment, Model)> {
    let (cands, missing) = candidates(uri, fp_dir, cx);
    let found = cands.iter().find_map(|c| step_file(c));
    let Some((path, substituted)) = found else {
        let any_file = cands
            .iter()
            .any(|c| std::fs::symlink_metadata(c).is_ok_and(|m| m.is_file()));
        let (code, msg) = if any_file {
            (
                "model.format_unsupported",
                format!(
                    "3D model {uri} is not a STEP file and has no STEP file beside it; dropped"
                ),
            )
        } else if !missing.is_empty() {
            (
                "model.not_found",
                format!(
                    "3D model {uri} uses {}, which is not set or not allowed; give --models-dir",
                    missing
                        .iter()
                        .map(|n| format!("${{{n}}}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        } else {
            ("model.not_found", format!("3D model {uri} not found"))
        };
        items.push(item(code, Severity::Dropped, msg, uri));
        return None;
    };
    // The file must really lie inside a folder the user or KiCad named.
    let canonical = path.canonicalize().ok()?;
    if !cx.roots(fp_dir).iter().any(|r| canonical.starts_with(r)) {
        items.push(item("model.path_refused", Severity::Dropped, format!("3D model {uri} is outside the folders it may be read from; not read (give --models-dir)"), uri));
        return None;
    }
    let bytes = match read_regular(&path, &canonical) {
        Ok(b) => b,
        Err(Refused::TooLarge) => {
            items.push(item(
                "model.too_large",
                Severity::Dropped,
                format!("3D model {uri} is larger than 256 MiB; not embedded"),
                uri,
            ));
            return None;
        }
        Err(Refused::Unreadable) => {
            items.push(item(
                "model.unreadable",
                Severity::Dropped,
                format!("3D model {uri} could not be read; not embedded"),
                uri,
            ));
            return None;
        }
    };
    if substituted {
        items.push(item(
            "model.step_substituted",
            Severity::Info,
            format!("3D model {uri} is not a STEP file; the STEP file beside it was embedded"),
            uri,
        ));
    }
    // Millimetres in `offset`; the old `at` form is inches.
    let (offset, inches) = match (xyz(node, "offset"), xyz(node, "at")) {
        (Some(o), _) => (o, false),
        (None, Some(a)) => (a, true),
        (None, None) => ([0.0; 3], false),
    };
    let offset = if inches {
        offset.map(|v| v * 25.4)
    } else {
        offset
    };
    if inches {
        items.push(item(
            "model.offset_inches",
            Severity::Approximated,
            format!(
                "3D model {uri}: the old (at) offset is in inches and was converted to millimetres"
            ),
            uri,
        ));
    }
    if xyz(node, "scale").is_some_and(|s| s.iter().any(|v| (v - 1.0).abs() > 1e-9)) {
        items.push(item("model.scale_not_carried", Severity::Approximated, format!("3D model {uri} has a scale other than 1, which Signex cannot carry; embedded unscaled"), uri));
    }
    let hash = Sha256::digest(&bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        });
    let filename = path.file_name().map_or_else(
        || "model.step".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let rotation = xyz(node, "rotate").unwrap_or([0.0; 3]);
    Some((
        StepAttachment {
            content_hash: hash.clone(),
            filename: filename.clone(),
            offset_xyz: offset,
            rotation_xyz: rotation,
        },
        Model {
            hash,
            filename,
            bytes: bytes.into(),
        },
    ))
}

enum Refused {
    TooLarge,
    Unreadable,
}

/// Open `path` without following a link, check the handle (a regular file,
/// within the size cap, the very file `canonical` names: same device and
/// inode), and read at most the cap plus one byte. Opening first and checking
/// the handle leaves no gap between the check and the read for a swap.
fn read_regular(path: &Path, canonical: &Path) -> Result<Vec<u8>, Refused> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let file = opts.open(path).map_err(|_| Refused::Unreadable)?;
    let meta = file.metadata().map_err(|_| Refused::Unreadable)?;
    if !meta.is_file() {
        return Err(Refused::Unreadable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let at = std::fs::metadata(canonical).map_err(|_| Refused::Unreadable)?;
        if (meta.dev(), meta.ino()) != (at.dev(), at.ino()) {
            return Err(Refused::Unreadable);
        }
    }
    #[cfg(not(unix))]
    let _ = canonical;
    if meta.len() > MAX_MODEL_BYTES as u64 {
        return Err(Refused::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_MODEL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Refused::Unreadable)?;
    if bytes.len() > MAX_MODEL_BYTES {
        return Err(Refused::TooLarge);
    }
    Ok(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_not_opened_and_a_swapped_file_is_not_the_checked_one() {
        let dir = std::env::temp_dir().join(format!("snx-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("d");
        std::fs::write(dir.join("a.step"), "a").expect("f");
        std::fs::write(dir.join("b.step"), "b").expect("f");
        std::os::unix::fs::symlink(dir.join("a.step"), dir.join("link.step")).expect("l");
        assert!(read_regular(&dir.join("a.step"), &dir.join("a.step")).is_ok_and(|b| b == b"a"));
        // The path is a link: refused at open.
        assert!(read_regular(&dir.join("link.step"), &dir.join("a.step")).is_err());
        // The handle is not the file the canonical path names.
        assert!(read_regular(&dir.join("b.step"), &dir.join("a.step")).is_err());
        // A folder is not a model.
        assert!(read_regular(&dir, &dir).is_err());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
