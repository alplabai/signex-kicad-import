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

pub mod model;
mod pcb;
mod project;
mod text;

pub use pcb::{FORMAT_TOKEN, GENERATOR, Written, write_board};
pub use project::write_project;
