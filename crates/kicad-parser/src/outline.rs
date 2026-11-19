// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The board edge: the edge-cut drawings chained into closed rings. The
//! ring with the largest area is the board outline; the drawings of every
//! other ring (cut-outs) and of any open chain stay drawings on the edge
//! layer.

use snx_writer::model::Point;

use crate::geometry::{close_to, twice_area};

/// How near two ends must be to join, mm.
const JOIN_MM: f64 = 0.001;

/// One edge drawing as a polyline (arcs and circles already in chords).
pub(crate) struct EdgePiece {
    pub(crate) points: Vec<Point>,
    /// Whether the polyline closes on itself (a rectangle, circle or polygon).
    pub(crate) closed: bool,
}

/// The outline ring and, for each piece, whether it went into the ring.
pub(crate) struct Outline {
    pub(crate) ring: Vec<Point>,
    pub(crate) used: Vec<bool>,
}

pub(crate) fn build(pieces: &[EdgePiece]) -> Outline {
    let mut used = vec![false; pieces.len()];
    let mut best: Option<(f64, Vec<Point>, Vec<usize>)> = None;
    for start in 0..pieces.len() {
        if used[start] {
            continue;
        }
        let (ring, members) = chain_from(pieces, &mut used, start);
        let Some(ring) = ring else { continue };
        let area = twice_area(&ring).abs();
        if best.as_ref().is_none_or(|(a, _, _)| area > *a) {
            best = Some((area, ring, members));
        }
    }
    let mut used = vec![false; pieces.len()];
    match best {
        Some((_, ring, members)) => {
            for i in members {
                used[i] = true;
            }
            Outline { ring, used }
        }
        None => Outline {
            ring: Vec::new(),
            used,
        },
    }
}

/// The ring the chain from `start` closes, with the pieces in it; `None`
/// when it does not close. Every piece it takes is marked in `used`.
fn chain_from(
    pieces: &[EdgePiece],
    used: &mut [bool],
    start: usize,
) -> (Option<Vec<Point>>, Vec<usize>) {
    used[start] = true;
    let first = &pieces[start];
    if first.closed {
        return (Some(open_ring(first.points.clone())), vec![start]);
    }
    let mut members = vec![start];
    let mut path = first.points.clone();
    loop {
        let (Some(&head), Some(&tail)) = (path.first(), path.last()) else {
            return (None, members);
        };
        if path.len() > 2 && close_to(head, tail, JOIN_MM) {
            return (Some(open_ring(path)), members);
        }
        let next = (0..pieces.len()).find_map(|i| {
            if used[i] || pieces[i].closed {
                return None;
            }
            let pts = &pieces[i].points;
            let (a, b) = (*pts.first()?, *pts.last()?);
            if close_to(a, tail, JOIN_MM) {
                Some((i, false))
            } else if close_to(b, tail, JOIN_MM) {
                Some((i, true))
            } else {
                None
            }
        });
        let Some((i, reversed)) = next else {
            return (None, members);
        };
        used[i] = true;
        members.push(i);
        let mut pts = pieces[i].points.clone();
        if reversed {
            pts.reverse();
        }
        path.extend(pts.into_iter().skip(1));
    }
}

/// A closed polyline without its repeated closing corner, and without
/// consecutive repeats.
fn open_ring(mut points: Vec<Point>) -> Vec<Point> {
    points.dedup_by(|a, b| close_to(*a, *b, JOIN_MM));
    while points.len() > 1 && close_to(points[0], points[points.len() - 1], JOIN_MM) {
        points.pop();
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> EdgePiece {
        EdgePiece {
            points: vec![Point::new(a.0, a.1), Point::new(b.0, b.1)],
            closed: false,
        }
    }

    #[test]
    fn four_lines_in_any_order_and_direction_close_the_outline() {
        let pieces = [
            line((0.0, 0.0), (10.0, 0.0)),
            line((10.0, 5.0), (0.0, 5.0)),
            line((10.0, 0.0), (10.0, 5.0)),
            line((0.0, 0.0), (0.0, 5.0)),
            EdgePiece {
                points: vec![
                    Point::new(2.0, 2.0),
                    Point::new(3.0, 2.0),
                    Point::new(3.0, 3.0),
                ],
                closed: true,
            },
        ];
        let outline = build(&pieces);
        assert_eq!(outline.ring.len(), 4);
        assert_eq!(outline.used, [true, true, true, true, false]);
    }

    #[test]
    fn an_open_edge_gives_no_outline() {
        let outline = build(&[line((0.0, 0.0), (1.0, 0.0))]);
        assert!(outline.ring.is_empty());
        assert_eq!(outline.used, [false]);
    }
}
