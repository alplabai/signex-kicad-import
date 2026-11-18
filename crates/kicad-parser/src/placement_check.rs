// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! A placement self-check for a converted board.
//!
//! Every track and copper-arc end is looked up among the pads, once each pad
//! sits where Signex will draw it. On a well-converted board an end that
//! touches a pad touches one of its own net; an end that touches only pads
//! of other nets on its layer means pads were placed in the wrong spot (a
//! wrong mirror or turn). The pads are bucketed on a square grid, so each
//! end looks at a handful of pads.

use std::collections::HashMap;

use snx_writer::model::{Board, Footprint, PadType, Point};

/// How many track ends touch a pad of their own net, and how many touch
/// only pads of other nets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndTally {
    pub total: usize,
    pub own_net: usize,
    pub other_net: usize,
}

/// Where Signex draws a footprint-local point on the board: the footprint's
/// bottom side mirrors local x, then the footprint's turn applies
/// (`x cos - y sin`, `x sin + y cos`), then its position.
pub fn placed(fp: &Footprint, local: Point) -> Point {
    let x = if fp.layer == "BottomCopper" {
        -local.x
    } else {
        local.x
    };
    let (sin, cos) = fp.rotation.to_radians().sin_cos();
    Point::new(
        fp.position.x + x * cos - local.y * sin,
        fp.position.y + x * sin + local.y * cos,
    )
}

/// A pad as the check sees it: a disc of the pad's half short side.
struct Site<'b> {
    at: Point,
    radius: f64,
    net: u32,
    /// The copper layers it is on; every layer for a plated hole.
    copper: Option<&'b [String]>,
}

impl Site<'_> {
    fn touches(&self, p: Point, layer: &str) -> bool {
        let on_layer = match self.copper {
            None => true,
            Some(layers) => layers.iter().any(|l| l == layer),
        };
        on_layer && (self.at.x - p.x).hypot(self.at.y - p.y) <= self.radius
    }
}

/// Grid buckets of pads; a cell is as wide as the widest pad disc, so a
/// point only needs its own cell and the eight around it.
struct Grid<'b> {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<Site<'b>>>,
}

impl<'b> Grid<'b> {
    fn new(sites: Vec<Site<'b>>) -> Self {
        let widest = sites.iter().map(|s| s.radius).fold(0.0_f64, f64::max);
        let cell = (2.0 * widest).max(0.1);
        let mut cells: HashMap<(i64, i64), Vec<Site<'b>>> = HashMap::new();
        for site in sites {
            cells.entry(Self::key(cell, site.at)).or_default().push(site);
        }
        Self { cell, cells }
    }

    fn key(cell: f64, p: Point) -> (i64, i64) {
        ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64)
    }

    /// The nets of the pads `p` touches on `layer`.
    fn nets_at(&self, p: Point, layer: &str) -> Vec<u32> {
        let (kx, ky) = Self::key(self.cell, p);
        let mut nets = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(bucket) = self.cells.get(&(kx + dx, ky + dy)) else {
                    continue;
                };
                nets.extend(
                    bucket
                        .iter()
                        .filter(|s| s.touches(p, layer))
                        .map(|s| s.net),
                );
            }
        }
        nets
    }
}

/// Tally the board's track and copper-arc ends against its netted pads.
pub fn tally_track_ends(board: &Board) -> EndTally {
    let mut sites = Vec::new();
    for fp in &board.footprints {
        for pad in &fp.pads {
            let Some(net) = pad.net.as_ref().map(|n| n.number).filter(|&n| n != 0) else {
                continue;
            };
            sites.push(Site {
                at: placed(fp, pad.position),
                radius: pad.size.x.abs().min(pad.size.y.abs()) / 2.0,
                net,
                copper: match pad.pad_type {
                    PadType::Thru => None,
                    _ => Some(pad.layers.as_slice()),
                },
            });
        }
    }
    let grid = Grid::new(sites);
    let mut tally = EndTally::default();
    let mut count = |p: Point, net: u32, layer: &str| {
        tally.total += 1;
        let nets = grid.nets_at(p, layer);
        if nets.contains(&net) {
            tally.own_net += 1;
        } else if !nets.is_empty() {
            tally.other_net += 1;
        }
    };
    for t in &board.tracks {
        count(t.start, t.net, &t.layer);
        count(t.end, t.net, &t.layer);
    }
    for a in &board.arcs {
        count(a.start, a.net, &a.layer);
        count(a.end, a.net, &a.layer);
    }
    tally
}
