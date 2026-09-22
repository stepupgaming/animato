//! Worley (cellular) noise sampler.
//!
//! For a query point, finds the nearest (`F1`) and second-nearest (`F2`)
//! feature-point distances over a site set. Combinations (`F2 - F1` for cell
//! borders, `1 / (1 + F1)` for cells) are the classic building blocks for
//! stone, scale, honeycomb, and water-shading looks.

extern crate alloc;

use crate::point::Point;
use alloc::vec::Vec;

/// Brute-force cellular-noise field over an explicit site set.
///
/// Sites are stored verbatim; queries scan all sites (`O(n)`), which is the
/// right trade-off for generative decks (dozens–hundreds of sites) without a
/// hash-grid dependency. Tileable / infinite-lattice modes are intentionally
/// out of scope for this starter.
#[derive(Clone, Debug, PartialEq)]
pub struct WorleyField {
    sites: Vec<Point>,
}

impl WorleyField {
    /// Create a field. Empty site sets sample as `INFINITY` distances.
    pub fn new(sites: Vec<Point>) -> Self {
        Self { sites }
    }

    /// Feature points backing this field.
    pub fn sites(&self) -> &[Point] {
        &self.sites
    }

    /// `(F1, F2)` distances at `p`.
    pub fn sample_f1_f2(&self, p: Point) -> (f32, f32) {
        let mut f1 = f32::INFINITY;
        let mut f2 = f32::INFINITY;
        for &s in &self.sites {
            let d = p.distance(s);
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
        (f1, f2)
    }

    /// Nearest-site distance.
    pub fn f1(&self, p: Point) -> f32 {
        self.sample_f1_f2(p).0
    }

    /// Second-nearest distance.
    pub fn f2(&self, p: Point) -> f32 {
        self.sample_f1_f2(p).1
    }

    /// Cell-edge distance (`F2 - F1`): near `0` on borders, large inside cells.
    pub fn edge(&self, p: Point) -> f32 {
        let (f1, f2) = self.sample_f1_f2(p);
        f2 - f1
    }

    /// Smooth cell interior in `(0, 1]`: `1` at feature points, decaying out.
    pub fn cellular(&self, p: Point) -> f32 {
        1.0 / (1.0 + self.f1(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn f1_is_zero_at_sites() {
        let field = WorleyField::new(alloc::vec![pt(0.0, 0.0), pt(2.0, 0.0)]);
        assert_eq!(field.f1(pt(0.0, 0.0)), 0.0);
        assert_eq!(field.f1(pt(2.0, 0.0)), 0.0);
        assert!((field.f1(pt(1.0, 0.0)) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn edge_is_zero_on_bisectors() {
        let field = WorleyField::new(alloc::vec![pt(0.0, 0.0), pt(2.0, 0.0)]);
        assert!((field.edge(pt(1.0, 5.0))).abs() < 1e-5);
        assert!((field.edge(pt(1.0, 0.0))).abs() < 1e-5);
        assert!(field.edge(pt(0.2, 0.0)) > 1.0);
    }

    #[test]
    fn cellular_decays_with_distance() {
        let field = WorleyField::new(alloc::vec![pt(0.0, 0.0)]);
        assert_eq!(field.cellular(pt(0.0, 0.0)), 1.0);
        let near = field.cellular(pt(0.5, 0.0));
        let far = field.cellular(pt(4.0, 0.0));
        assert!(near > far && far > 0.0);
    }

    #[test]
    fn f1_matches_brute_force() {
        use crate::rng::SmallRng;
        let mut rng = SmallRng::new(21);
        let sites: Vec<Point> = (0..12)
            .map(|_| pt(rng.range(-2.0, 2.0), rng.range(-2.0, 2.0)))
            .collect();
        let field = WorleyField::new(sites.clone());
        for _ in 0..64 {
            let q = pt(rng.range(-2.0, 2.0), rng.range(-2.0, 2.0));
            let expect = sites.iter().map(|s| q.distance(*s)).fold(f32::INFINITY, f32::min);
            assert!((field.f1(q) - expect).abs() < 1e-6);
        }
    }
}
