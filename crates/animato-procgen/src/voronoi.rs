//! Voronoi diagram via sequential half-plane clipping.
//!
//! Each cell starts as the bounding rectangle and is clipped against the
//! perpendicular bisector of every other site (Sutherland–Hodgman against
//! `|p - si| <= |p - sj|`). The result is the exact dual of the Delaunay
//! triangulation for non-degenerate inputs, computed directly in `O(n^2)` per
//! cell without an explicit triangulation pass.

extern crate alloc;

use crate::point::Point;
use alloc::vec::Vec;

/// Axis-aligned clipping rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Lower-left corner.
    pub min: Point,
    /// Upper-right corner.
    pub max: Point,
}

impl Bounds {
    /// Create bounds; corners are sorted so `min <= max` per axis.
    pub fn new(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Unit square `[0,1] x [0,1]`.
    pub fn unit() -> Self {
        Self {
            min: Point::new(0.0, 0.0),
            max: Point::new(1.0, 1.0),
        }
    }

    /// Width (`max.x - min.x`).
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    /// Height (`max.y - min.y`).
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// Area (zero when degenerate).
    pub fn area(&self) -> f32 {
        (self.width().max(0.0)) * (self.height().max(0.0))
    }

    /// `true` when `p` lies inside (inclusive of edges).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    fn corners(&self) -> Vec<Point> {
        alloc::vec![
            self.min,
            Point::new(self.max.x, self.min.y),
            self.max,
            Point::new(self.min.x, self.max.y),
        ]
    }
}

/// One Voronoi region: the set of points closer to `site` than to any other.
#[derive(Clone, Debug, PartialEq)]
pub struct VoronoiCell {
    /// Index of the generating site in the caller's slice.
    pub site: usize,
    /// Clipped convex polygon in CCW order (may be empty for duplicates).
    pub polygon: Vec<Point>,
}

impl VoronoiCell {
    /// Polygon area via the shoelace formula (zero when `< 3` vertices).
    pub fn area(&self) -> f32 {
        polygon_area(&self.polygon)
    }

    /// Area-weighted centroid; falls back to the site when degenerate.
    pub fn centroid(&self, site: Point) -> Point {
        polygon_centroid(&self.polygon).unwrap_or(site)
    }
}

/// Shoelace area (always `>= 0`).
pub fn polygon_area(poly: &[Point]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    libm::fabsf(sum) * 0.5
}

/// Area-weighted centroid, or `None` for degenerate polygons.
pub fn polygon_centroid(poly: &[Point]) -> Option<Point> {
    if poly.len() < 3 {
        return None;
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cross_sum = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let cross = a.x * b.y - b.x * a.y;
        cross_sum += cross;
        cx += (a.x + b.x) * cross;
        cy += (a.y + b.y) * cross;
    }
    if libm::fabsf(cross_sum) < 1e-9 {
        return None;
    }
    Some(Point::new(cx / (3.0 * cross_sum), cy / (3.0 * cross_sum)))
}

/// Clip `poly` to the half-plane closer to `si` than to `sj`.
fn clip_bisector(poly: &[Point], si: Point, sj: Point) -> Vec<Point> {
    // Keep p with dot(p - mid, n) <= 0, where n = sj - si.
    let nx = sj.x - si.x;
    let ny = sj.y - si.y;
    let mx = (si.x + sj.x) * 0.5;
    let my = (si.y + sj.y) * 0.5;
    let inside = |p: Point| (p.x - mx) * nx + (p.y - my) * ny <= 1e-6;

    let mut out: Vec<Point> = Vec::with_capacity(poly.len() + 1);
    if poly.is_empty() {
        return out;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = inside(prev);
    for &cur in poly {
        let cur_in = inside(cur);
        match (prev_in, cur_in) {
            (true, true) => out.push(cur),
            (true, false) => {
                if let Some(hit) = intersect_segment_line(prev, cur, mx, my, nx, ny) {
                    out.push(hit);
                }
            }
            (false, true) => {
                if let Some(hit) = intersect_segment_line(prev, cur, mx, my, nx, ny) {
                    out.push(hit);
                }
                out.push(cur);
            }
            (false, false) => {}
        }
        prev = cur;
        prev_in = cur_in;
    }
    out
}

/// Intersection of segment `ab` with the line `dot(p - m, n) = 0`.
fn intersect_segment_line(
    a: Point,
    b: Point,
    mx: f32,
    my: f32,
    nx: f32,
    ny: f32,
) -> Option<Point> {
    let da = (a.x - mx) * nx + (a.y - my) * ny;
    let db = (b.x - mx) * nx + (b.y - my) * ny;
    let denom = da - db;
    if libm::fabsf(denom) < 1e-12 {
        return None;
    }
    let t = da / denom;
    Some(Point::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
    ))
}

