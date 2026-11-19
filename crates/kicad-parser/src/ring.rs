// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Whether a custom pad outline is one Signex accepts: a simple polygon,
//! checked on the whole-nanometre corners the file stores (the rule in
//! `docs/SNX_FORMAT_NOTES.md`, "Custom pad outlines"). The edge test is the
//! textbook orientation test for two segments.

use snx_writer::model::Point;

type Corner = (i128, i128);

/// A length in mm as whole nanometres.
pub(crate) fn to_nm(mm: f64) -> i64 {
    (mm * 1.0e6).round() as i64
}

fn corner(p: Point) -> Corner {
    (i128::from(to_nm(p.x)), i128::from(to_nm(p.y)))
}

/// Which side of the line `a`→`b` the point `c` is on: 1 left, -1 right,
/// 0 on the line.
fn side(a: Corner, b: Corner, c: Corner) -> i128 {
    ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).signum()
}

/// `c`, already on the line through `a` and `b`, lies between them.
fn between(a: Corner, b: Corner, c: Corner) -> bool {
    a.0.min(b.0) <= c.0 && c.0 <= a.0.max(b.0) && a.1.min(b.1) <= c.1 && c.1 <= a.1.max(b.1)
}

/// The segments `p1`-`p2` and `p3`-`p4` share at least one point.
fn segments_meet(p1: Corner, p2: Corner, p3: Corner, p4: Corner) -> bool {
    let (s1, s2) = (side(p3, p4, p1), side(p3, p4, p2));
    let (s3, s4) = (side(p1, p2, p3), side(p1, p2, p4));
    if s1 * s2 < 0 && s3 * s4 < 0 {
        return true;
    }
    (s1 == 0 && between(p3, p4, p1))
        || (s2 == 0 && between(p3, p4, p2))
        || (s3 == 0 && between(p1, p2, p3))
        || (s4 == 0 && between(p1, p2, p4))
}

/// Two edges that follow each other turn straight back along themselves.
fn folds_back(prev: Corner, at: Corner, next: Corner) -> bool {
    let (u, v) = (
        (at.0 - prev.0, at.1 - prev.1),
        (next.0 - at.0, next.1 - at.1),
    );
    let cross = u.0 * v.1 - u.1 * v.0;
    let dot = u.0 * v.0 + u.1 * v.1;
    cross == 0 && dot < 0
}

/// At least three corners, no zero-length edge, a non-zero area, no two
/// edges that follow each other folding back, and no two other edges that
/// touch or cross.
pub(crate) fn is_simple_ring(points: &[Point]) -> bool {
    let corners: Vec<Corner> = points.iter().map(|&p| corner(p)).collect();
    let n = corners.len();
    if n < 3 {
        return false;
    }
    let at = |i: usize| corners[i % n];
    let doubled_area: i128 = (0..n)
        .map(|i| {
            let (a, b) = (at(i), at(i + 1));
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    if doubled_area == 0 {
        return false;
    }
    if (0..n).any(|i| at(i) == at(i + 1) || folds_back(at(i), at(i + 1), at(i + 2))) {
        return false;
    }
    // Edge i runs from corner i to corner i + 1; edges i and j are
    // neighbours when j is i + 1, or i is 0 and j the last edge.
    for i in 0..n {
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_meet(at(i), at(i + 1), at(j), at(j + 1)) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(points: &[(f64, f64)]) -> Vec<Point> {
        points.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn a_square_and_an_l_shape_are_simple() {
        let square = ring(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert!(is_simple_ring(&square));
        let l_shape = ring(&[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
        ]);
        assert!(is_simple_ring(&l_shape));
    }

    #[test]
    fn crossing_folding_and_flat_rings_are_not() {
        let bowtie = ring(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 1.0)]);
        assert!(!is_simple_ring(&bowtie));
        let folded = ring(&[(0.0, 0.0), (2.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        assert!(!is_simple_ring(&folded));
        let flat = ring(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]);
        assert!(!is_simple_ring(&flat));
        assert!(!is_simple_ring(&ring(&[(0.0, 0.0), (1.0, 1.0)])));
        let touching = ring(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (2.0, 0.0), (0.0, 4.0)]);
        assert!(!is_simple_ring(&touching));
    }
}
