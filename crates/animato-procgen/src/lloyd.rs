//! Lloyd relaxation: iterative centroidal Voronoi smoothing.
//!
//! Repeatedly replaces each site with the centroid of its Voronoi cell.
//! A few iterations turn clumps / grids into the even, organic point sets
//! used for generative decks, stippling, and cellular textures.

extern crate alloc;

use crate::point::Point;
use crate::voronoi::{Bounds, voronoi};
use alloc::vec::Vec;

/// Relax `sites` inside `bounds` for `iterations` Lloyd steps.
///
/// Returns a new point set (inputs are not modified). `0` iterations returns
/// a copy of the input. Sites are clamped into `bounds` first so stray inputs
/// cannot produce empty cells.
pub fn lloyd_relax(sites: &[Point], bounds: Bounds, iterations: usize) -> Vec<Point> {
    let mut current: Vec<Point> = sites
        .iter()
        .map(|&p| {
            Point::new(
                p.x.clamp(bounds.min.x, bounds.max.x),
                p.y.clamp(bounds.min.y, bounds.max.y),
            )
        })
        .collect();
    for _ in 0..iterations {
        if current.is_empty() {
            break;
        }
        let cells = voronoi(&current, bounds);
        for (cell, site) in cells.iter().zip(current.iter_mut()) {
            *site = cell.centroid(*site);
        }
    }
    current
}

/// Mean nearest-neighbour distance — a cheap evenness metric used in tests.
#[cfg(test)]
pub(crate) fn mean_nearest_distance(pts: &[Point]) -> f32 {
    if pts.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0;
    for (i, &a) in pts.iter().enumerate() {
        let mut best = f32::INFINITY;
        for (j, &b) in pts.iter().enumerate() {
            if i == j {
                continue;
            }
            let d = a.dist2(b);
            if d < best {
                best = d;
            }
        }
        sum += libm::sqrtf(best);
    }
    sum / pts.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SmallRng;
    use alloc::vec;

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn zero_iterations_returns_copy() {
        let sites = vec![pt(0.2, 0.3), pt(0.8, 0.7)];
        let bounds = Bounds::unit();
        assert_eq!(lloyd_relax(&sites, bounds, 0), sites);
    }

    #[test]
    fn relaxation_improves_spacing_evenness() {
        // Clustered RNG points should spread out: the minimum nearest-neighbour
        // gap grows and stays inside bounds.
        let bounds = Bounds::unit();
        let mut rng = SmallRng::new(1234);
        let mut sites = Vec::new();
        for _ in 0..24 {
            // Tight cluster in one corner.
            sites.push(pt(rng.range(0.0, 0.3), rng.range(0.0, 0.3)));
        }
        let min_gap_before = sites
            .iter()
            .enumerate()
            .map(|(i, &a)| {
                sites
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, &b)| a.distance(b))
                    .fold(f32::INFINITY, f32::min)
            })
            .fold(f32::INFINITY, f32::min);
        let relaxed = lloyd_relax(&sites, bounds, 12);
        assert_eq!(relaxed.len(), sites.len());
        for p in &relaxed {
            assert!(bounds.contains(*p), "escaped bounds: {p:?}");
        }
        let min_gap_after = relaxed
            .iter()
            .enumerate()
            .map(|(i, &a)| {
                relaxed
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, &b)| a.distance(b))
                    .fold(f32::INFINITY, f32::min)
            })
            .fold(f32::INFINITY, f32::min);
        assert!(
            min_gap_after > min_gap_before,
            "no spreading: before={min_gap_before} after={min_gap_after}"
        );
        // Centroidal convergence: one more pass barely moves (fixed-point).
        let again = lloyd_relax(&relaxed, bounds, 1);
        let shift: f32 = relaxed
            .iter()
            .zip(again.iter())
            .map(|(a, b)| a.distance(*b))
            .sum();
        let mean_shift = shift / relaxed.len() as f32;
        assert!(mean_shift < 0.05, "not converging: {mean_shift}");
    }

    #[test]
    fn grid_stays_put_when_already_centroidal() {
        // A symmetric 2x2 grid in the unit square is already centroidal.
        let bounds = Bounds::unit();
        let sites = vec![
            pt(0.25, 0.25),
            pt(0.75, 0.25),
            pt(0.25, 0.75),
            pt(0.75, 0.75),
        ];
        let relaxed = lloyd_relax(&sites, bounds, 3);
        for (a, b) in sites.iter().zip(relaxed.iter()) {
            assert!(
                a.distance(*b) < 1e-4,
                "centroidal grid drifted: {a:?} -> {b:?}"
            );
        }
    }
}
