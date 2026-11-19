// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! The copper of a custom pad: its anchor shape and its filled primitives
//! merged into one outline, since Signex draws a custom pad from its
//! outline alone.
//!
//! The merge works on whole-nanometre corners. Each edge is split where
//! it meets the other ring; a piece of one ring is kept when it lies
//! outside the other (or along it the same way round, kept once), and the
//! kept pieces are walked into rings. A ring of positive area is copper,
//! one of negative area a hole in it.

use std::collections::HashMap;

use snx_writer::model::Point;

use crate::ring::to_nm;

type P = (i128, i128);
type Ring = Vec<P>;

/// The merged copper.
#[derive(Debug, PartialEq)]
pub(crate) struct Merged {
    /// The largest piece, in mm.
    pub(crate) outline: Vec<Point>,
    /// Pieces of copper that touch no other piece, left out.
    pub(crate) pieces_left_out: usize,
    /// Holes in the copper, filled.
    pub(crate) holes_filled: usize,
}

/// The union of `rings` (mm, either winding); `None` when no ring has area
/// or the merge cannot be walked into rings.
pub(crate) fn union(rings: &[Vec<Point>]) -> Option<Merged> {
    let mut pieces: Vec<Ring> = Vec::new();
    let mut holes_filled = 0;
    for ring in rings.iter().filter_map(|r| prepared(r)) {
        let mut current = ring;
        loop {
            let mut merged_with = None;
            for (i, piece) in pieces.iter().enumerate() {
                let (outers, holes) = union_two(piece, &current)?;
                if outers.len() == 1 {
                    merged_with = Some((i, outers.into_iter().next()?, holes));
                    break;
                }
            }
            match merged_with {
                Some((i, outer, holes)) => {
                    pieces.remove(i);
                    current = outer;
                    holes_filled += holes;
                }
                None => break,
            }
        }
        pieces.push(current);
    }
    let largest =
        (0..pieces.len()).max_by(|&a, &b| twice_area(&pieces[a]).cmp(&twice_area(&pieces[b])))?;
    let outline = pieces[largest]
        .iter()
        .map(|&(x, y)| Point::new(x as f64 / 1.0e6, y as f64 / 1.0e6))
        .collect();
    Some(Merged {
        outline,
        pieces_left_out: pieces.len() - 1,
        holes_filled,
    })
}

/// A ring in whole nm, repeated corners out, wound to a positive area;
/// `None` when it has no area.
fn prepared(ring: &[Point]) -> Option<Ring> {
    let mut out: Ring = ring
        .iter()
        .map(|p| (i128::from(to_nm(p.x)), i128::from(to_nm(p.y))))
        .collect();
    out.dedup();
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    let area = twice_area(&out);
    if out.len() < 3 || area == 0 {
        return None;
    }
    if area < 0 {
        out.reverse();
    }
    Some(out)
}

fn twice_area(ring: &[P]) -> i128 {
    let n = ring.len();
    (0..n).map(|i| cross(ring[i], ring[(i + 1) % n])).sum()
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn cross(a: P, b: P) -> i128 {
    a.0 * b.1 - a.1 * b.0
}

fn dot(a: P, b: P) -> i128 {
    a.0 * b.0 + a.1 * b.1
}

/// `n / d` rounded half away from zero; `d > 0`.
fn div_round(n: i128, d: i128) -> i128 {
    if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    }
}

/// `c` lies on the segment `a`-`b`, ends included.
fn on_segment(a: P, b: P, c: P) -> bool {
    cross(sub(b, a), sub(c, a)) == 0
        && a.0.min(b.0) <= c.0
        && c.0 <= a.0.max(b.0)
        && a.1.min(b.1) <= c.1
        && c.1 <= a.1.max(b.1)
}

/// The points the segments `p1`-`p2` and `q1`-`q2` share: the crossing
/// point (rounded to whole nm), or the ends of a shared stretch.
fn meeting_points(p1: P, p2: P, q1: P, q2: P) -> Vec<P> {
    let (r, s) = (sub(p2, p1), sub(q2, q1));
    let qp = sub(q1, p1);
    let mut denom = cross(r, s);
    if denom == 0 {
        if cross(qp, r) != 0 {
            return Vec::new();
        }
        return [p1, p2, q1, q2]
            .into_iter()
            .filter(|&c| on_segment(p1, p2, c) && on_segment(q1, q2, c))
            .collect();
    }
    let (mut t, mut u) = (cross(qp, s), cross(qp, r));
    if denom < 0 {
        (denom, t, u) = (-denom, -t, -u);
    }
    if t < 0 || t > denom || u < 0 || u > denom {
        return Vec::new();
    }
    vec![(
        p1.0 + div_round(r.0 * t, denom),
        p1.1 + div_round(r.1 * t, denom),
    )]
}

/// Where a point lies against a ring.
enum Place {
    Inside,
    Outside,
    /// On an edge running the given way.
    On(P),
}

/// Where `m` lies against `ring`; both at twice their size, so the middle
/// of an edge stays whole.
fn place(ring: &[P], m: P) -> Place {
    let n = ring.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if on_segment(a, b, m) {
            return Place::On(sub(b, a));
        }
        if (a.1 > m.1) != (b.1 > m.1) {
            // `m` is left of the edge's crossing of its row.
            let (dy, lhs, rhs) = (
                b.1 - a.1,
                (m.0 - a.0) * (b.1 - a.1),
                (m.1 - a.1) * (b.0 - a.0),
            );
            if (dy > 0 && lhs < rhs) || (dy < 0 && lhs > rhs) {
                inside = !inside;
            }
        }
    }
    if inside {
        Place::Inside
    } else {
        Place::Outside
    }
}

