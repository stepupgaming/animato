//! Line-cast targeting math.
//!
//! Ports the distance handling of `AimController.js`: the pointer is
//! raycast onto the ground plane every frame, the raw distance is clamped
//! into the ability's reach, and aiming nearer than `min_range` marks the
//! cast invalid (the red arrow) and refuses it. No DOM, no Three.js.

use crate::math::clamp;
use crate::params::FrostLanceParams;

/// Solved line-cast target: origin on the floor, unit heading, distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimSolution {
    /// Cast origin on the floor (`y = 0` plane).
    pub origin: [f32; 2],
    /// Unit heading on the floor plane.
    pub direction: [f32; 2],
    /// Clamped cast distance in metres.
    pub distance: f32,
    /// Raw requested distance before clamping.
    pub raw_distance: f32,
    /// `false` while the pointer is nearer than `min_range`.
    pub valid: bool,
}

/// Solve a line-cast aim from a floor-plane origin, a heading and a raw
/// pointer distance. Mirrors `AimController#update` clamping:
///
/// ```text
/// valid    = raw >= min_range
/// distance = clamp(raw, max(0.2, min_range), max(0.4, range))
/// ```
pub fn solve_aim(
    params: &FrostLanceParams,
    origin: [f32; 2],
    direction: [f32; 2],
    raw_distance: f32,
) -> AimSolution {
    let len = libm::sqrtf(direction[0] * direction[0] + direction[1] * direction[1]);
    let direction = if len > 1e-6 {
        [direction[0] / len, direction[1] / len]
    } else {
        [0.0, 1.0]
    };
    let valid = raw_distance >= params.min_range;
    let distance = clamp(
        raw_distance,
        0.2f32.max(params.min_range),
        0.4f32.max(params.range),
    );
    AimSolution {
        origin: [origin[0], origin[1]],
        direction,
        distance,
        raw_distance,
        valid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_into_reach_and_flags_min_range() {
        let p = FrostLanceParams::default();
        let near = solve_aim(&p, [0.0, 0.0], [0.0, 1.0], 1.0);
        assert!(!near.valid);
        assert_eq!(near.distance, p.min_range);

        let far = solve_aim(&p, [0.0, 0.0], [1.0, 0.0], 99.0);
        assert!(far.valid);
        assert_eq!(far.distance, p.range);

        let mid = solve_aim(&p, [1.0, 2.0], [0.0, 2.0], 8.0);
        assert!(mid.valid);
        assert_eq!(mid.distance, 8.0);
        assert_eq!(mid.direction, [0.0, 1.0]);
    }

    #[test]
    fn degenerate_heading_falls_back_to_north() {
        let p = FrostLanceParams::default();
        let s = solve_aim(&p, [0.0, 0.0], [0.0, 0.0], 5.0);
        assert_eq!(s.direction, [0.0, 1.0]);
        assert!(s.valid);
    }
}
