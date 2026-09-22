//! Delaunay triangulation of 2D points (Bowyer–Watson).
//!
//! Small, dependency-free, `O(n^2)` implementation suited to generative-art
//! site counts (tens to low thousands of points). Returns index triangles
//! into the caller's point slice.

extern crate alloc;

use crate::point::Point;
use alloc::vec::Vec;

/// Epsilon for circumcircle / degeneracy tests.
const EPS: f32 = 1e-6;

#[derive(Clone, Copy, Debug)]
struct Triangle {
    a: usize,
    b: usize,
    c: usize,
}

#[derive(Clone, Copy, Debug)]
struct Circumcircle {
    cx: f32,
    cy: f32,
    r2: f32,
    degenerate: bool,
}

fn circumcircle(pa: Point, pb: Point, pc: Point) -> Circumcircle {
    let d = 2.0 * (pa.x * (pb.y - pc.y) + pb.x * (pc.y - pa.y) + pc.x * (pa.y - pb.y));
    if libm::fabsf(d) < EPS {
        return Circumcircle {
            cx: 0.0,
            cy: 0.0,
            r2: f32::INFINITY,
            degenerate: true,
        };
    }
    let a2 = pa.x * pa.x + pa.y * pa.y;
    let b2 = pb.x * pb.x + pb.y * pb.y;
    let c2 = pc.x * pc.x + pc.y * pc.y;
    let cx = (a2 * (pb.y - pc.y) + b2 * (pc.y - pa.y) + c2 * (pa.y - pb.y)) / d;
    let cy = (a2 * (pc.x - pb.x) + b2 * (pa.x - pc.x) + c2 * (pb.x - pa.x)) / d;
    let dx = pa.x - cx;
    let dy = pa.y - cy;
    Circumcircle {
        cx,
        cy,
        r2: dx * dx + dy * dy,
        degenerate: false,
    }
}

#[inline]
fn in_circumcircle(cc: &Circumcircle, p: Point) -> bool {
    if cc.degenerate {
        return false;
    }
    let dx = p.x - cc.cx;
    let dy = p.y - cc.cy;
    dx * dx + dy * dy <= cc.r2 + 1e-4
}

/// Triangulate `points` with the Bowyer–Watson algorithm.
///
/// - Returns triangles as index triples into `points`, oriented
///   counter-clockwise (positive signed area) where possible.
/// - Degenerate inputs (`len < 3`, all-collinear, exact duplicates) yield
///   whatever non-degenerate triangles exist, possibly none.
/// - Near-duplicate points (within `1e-6`) are skipped to keep the
///   triangulation valid.
pub fn triangulate(points: &[Point]) -> Vec<[usize; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }

    // Filter exact/near duplicates, remembering original indices.
    let mut uniq: Vec<Point> = Vec::with_capacity(points.len());
    let mut index_of: Vec<usize> = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        let dup = uniq.iter().any(|&q| p.dist2(q) < EPS * EPS);
        if !dup {
            index_of.push(i);
            uniq.push(p);
        }
    }
    if uniq.len() < 3 {
        return Vec::new();
    }

    // Bounding box for the super-triangle.
    let mut min_x = uniq[0].x;
    let mut max_x = uniq[0].x;
    let mut min_y = uniq[0].y;
    let mut max_y = uniq[0].y;
    for p in &uniq[1..] {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }
    let dx = (max_x - min_x).max(1.0);
    let dy = (max_y - min_y).max(1.0);
    let delta = dx.max(dy) * 100.0;
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    // Extended vertex list: deduped sites + 3 super-triangle corners.
    let mut verts: Vec<Point> = uniq;
    let s0 = verts.len();
    verts.push(Point::new(mid_x - delta, mid_y - delta));
    verts.push(Point::new(mid_x, mid_y + delta));
    verts.push(Point::new(mid_x + delta, mid_y - delta));

    let mut tris = alloc::vec![Triangle {
        a: s0,
        b: s0 + 1,
        c: s0 + 2
    }];

    for (vi, &p) in verts.iter().enumerate().take(verts.len() - 3) {
        let _ = vi;
        let mut bad: Vec<Triangle> = Vec::new();
        let mut circles: Vec<Circumcircle> = Vec::new();
        for &t in &tris {
            let cc = circumcircle(verts[t.a], verts[t.b], verts[t.c]);
            if in_circumcircle(&cc, p) {
                bad.push(t);
                circles.push(cc);
            }
        }
        if bad.is_empty() {
            continue;
        }
        // Boundary of the polygonal hole: edges used exactly once.
        let mut boundary: Vec<(usize, usize)> = Vec::new();
        for t in &bad {
            for &(u, v) in &[(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
                if let Some(pos) = boundary.iter().position(|&(x, y)| {
                    (x == u && y == v) || (x == v && y == u)
                }) {
                    boundary.swap_remove(pos);
                } else {
                    boundary.push((u, v));
                }
            }
        }
        // `vi` indexes into `verts`, which for the first `n` entries matches
        // the deduped-site order, so the new vertex id is `vi`.
        tris.retain(|t| {
            !bad.iter()
                .any(|b| b.a == t.a && b.b == t.b && b.c == t.c)
        });
        for (u, v) in boundary {
            tris.push(Triangle { a: u, b: v, c: vi });
        }
    }

    // Drop triangles touching the super-triangle and map back to caller indices.
    let n = verts.len() - 3;
    let mut out: Vec<[usize; 3]> = Vec::new();
    for t in tris {
        if t.a >= n || t.b >= n || t.c >= n {
            continue;
        }
        let (ia, ib, ic) = (index_of[t.a], index_of[t.b], index_of[t.c]);
        // Skip degenerate output triangles.
        let area2 = (points[ib].x - points[ia].x) * (points[ic].y - points[ia].y)
            - (points[ic].x - points[ia].x) * (points[ib].y - points[ia].y);
        if libm::fabsf(area2) < EPS {
            continue;
        }
        if area2 > 0.0 {
            out.push([ia, ib, ic]);
        } else {
            out.push([ia, ic, ib]);
        }
    }
    out
}

