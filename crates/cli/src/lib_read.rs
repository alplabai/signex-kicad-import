// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The `library` command's read phase: every input is hashed for the
//! report's `sources`.

use std::path::PathBuf;

use anyhow::Result;
use kicad_parser::Item;

use crate::contract::Counts;
use crate::lib_inputs::InputKind;
use crate::lib_report::{Source, sha256_of};
use crate::progress::Emitter;

pub struct Read {
    pub sources: Vec<Source>,
    pub counts: Counts,
    pub items: Vec<Item>,
}

pub fn read_inputs(inputs: &[(PathBuf, InputKind)], emitter: &Emitter) -> Result<Read> {
    let mut read = Read {
        sources: Vec::new(),
        counts: Counts::default(),
        items: Vec::new(),
    };
    for (done, (path, _kind)) in inputs.iter().enumerate() {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        emitter.progress(done, Some(inputs.len()), &name);
        read.sources.push(Source {
            path: path.clone(),
            sha256: sha256_of(path)?,
        });
        emitter.progress(done + 1, Some(inputs.len()), &name);
    }
    Ok(read)
}
