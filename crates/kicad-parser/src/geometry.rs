// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Plane geometry the conversion needs, and the reads of `(at ..)`,
//! `(xy ..)`-style points from the source tree.

use snx_writer::model::Point;

use crate::sexpr::SExpr;

/// The point in `node`'s first two arguments; `None` unless both read.
pub(crate) fn point_args(node: &SExpr) -> Option<Point> {
    Some(Point::new(node.arg_f64(0)?, node.arg_f64(1)?))
}

/// The point under the child `key` of `node` (`(start x y)` and the like).
pub(crate) fn point_of(node: &SExpr, key: &str) -> Option<Point> {
    node.find(key).and_then(point_args)
}

/// `(at x y [angle])` of `node`: position and angle, zero when absent.
pub(crate) fn at_of(node: &SExpr) -> (Point, f64) {
    match node.find("at") {
        Some(at) => (
            Point::new(at.arg_f64(0).unwrap_or(0.0), at.arg_f64(1).unwrap_or(0.0)),
            at.arg_f64(2).unwrap_or(0.0),
        ),
        None => (Point::default(), 0.0),
    }
}

/// A number under the child `key` of `node` (`(width 0.2)`).
pub(crate) fn number_of(node: &SExpr, key: &str) -> Option<f64> {
    node.find(key).and_then(|n| n.arg_f64(0))
}

/// The stroke width of a drawing: `(stroke (width w))` or `(width w)`.
pub(crate) fn stroke_width(node: &SExpr) -> f64 {
    node.find("stroke")
        .and_then(|s| number_of(s, "width"))
        .or_else(|| number_of(node, "width"))
        .unwrap_or(0.0)
}

/// An angle in `[0, 360)`; `-0` becomes `0`.
pub(crate) fn normalize_deg(deg: f64) -> f64 {
    let turned = deg.rem_euclid(360.0);
    if turned == 0.0 || turned >= 360.0 {
        0.0
    } else {
        turned
    }
}

/// `p` mirrored about the y axis when `mirror`.
pub(crate) fn mirror_x(p: Point, mirror: bool) -> Point {
    if mirror { Point::new(-p.x, p.y) } else { p }
}

/// The centre of the circle through three points; `None` when they lie on
/// one line.
pub(crate) fn circumcenter(a: Point, b: Point, c: Point) -> Option<Point> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    Some(Point::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    ))
}

/// The arc from `start` through `mid` to `end` as a polyline whose chords
/// turn by at most `step_deg`, both ends included. Collinear points give
/// the straight line.
pub(crate) fn arc_polyline(start: Point, mid: Point, end: Point, step_deg: f64) -> Vec<Point> {
    let Some(center) = circumcenter(start, mid, end) else {
        return vec![start, end];
    };
    let angle = |p: Point| (p.y - center.y).atan2(p.x - center.x);
    let radius = (start.x - center.x).hypot(start.y - center.y);
    let (a0, am, a1) = (angle(start), angle(mid), angle(end));
    let tau = std::f64::consts::TAU;
    let forward = (a1 - a0).rem_euclid(tau);
    let sweep = if (am - a0).rem_euclid(tau) <= forward {
        forward
    } else {
        forward - tau
    };
    let steps = ((sweep.abs().to_degrees() / step_deg.max(0.5)).ceil() as usize).max(1);
    let mut points = Vec::with_capacity(steps + 1);
    points.push(start);
    for i in 1..steps {
        let a = a0 + sweep * i as f64 / steps as f64;
        points.push(Point::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    points.push(end);
    points
}

/// The polygon of a circle, `segments` corners.
pub(crate) fn circle_polygon(center: Point, radius: f64, segments: usize) -> Vec<Point> {
    (0..segments)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

/// Twice the signed area of a ring.
pub(crate) fn twice_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

pub(crate) fn close_to(a: Point, b: Point, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_centre_of_a_half_circle() {
        let c = circumcenter(
            Point::new(-1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(1.0, 0.0),
        )
        .expect("a circle");
        assert!(close_to(c, Point::new(0.0, 0.0), 1e-9));
        let line = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(2.0, 2.0),
        ];
        assert!(circumcenter(line[0], line[1], line[2]).is_none());
    }

    #[test]
    fn an_arc_polyline_passes_its_middle_side() {
        let line = arc_polyline(
            Point::new(-1.0, 0.0),
            Point::new(0.0, -1.0),
            Point::new(1.0, 0.0),
            10.0,
        );
        assert!(line.len() > 3);
        assert!(line[1..line.len() - 1].iter().all(|p| p.y < 0.0));
    }

    #[test]
    fn angles_normalize_into_one_turn() {
        assert_eq!(normalize_deg(-90.0), 270.0);
        assert_eq!(normalize_deg(360.0), 0.0);
        assert_eq!(normalize_deg(-0.0), 0.0);
    }
}