/// Signed doubled area of triangle `(a, b, c)` (positive = CCW).
pub fn signed_area2(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn quad_yields_two_triangles_covering_hull() {
        let pts = vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let tris = triangulate(&pts);
        assert_eq!(tris.len(), 2, "tris: {tris:?}");
        // Every triangle must be CCW and reference valid indices.
        for [a, b, c] in &tris {
            assert!(*a < 4 && *b < 4 && *c < 4);
            assert!(signed_area2(pts[*a], pts[*b], pts[*c]) > 0.0);
        }
        // Union area must equal the unit square.
        let area: f32 = tris
            .iter()
            .map(|[a, b, c]| signed_area2(pts[*a], pts[*b], pts[*c]) * 0.5)
            .sum();
        assert!((area - 1.0).abs() < 1e-4, "area={area}");
    }

    #[test]
    fn grid_has_no_inverted_triangles_and_covers_hull() {
        let mut pts = Vec::new();
        for iy in 0..4 {
            for ix in 0..4 {
                pts.push(pt(ix as f32, iy as f32));
            }
        }
        let tris = triangulate(&pts);
        // 4x4 grid: hull is 3x3 square (area 9); Delaunay gives 2*(n-1)^2 = 18 tris.
        assert_eq!(tris.len(), 18, "tris: {tris:?}");
        let mut area = 0.0;
        for [a, b, c] in &tris {
            let a2 = signed_area2(pts[*a], pts[*b], pts[*c]);
            assert!(a2 > 0.0, "inverted/degenerate: {a},{b},{c}");
            area += a2 * 0.5;
        }
        assert!((area - 9.0).abs() < 1e-3, "area={area}");
    }

    #[test]
    fn degenerate_inputs_yield_no_triangles() {
        assert!(triangulate(&[]).is_empty());
        assert!(triangulate(&[pt(0.0, 0.0)]).is_empty());
        assert!(triangulate(&[pt(0.0, 0.0), pt(1.0, 1.0)]).is_empty());
        // Collinear: no area.
        let line = vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(2.0, 0.0), pt(3.0, 0.0)];
        assert!(triangulate(&line).is_empty());
        // All duplicates.
        let dups = vec![pt(1.0, 1.0); 5];
        assert!(triangulate(&dups).is_empty());
    }

    #[test]
    fn delaunay_property_holds_for_small_set() {
        // No site may lie strictly inside any triangle's circumcircle.
        let pts = vec![
            pt(0.0, 0.0),
            pt(2.0, 0.3),
            pt(1.0, 2.0),
            pt(3.0, 1.8),
            pt(0.4, 1.2),
        ];
        let tris = triangulate(&pts);
        assert!(!tris.is_empty());
        for [a, b, c] in &tris {
            let cc = circumcircle(pts[*a], pts[*b], pts[*c]);
            for (i, &p) in pts.iter().enumerate() {
                if i == *a || i == *b || i == *c {
                    continue;
                }
                let dx = p.x - cc.cx;
                let dy = p.y - cc.cy;
                assert!(
                    dx * dx + dy * dy >= cc.r2 - 1e-3,
                    "site {i} inside circumcircle of {a},{b},{c}"
                );
            }
        }
    }
}
