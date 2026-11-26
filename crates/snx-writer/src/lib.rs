// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Writer for the Signex board (`.snxpcb`) and project (`.snxprj`) files.
//!
//! The board model here is this converter's own: plain data already in the
//! Signex conventions (layer tokens such as `TopCopper`, footprint-local pad
//! coordinates, millimetres in memory). [`write_board`] turns it into the
//! current `.snxpcb` text and [`write_project`] writes the `.snxprj`
//! sidecar that carries the net classes.
//!
//! [`symbol_file`] and [`footprint_file`] write the Signex library
//! envelopes (`.snxsym`, `.snxfpt`) from the public library format spec.

pub mod canon;
pub mod footprint_file;
mod manifest;
pub mod model;
pub mod part_table;
mod pcb;
mod project;
pub mod symbol_file;
mod text;

pub use footprint_file::write_footprint_file;
pub use manifest::{LibraryMeta, write_manifest};
pub use pcb::{FORMAT_TOKEN, GENERATOR, Written, write_board};
pub use project::write_project;
pub use symbol_file::write_symbol_file;
