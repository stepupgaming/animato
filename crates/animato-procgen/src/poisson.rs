//! Poisson-disk sampling (Bridson's algorithm).
//!
//! Produces evenly-spread points with a guaranteed minimum separation — the
//! standard way to seed Voronoi / Delaunay decks so no two sites clump.
//! Deterministic for a given `(bounds, min_dist, seed)`.

extern crate alloc;

use crate::point::Point;
use crate::rng::SmallRng;
use crate::voronoi::Bounds;
use alloc::vec::Vec;

/// Number of candidates tried around each active point.
const TRIES_PER_POINT: usize = 30;

/// Sample points inside `bounds` with minimum separation `min_dist`.
///
/// - Deterministic for fixed `(bounds, min_dist, seed)`.
/// - Returns an empty vec for non-positive `min_dist` or degenerate bounds.
/// - Every returned pair is at least `min_dist` apart (within tolerance).
pub fn poisson_disk(bounds: Bounds, min_dist: f32, seed: u64) -> Vec<Point> {
    if min_dist <= 0.0 || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return Vec::new();
    }
    let mut rng = SmallRng::new(seed);

    // Grid acceleration: cell size = min_dist / sqrt(2) so each cell holds
    // at most one sample; neighbourhood checks cover 5x5 cells.
    let cell = min_dist / libm::sqrtf(2.0);
    let gw = ((bounds.width() / cell).ceil() as usize).max(1);
    let gh = ((bounds.height() / cell).ceil() as usize).max(1);
    let mut grid: Vec<Option<Point>> = alloc::vec![None; gw * gh];
    let cell_index = |p: Point| {
        let gx = (((p.x - bounds.min.x) / cell).floor() as usize).min(gw - 1);
        let gy = (((p.y - bounds.min.y) / cell).floor() as usize).min(gh - 1);
        gy * gw + gx
    };
    let far_enough = |p: Point, grid: &[Option<Point>]| {
        let gx = (((p.x - bounds.min.x) / cell).floor() as isize)
            .clamp(0, gw as isize - 1);
        let gy = (((p.y - bounds.min.y) / cell).floor() as isize)
            .clamp(0, gh as isize - 1);
        for cy in (gy - 2).max(0)..=(gy + 2).min(gh as isize - 1) {
            for cx in (gx - 2).max(0)..=(gx + 2).min(gw as isize - 1) {
                if let Some(q) = grid[(cy as usize) * gw + (cx as usize)] {
                    if p.dist2(q) < min_dist * min_dist - 1e-6 {
                        return false;
                    }
                }
            }
        }
        true
    };

    let mut samples: Vec<Point> = Vec::new();
    let mut active: Vec<usize> = Vec::new();

    let first = Point::new(
        rng.range(bounds.min.x, bounds.max.x),
        rng.range(bounds.min.y, bounds.max.y),
    );
    samples.push(first);
    grid[cell_index(first)] = Some(first);
    active.push(0);

    while let Some(&head) = active.last() {
        let base = samples[head];
        let mut placed = None;
        for _ in 0..TRIES_PER_POINT {
            let angle = rng.range(0.0, 2.0 * core::f32::consts::PI);
            let radius = rng.range(min_dist, 2.0 * min_dist);
            let cand = Point::new(
                base.x + libm::cosf(angle) * radius,
                base.y + libm::sinf(angle) * radius,
            );
            if !bounds.contains(cand) {
                continue;
            }
            if far_enough(cand, &grid) {
                placed = Some(cand);
                break;
            }
        }
        match placed {
            Some(p) => {
                grid[cell_index(p)] = Some(p);
                samples.push(p);
                active.push(samples.len() - 1);
            }
            None => {
                active.pop();
            }
        }
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_min_dist(pts: &[Point], min_dist: f32) {
        for (i, &a) in pts.iter().enumerate() {
            for &b in &pts[i + 1..] {
                assert!(
                    a.distance(b) + 1e-4 >= min_dist,
                    "pair too close: {a:?} {b:?}"
                );
            }
        }
    }

    #[test]
    fn respects_minimum_distance_and_bounds() {
        let bounds = Bounds::unit();
        let pts = poisson_disk(bounds, 0.15, 99);
        assert!(pts.len() > 10, "too few samples: {}", pts.len());
        for p in &pts {
            assert!(bounds.contains(*p), "outside bounds: {p:?}");
        }
        check_min_dist(&pts, 0.15);
    }

    #[test]
    fn deterministic_for_same_seed() {
        let bounds = Bounds::unit();
        let a = poisson_disk(bounds, 0.2, 7);
        let b = poisson_disk(bounds, 0.2, 7);
        assert_eq!(a, b);
        let c = poisson_disk(bounds, 0.2, 8);
        assert_ne!(a, c, "different seeds should (almost surely) differ");
    }

    #[test]
    fn degenerate_inputs_yield_empty() {
        let bounds = Bounds::unit();
        assert!(poisson_disk(bounds, 0.0, 1).is_empty());
        assert!(poisson_disk(bounds, -1.0, 1).is_empty());
        let flat = Bounds::new(Point::new(0.0, 0.0), Point::new(0.0, 1.0));
        assert!(poisson_disk(flat, 0.1, 1).is_empty());
    }

    #[test]
    fn coverage_scales_with_radius() {
        // Smaller radius must yield at least as many samples.
        let bounds = Bounds::unit();
        let coarse = poisson_disk(bounds, 0.3, 5);
        let fine = poisson_disk(bounds, 0.12, 5);
        assert!(fine.len() > coarse.len());
        assert!(!coarse.is_empty());
    }
}
