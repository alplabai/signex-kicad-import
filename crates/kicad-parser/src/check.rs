// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! A self-check of a converted board: where its track ends meet pads once
//! Signex places the pads. A track usually ends on a pad of its own net and
//! never on another net's; many ends on other nets' pads mean the pads were
//! placed wrong.

use snx_writer::model::{Board, Footprint, Point};

/// Counts of track and arc ends by what they land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndCheck {
    pub ends: usize,
    pub on_own_net_pad: usize,
    pub on_other_net_pad: usize,
}

struct PlacedPad {
    center: Point,
    reach: f64,
    net: u32,
}

/// Where Signex puts a footprint-local point: mirrored x on the bottom side,
/// turned by the plain rotation matrix, moved to the footprint position.
pub fn signex_world(fp: &Footprint, local: Point) -> Point {
    let p = if fp.layer == "BottomCopper" {
        Point::new(-local.x, local.y)
    } else {
        local
    };
    let (s, c) = fp.rotation.to_radians().sin_cos();
    Point::new(
        fp.position.x + p.x * c - p.y * s,
        fp.position.y + p.x * s + p.y * c,
    )
}

pub fn track_ends(board: &Board) -> EndCheck {
    let mut pads: Vec<PlacedPad> = board
        .footprints
        .iter()
        .flat_map(|fp| {
            fp.pads.iter().filter_map(move |pad| {
                let net = pad.net.as_ref()?.number;
                Some(PlacedPad {
                    center: signex_world(fp, pad.position),
                    reach: pad.size.x.abs().min(pad.size.y.abs()) / 2.0,
                    net,
                })
            })
        })
        .collect();
    pads.sort_by(|a, b| a.center.x.total_cmp(&b.center.x));
    let widest = pads.iter().map(|p| p.reach).fold(0.0, f64::max);
    let ends = board
        .tracks
        .iter()
        .flat_map(|t| [(t.start, t.net), (t.end, t.net)])
        .chain(board.arcs.iter().flat_map(|a| [(a.start, a.net), (a.end, a.net)]));
    let mut check = EndCheck::default();
    for (at, net) in ends {
        check.ends += 1;
        let from = pads.partition_point(|p| p.center.x < at.x - widest);
        let hits = pads[from..]
            .iter()
            .take_while(|p| p.center.x <= at.x + widest)
            .filter(|p| (p.center.x - at.x).hypot(p.center.y - at.y) <= p.reach);
        let (mut own, mut other) = (false, false);
        for pad in hits {
            if pad.net == net {
                own = true;
            } else {
                other = true;
            }
        }
        if own {
            check.on_own_net_pad += 1;
        } else if other {
            check.on_other_net_pad += 1;
        }
    }
    check
}