/// Compute the Voronoi diagram of `sites` clipped to `bounds`.
///
/// - One cell per input site, in input order (`cell[i].site == i`).
/// - Duplicate sites produce empty polygons for the later copies.
/// - Cells tile `bounds`: their areas sum to `bounds.area()`.
pub fn voronoi(sites: &[Point], bounds: Bounds) -> Vec<VoronoiCell> {
    let mut cells: Vec<VoronoiCell> = Vec::with_capacity(sites.len());
    for (i, &si) in sites.iter().enumerate() {
        // Duplicate of an earlier site: no region of its own.
        if sites[..i].iter().any(|&q| si.dist2(q) < 1e-12) {
            cells.push(VoronoiCell {
                site: i,
                polygon: Vec::new(),
            });
            continue;
        }
        let mut poly = bounds.corners();
        for (j, &sj) in sites.iter().enumerate() {
            if i == j {
                continue;
            }
            if poly.is_empty() {
                break;
            }
            // Skip exact duplicates of the *other* site list consistently:
            // the first occurrence owns the region.
            if sj.dist2(si) < 1e-12 {
                continue;
            }
            poly = clip_bisector(&poly, si, sj);
        }
        cells.push(VoronoiCell { site: i, polygon: poly });
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn single_site_claims_full_bounds() {
        let bounds = Bounds::new(pt(0.0, 0.0), pt(4.0, 2.0));
        let cells = voronoi(&[pt(1.0, 1.0)], bounds);
        assert_eq!(cells.len(), 1);
        assert!((cells[0].area() - 8.0).abs() < 1e-4);
    }

    #[test]
    fn two_sites_split_bounds_evenly() {
        let bounds = Bounds::new(pt(0.0, 0.0), pt(2.0, 2.0));
        let cells = voronoi(&[pt(0.5, 1.0), pt(1.5, 1.0)], bounds);
        assert_eq!(cells.len(), 2);
        assert!((cells[0].area() - 2.0).abs() < 1e-4);
        assert!((cells[1].area() - 2.0).abs() < 1e-4);
        // Bisector must be the vertical line x = 1.
        for p in &cells[0].polygon {
            assert!(p.x <= 1.0 + 1e-4, "left cell leaks: {p:?}");
        }
        for p in &cells[1].polygon {
            assert!(p.x >= 1.0 - 1e-4, "right cell leaks: {p:?}");
        }
    }

    #[test]
    fn cells_tile_bounds_and_contain_their_sites() {
        let bounds = Bounds::new(pt(0.0, 0.0), pt(3.0, 3.0));
        let sites = vec![
            pt(0.5, 0.5),
            pt(2.5, 0.5),
            pt(1.5, 1.8),
            pt(0.6, 2.5),
            pt(2.4, 2.6),
        ];
        let cells = voronoi(&sites, bounds);
        assert_eq!(cells.len(), sites.len());
        let total: f32 = cells.iter().map(|c| c.area()).sum();
        assert!((total - 9.0).abs() < 1e-3, "tile area={total}");
        for (cell, &site) in cells.iter().zip(sites.iter()) {
            assert!(!cell.polygon.is_empty());
            // Site must be closer to its own cell vertices than to others? At
            // minimum the site must be inside its own clipped polygon.
            assert!(
                point_in_convex_polygon(site, &cell.polygon),
                "site {site:?} outside its cell"
            );
            for (other, &osite) in sites.iter().enumerate() {
                if other == cell.site {
                    continue;
                }
                assert!(
                    site.distance(osite) > 1e-6,
                    "test sites must be distinct"
                );
            }
        }
        // Nearest-site property: sample each cell's centroid — the owning
        // site must be the nearest.
        for cell in &cells {
            let c = cell.centroid(sites[cell.site]);
            let own = sites[cell.site].dist2(c);
            for (j, &s) in sites.iter().enumerate() {
                if j != cell.site {
                    assert!(own <= s.dist2(c) + 1e-4, "centroid misowned");
                }
            }
        }
    }

    fn point_in_convex_polygon(p: Point, poly: &[Point]) -> bool {
        // Convex CCW-or-CW containment with small tolerance.
        if poly.is_empty() {
            return false;
        }
        let mut sign = 0.0f32;
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            if libm::fabsf(cross) < 1e-5 {
                continue;
            }
            let s = if cross > 0.0 { 1.0 } else { -1.0 };
            if sign == 0.0 {
                sign = s;
            } else if sign != s {
                return false;
            }
        }
        true
    }
}