/// The edges of `ring` cut at every point the other ring meets them.
fn pieces_of(ring: &[P], cuts: &[Vec<P>]) -> Vec<(P, P)> {
    let n = ring.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let dir = sub(b, a);
        let mut points = cuts[i].clone();
        points.push(a);
        points.push(b);
        points.sort_by_key(|&p| dot(sub(p, a), dir));
        points.dedup();
        out.extend(points.windows(2).map(|w| (w[0], w[1])));
    }
    out
}

/// `a` ∪ `b`: the copper rings and the number of holes.
fn union_two(a: &[P], b: &[P]) -> Option<(Vec<Ring>, usize)> {
    let mut cuts_a: Vec<Vec<P>> = vec![Vec::new(); a.len()];
    let mut cuts_b: Vec<Vec<P>> = vec![Vec::new(); b.len()];
    for i in 0..a.len() {
        for j in 0..b.len() {
            let points = meeting_points(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]);
            cuts_a[i].extend(&points);
            cuts_b[j].extend(&points);
        }
    }
    let twice = |r: &[P]| -> Ring { r.iter().map(|&(x, y)| (2 * x, 2 * y)).collect() };
    let (a2, b2) = (twice(a), twice(b));
    let middle = |(p, q): (P, P)| (p.0 + q.0, p.1 + q.1);
    let mut kept: Vec<(P, P)> = Vec::new();
    for edge in pieces_of(a, &cuts_a) {
        let keep = match place(&b2, middle(edge)) {
            Place::Outside => true,
            Place::Inside => false,
            Place::On(way) => dot(sub(edge.1, edge.0), way) > 0,
        };
        if keep {
            kept.push(edge);
        }
    }
    for edge in pieces_of(b, &cuts_b) {
        if matches!(place(&a2, middle(edge)), Place::Outside) {
            kept.push(edge);
        }
    }
    let rings = walk(&kept)?;
    let (outers, holes): (Vec<Ring>, Vec<Ring>) =
        rings.into_iter().partition(|r| twice_area(r) > 0);
    Some((outers, holes.len()))
}

/// The kept edges walked into closed rings, turning as far left as the
/// edges allow where several leave one corner; `None` when an edge leads
/// nowhere. Straight-through corners are dropped.
fn walk(edges: &[(P, P)]) -> Option<Vec<Ring>> {
    let mut leaving: HashMap<P, Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        leaving.entry(e.0).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut rings = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        let mut ring = vec![edges[first].0];
        let mut current = first;
        loop {
            used[current] = true;
            let (from, to) = edges[current];
            if to == ring[0] {
                break;
            }
            ring.push(to);
            let way_in = sub(to, from);
            let turn = |i: usize| {
                let way_out = sub(edges[i].1, edges[i].0);
                (cross(way_in, way_out) as f64).atan2(dot(way_in, way_out) as f64)
            };
            current = leaving
                .get(&to)?
                .iter()
                .copied()
                .filter(|&i| !used[i])
                .max_by(|&x, &y| turn(x).total_cmp(&turn(y)))?;
        }
        let ring = without_straight_corners(ring);
        if ring.len() >= 3 && twice_area(&ring) != 0 {
            rings.push(ring);
        }
    }
    Some(rings)
}

fn without_straight_corners(mut ring: Ring) -> Ring {
    let mut changed = true;
    while changed && ring.len() >= 3 {
        changed = false;
        let n = ring.len();
        for i in 0..n {
            let (prev, at, next) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            let (u, v) = (sub(at, prev), sub(next, at));
            if cross(u, v) == 0 && dot(u, v) > 0 {
                ring.remove(i);
                changed = true;
                break;
            }
        }
    }
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, side: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x0 + side, y0),
            Point::new(x0 + side, y0 + side),
            Point::new(x0, y0 + side),
        ]
    }

    fn area_mm2(ring: &[Point]) -> f64 {
        let n = ring.len();
        (0..n)
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                a.x * b.y - b.x * a.y
            })
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn overlapping_squares_merge_into_one_outline() {
        let m = union(&[square(0.0, 0.0, 2.0), square(1.0, 1.0, 2.0)]).expect("merges");
        assert_eq!(m.outline.len(), 8);
        assert!((area_mm2(&m.outline) - 7.0).abs() < 1e-9);
        assert_eq!((m.pieces_left_out, m.holes_filled), (0, 0));
    }

    #[test]
    fn a_shape_inside_another_adds_nothing() {
        let m = union(&[square(0.0, 0.0, 4.0), square(1.0, 1.0, 1.0)]).expect("merges");
        assert_eq!(m.outline.len(), 4);
        assert!((area_mm2(&m.outline) - 16.0).abs() < 1e-9);
    }

    #[test]
    fn shapes_sharing_an_edge_merge_without_it() {
        let m = union(&[square(0.0, 0.0, 1.0), square(1.0, 0.0, 1.0)]).expect("merges");
        assert_eq!(m.outline.len(), 4);
        assert!((area_mm2(&m.outline) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn apart_shapes_keep_the_largest_and_count_the_rest() {
        let m = union(&[square(0.0, 0.0, 1.0), square(5.0, 0.0, 2.0)]).expect("merges");
        assert!((area_mm2(&m.outline) - 4.0).abs() < 1e-9);
        assert_eq!(m.pieces_left_out, 1);
    }
}
