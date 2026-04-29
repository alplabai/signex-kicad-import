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

//! KiCad S-expression serializer -- writes .kicad_sch / .kicad_pcb files.

pub mod pcb;
pub mod schematic;
mod sexpr_render;

pub use pcb::write_pcb;
pub use schematic::write_schematic;
