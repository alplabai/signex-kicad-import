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
//
// Original Signex authorship: this file was written for the main
// signex repository (Apache-2.0) and was relocated to this GPL-3.0
// companion as part of the issue-62 licensing remediation.

//! KiCad S-expression parser -- .kicad_sch, .kicad_pcb, .kicad_sym files.
#![allow(
    clippy::collapsible_if,
    clippy::redundant_closure,
    clippy::unnecessary_lazy_evaluations
)]

pub mod error;
pub mod pcb;
pub mod schematic;
pub mod sexpr;
pub mod sexpr_builder;
pub mod symbol_lib;

pub use error::ParseError;
pub use pcb::{parse_pcb, parse_pcb_file};
pub use schematic::{parse_project, parse_schematic, parse_schematic_file};
pub use symbol_lib::parse_symbol_lib;
