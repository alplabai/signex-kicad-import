// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors.
//
// This file is part of signex-kicad-import. signex-kicad-import is
// free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

//! KiCad board reader: a `.kicad_pcb` (and the net classes and rules of
//! the `.kicad_pro` beside it) into the `snx-writer` board model, in Signex
//! conventions: Signex layer tokens, footprint-local pad coordinates that
//! Signex mirrors itself on the bottom side, and Signex rotation sense.

mod board_graphics;
pub mod error;
mod footprint;
pub mod footprint_lib;
mod geometry;
mod ids;
pub mod items;
mod layers;
pub mod libids;
pub mod message_codes;
mod nets;
mod outline;
mod pad_copper;
pub mod pcb;
pub mod placement_check;
pub mod project;
mod report;
mod ring;
pub mod sexpr;
mod stackup;
pub mod symlib;
mod zones;

pub use error::ParseError;
pub use items::{Item, ItemKind, Severity};
pub use pcb::{Converted, convert_pcb};
pub use project::{ProjectRules, parse_project_rules};
pub use report::Report;
