//! Starter caustics field: a seekable, analytic intensity function.
//!
//! **Not light transport.** Real caustics integrate refracted light paths;
//! this is a cheap procedural stand-in for animated water-light looks on
//! generative decks: Worley cell edges (`F2 - F1`) over sites that wobble
//! with time, sharpened into bright filaments.
//!
//! The field is a pure function of `(x, y, time)` — fully seekable — and also
//! implements [`Update`](animato_core::Update) /
//! [`Playable`](animato_core::Playable) so Animato clocks and drivers can
//! drive it like any other animation. Sites wobble with a `period`, so
//! `seek_to(progress)` maps to `time = progress * period`.

extern crate alloc;

use crate::point::Point;
use alloc::vec::Vec;
use animato_core::{Playable, Update};

/// Analytic water-caustics-style intensity field.
#[derive(Clone, Debug, PartialEq)]
pub struct CausticField {
    sites: Vec<Point>,
    time: f32,
    /// Seconds per wobble cycle. Must be `> 0`.
    period: f32,
    /// Wobble amplitude in model units.
    amplitude: f32,
    /// Edge width mapping `F2 - F1` into brightness.
    edge_width: f32,
    /// Filament sharpness exponent.
    sharpness: f32,
}

impl CausticField {
    /// Create a field over `sites` with default look parameters.
    pub fn new(sites: Vec<Point>) -> Self {
        Self {
            sites,
            time: 0.0,
            period: 4.0,
            amplitude: 0.08,
            edge_width: 0.18,
            sharpness: 3.0,
        }
    }

    /// Override the wobble period (clamped to `> 0`).
    pub fn with_period(mut self, period: f32) -> Self {
        self.period = if period.is_finite() && period > 1e-3 {
            period
        } else {
            4.0
        };
        self
    }

    /// Override wobble amplitude (`>= 0`).
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.max(0.0);
        self
    }

    /// Override edge width / sharpness (both clamped positive).
    pub fn with_edge(mut self, edge_width: f32, sharpness: f32) -> Self {
        self.edge_width = if edge_width.is_finite() && edge_width > 1e-4 {
            edge_width
        } else {
            0.18
        };
        self.sharpness = if sharpness.is_finite() && sharpness > 0.0 {
            sharpness
        } else {
            3.0
        };
        self
    }

    /// Feature points (base positions, before time wobble).
    pub fn sites(&self) -> &[Point] {
        &self.sites
    }

    /// Current field time in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Wobble period in seconds.
    pub fn period(&self) -> f32 {
        self.period
    }

    /// Jump to an absolute field time (any finite value accepted).
    pub fn seek_time(&mut self, time: f32) {
        if time.is_finite() {
            self.time = time.max(0.0);
        }
    }

    /// Wobbled site positions at the current time.
    pub fn animated_sites(&self) -> Vec<Point> {
        let w = core::f32::consts::TAU / self.period;
        self.sites
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let phase = i as f32 * 2.39996; // golden-angle decorrelation
                Point::new(
                    s.x + self.amplitude * libm::cosf(w * self.time + phase),
                    s.y + self.amplitude * libm::sinf(w * self.time * 1.31 + phase * 1.7),
                )
            })
            .collect()
    }

    /// Intensity at `p` in `[0, 1]`: bright filaments on cell borders.
    pub fn sample(&self, p: Point) -> f32 {
        if self.sites.len() < 2 {
            return 0.0;
        }
        let sites = self.animated_sites();
        let mut f1 = f32::INFINITY;
        let mut f2 = f32::INFINITY;
        for &s in &sites {
            let d = p.distance(s);
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
        let edge = (f2 - f1) / self.edge_width;
        let glow = (1.0 - edge.clamp(0.0, 1.0)).max(0.0);
        libm::powf(glow, self.sharpness)
    }
}

impl Update for CausticField {
    fn update(&mut self, dt: f32) -> bool {
        if dt.is_finite() && dt > 0.0 {
            self.time += dt;
        }
        true // ambient field: never completes on its own
    }
}

impl Playable for CausticField {
    fn duration(&self) -> f32 {
        f32::INFINITY
    }

    fn reset(&mut self) {
        self.time = 0.0;
    }

    fn seek_to(&mut self, progress: f32) {
        self.time = progress.clamp(0.0, 1.0) * self.period;
    }

    fn is_complete(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point;

    fn deck() -> Vec<Point> {
        alloc::vec![
            Point::new(0.2, 0.2),
            Point::new(0.8, 0.2),
            Point::new(0.5, 0.6),
            Point::new(0.2, 0.9),
            Point::new(0.9, 0.85),
        ]
    }

    #[test]
    fn sample_is_bounded_and_seekable() {
        let mut field = CausticField::new(deck());
        for t in [0.0, 0.5, 1.0, 2.0, 3.7] {
            field.seek_time(t);
            for i in 0..10 {
                for j in 0..10 {
                    let v = field.sample(Point::new(i as f32 / 9.0, j as f32 / 9.0));
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "out of range {v} at t={t}"
                    );
                }
            }
        }
        // Pure function of time: re-seeking reproduces samples exactly.
        field.seek_time(1.25);
        let a: Vec<f32> = (0..16)
            .map(|k| field.sample(Point::new(k as f32 * 0.061, 0.4)))
            .collect();
        field.seek_time(0.0);
        field.seek_time(1.25);
        let b: Vec<f32> = (0..16)
            .map(|k| field.sample(Point::new(k as f32 * 0.061, 0.4)))
            .collect();
        assert_eq!(a, b);
    }

    #[test]
    fn time_animates_the_field() {
        let mut field = CausticField::new(deck());
        field.seek_time(0.0);
        let p = Point::new(0.45, 0.45);
        let v0 = field.sample(p);
        field.seek_time(1.0);
        let v1 = field.sample(p);
        assert!(
            (v0 - v1).abs() > 1e-4,
            "field static over time: {v0} vs {v1}"
        );
    }

    #[test]
    fn zero_amplitude_is_time_invariant() {
        let mut field = CausticField::new(deck()).with_amplitude(0.0);
        field.seek_time(0.0);
        let v0 = field.sample(Point::new(0.45, 0.45));
        field.seek_time(2.5);
        let v1 = field.sample(Point::new(0.45, 0.45));
        assert!((v0 - v1).abs() < 1e-6);
    }

    #[test]
    fn playable_contract_for_drivers() {
        let mut field = CausticField::new(deck());
        assert_eq!(field.time(), 0.0);
        assert!(field.update(0.5));
        assert!((field.time() - 0.5).abs() < 1e-6);
        assert!(!field.is_complete());
        Playable::seek_to(&mut field, 0.5);
        assert!((field.time() - 2.0).abs() < 1e-6); // period 4.0
        Playable::reset(&mut field);
        assert_eq!(field.time(), 0.0);
        assert!(Playable::as_any(&field).is::<CausticField>());
    }

    #[test]
    fn degenerate_site_sets_sample_zero() {
        let field = CausticField::new(Vec::new());
        assert_eq!(field.sample(Point::new(0.5, 0.5)), 0.0);
        let single = CausticField::new(alloc::vec![Point::new(0.5, 0.5)]);
        assert_eq!(single.sample(Point::new(0.5, 0.5)), 0.0);
    }
}
