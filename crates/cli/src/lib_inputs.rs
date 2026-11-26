// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's options and what is checked about them before
//! anything is read.

use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Models {
    Include,
    Skip,
}

/// `library <input>... --output <dir>` (contract section 2.2).
#[derive(Args, Debug)]
pub struct LibraryArgs {
    /// Source files or folders; several go into one library.
    #[arg(required = true, num_args = 1..)]
    pub inputs: Vec<PathBuf>,

    /// The library directory to write.
    #[arg(short, long)]
    pub output: PathBuf,

    /// Library display name. Default: derived from the first input.
    #[arg(long)]
    pub name: Option<String>,

    /// Overrides the derived library id (a UUID).
    #[arg(long)]
    pub library_id: Option<String>,

    /// Database sources only: a path, a connection string or a CSV folder.
    #[arg(long)]
    pub db: Option<String>,

    /// Extra folders searched for libraries a source refers to, in order.
    #[arg(long, action = clap::ArgAction::Append)]
    pub search_path: Vec<PathBuf>,

    /// Field-mapping overrides (TOML).
    #[arg(long)]
    pub map: Option<PathBuf>,

    /// `include` embeds 3D models in footprint files, `skip` writes none.
    #[arg(long, value_enum, default_value = "include")]
    pub models: Models,

    /// Folder that model paths in the source resolve against.
    #[arg(long)]
    pub models_dir: Option<PathBuf>,

    /// Read and map everything, write only the report into --output.
    #[arg(long)]
    pub dry_run: bool,

    /// --output holds a library this converter wrote from the same source.
    #[arg(long)]
    pub update: bool,

    /// Print the summary as the last stdout line.
    #[arg(long)]
    pub json: bool,

    /// Print progress events on stdout; implies --json.
    #[arg(long)]
    pub progress: bool,
}

/// What a source path is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    SymbolFile,
    SymbolDir,
    FootprintDir,
    FootprintFile,
    SymLibTable,
    FpLibTable,
}

/// Classify an existing input by its name; the message says what is wrong
/// otherwise.
pub fn classify(path: &Path) -> Result<InputKind, String> {
    if !path.exists() {
        return Err(format!("input not found: {}", path.display()));
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    let dir = path.is_dir();
    let kind = match (name.as_str(), ext, dir) {
        ("sym-lib-table", _, false) => InputKind::SymLibTable,
        ("fp-lib-table", _, false) => InputKind::FpLibTable,
        (_, "kicad_sym", false) => InputKind::SymbolFile,
        (_, "kicad_symdir", true) => InputKind::SymbolDir,
        (_, "pretty", true) => InputKind::FootprintDir,
        (_, "kicad_mod", false) => InputKind::FootprintFile,
        (_, "kicad_sym", true) => {
            return Err(format!(
                "{} is a folder; a .kicad_sym is one file",
                path.display()
            ));
        }
        (_, "kicad_symdir" | "pretty", false) => {
            return Err(format!("{} is a file; expected a folder", path.display()));
        }
        _ => {
            return Err(format!(
                "unsupported input {}: expected .kicad_sym, .kicad_symdir, .pretty, .kicad_mod, \
                 sym-lib-table or fp-lib-table",
                path.display()
            ));
        }
    };
    Ok(kind)
}

/// Check the options against the file system, before any input is read.
/// Returns the classified inputs and the absolute output path.
pub fn validate(args: &LibraryArgs) -> Result<(Vec<(PathBuf, InputKind)>, PathBuf), String> {
    if let Some(id) = &args.library_id
        && uuid::Uuid::parse_str(id).is_err()
    {
        return Err(format!("--library-id is not a UUID: {id}"));
    }
    if let Some(map) = &args.map
        && !map.is_file()
    {
        return Err(format!("--map file not found: {}", map.display()));
    }
    if let Some(dir) = &args.models_dir
        && !dir.is_dir()
    {
        return Err(format!("--models-dir is not a folder: {}", dir.display()));
    }
    for dir in &args.search_path {
        if !dir.is_dir() {
            return Err(format!("--search-path is not a folder: {}", dir.display()));
        }
    }
    let mut inputs = Vec::new();
    for path in &args.inputs {
        let kind = classify(path)?;
        let abs =
            std::path::absolute(path).map_err(|e| format!("input path {}: {e}", path.display()))?;
        inputs.push((abs, kind));
    }
    let out = std::path::absolute(&args.output)
        .map_err(|e| format!("output path {}: {e}", args.output.display()))?;
    if out.exists() {
        if !out.is_dir() {
            return Err(format!("--output is not a folder: {}", out.display()));
        }
        let empty = std::fs::read_dir(&out)
            .map_err(|e| format!("reading {}: {e}", out.display()))?
            .next()
            .is_none();
        if !empty && !args.update {
            return Err(format!(
                "{} is not empty; pass --update to overwrite a library this converter wrote",
                out.display()
            ));
        }
    } else if args.update {
        return Err(format!("--update: {} does not exist", out.display()));
    }
    Ok((inputs, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("snx-inputs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn inputs_are_classified_by_name_and_shape() {
        let dir = tmp("classify");
        std::fs::write(dir.join("a.KICAD_SYM"), "").expect("f");
        std::fs::write(dir.join("sym-lib-table"), "").expect("f");
        std::fs::write(dir.join("fp-lib-table"), "").expect("f");
        std::fs::create_dir(dir.join("b.kicad_symdir")).expect("d");
        std::fs::create_dir(dir.join("c.pretty")).expect("d");
        std::fs::write(dir.join("d.txt"), "").expect("f");
        assert_eq!(
            classify(&dir.join("a.KICAD_SYM")),
            Ok(InputKind::SymbolFile)
        );
        assert_eq!(
            classify(&dir.join("sym-lib-table")),
            Ok(InputKind::SymLibTable)
        );
        assert_eq!(
            classify(&dir.join("fp-lib-table")),
            Ok(InputKind::FpLibTable)
        );
        assert_eq!(
            classify(&dir.join("b.kicad_symdir")),
            Ok(InputKind::SymbolDir)
        );
        assert_eq!(classify(&dir.join("c.pretty")), Ok(InputKind::FootprintDir));
        assert!(classify(&dir.join("d.txt")).is_err());
        assert!(classify(&dir.join("missing.kicad_sym")).is_err());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
