//! Line-cast targeting math.
//!
//! Ports the distance handling of `AimController.js`: the pointer is
//! raycast onto the ground plane every frame, the raw distance is clamped
//! into the ability's reach, and aiming nearer than `min_range` marks the
//! cast invalid (the red arrow) and refuses it. No DOM, no Three.js.

use crate::math::clamp;

/// Reach envelope shared by every line-cast ability (`FrostLanceParams`,
/// [`StormLanceParams`](crate::params::StormLanceParams), …).
pub trait AimReach {
    /// Maximum cast distance, metres.
    fn cast_range(&self) -> f32;
    /// Casts nearer than this are refused.
    fn cast_min_range(&self) -> f32;
}

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
///
/// ```rust
/// use animato_fx_elemental::{FrostLanceParams, solve_aim};
///
/// let params = FrostLanceParams::default();
/// let near = solve_aim(&params, [0.0, 0.0], [0.0, 1.0], 1.0);
/// assert!(!near.valid); // nearer than `min_range`: the red-arrow state
/// assert_eq!(near.distance, params.min_range);
///
/// let far = solve_aim(&params, [0.0, 0.0], [1.0, 0.0], 99.0);
/// assert!(far.valid);
/// assert_eq!(far.distance, params.range);
/// ```
pub fn solve_aim(
    params: &impl AimReach,
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
    let min_range = params.cast_min_range();
    let range = params.cast_range();
    let valid = raw_distance >= min_range;
    let distance = clamp(
        raw_distance,
        0.2f32.max(min_range),
        0.4f32.max(range),
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
    use crate::params::FrostLanceParams;

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

/// Zone-cast reach envelope (`VoltaicSnareParams`, …).
///
/// Far casts aim a circle at a floor point rather than a line. Upstream's
/// `AimController` still measures a distance from the caster and clamps it,
/// but this port **refuses** aims outside `cast_range` (in addition to the
/// usual `min_range` refusal) so the planted centre is never silently
/// pulled inward.
pub trait ZoneAimReach: AimReach {
    /// Footprint radius the circle indicator measures out, metres.
    fn zone_radius(&self) -> f32;
}

/// Solved zone-cast target: origin, planted centre, unit heading, distance
/// and footprint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneAimSolution {
    /// Cast origin on the floor (`y = 0` plane).
    pub origin: [f32; 2],
    /// Planted trap centre on the floor.
    pub center: [f32; 2],
    /// Unit heading from origin toward the centre.
    pub direction: [f32; 2],
    /// Distance used for the leash travel, metres (clamped when valid).
    pub distance: f32,
    /// Raw requested distance before clamping.
    pub raw_distance: f32,
    /// Live footprint radius (`zone_radius`), metres.
    pub zone_radius: f32,
    /// `false` when nearer than `min_range` **or** farther than `range`.
    pub valid: bool,
    /// `true` when `raw_distance > range`.
    pub too_far: bool,
    /// `true` when `raw_distance < min_range`.
    pub too_close: bool,
}

/// Solve a zone-cast aim from origin, heading and raw pointer distance.
///
/// ```text
/// too_close = raw < min_range
/// too_far   = raw > range
/// valid     = !too_close && !too_far
/// distance  = clamp(raw, max(0.2, min_range), max(0.4, range))   // when valid
/// center    = origin + direction * distance
/// ```
pub fn solve_zone_aim(
    params: &impl ZoneAimReach,
    origin: [f32; 2],
    direction: [f32; 2],
    raw_distance: f32,
) -> ZoneAimSolution {
    let len = libm::sqrtf(direction[0] * direction[0] + direction[1] * direction[1]);
    let direction = if len > 1e-6 {
        [direction[0] / len, direction[1] / len]
    } else {
        [0.0, 1.0]
    };
    let min_range = params.cast_min_range();
    let range = params.cast_range();
    let too_close = raw_distance < min_range;
    let too_far = raw_distance > range;
    let valid = !too_close && !too_far;
    let distance = if valid {
        clamp(
            raw_distance,
            0.2f32.max(min_range),
            0.4f32.max(range),
        )
    } else {
        // Still report a clamped distance for indicator feedback.
        clamp(
            raw_distance,
            0.2f32.max(min_range),
            0.4f32.max(range),
        )
    };
    let center = [
        origin[0] + direction[0] * distance,
        origin[1] + direction[1] * distance,
    ];
    ZoneAimSolution {
        origin: [origin[0], origin[1]],
        center,
        direction,
        distance,
        raw_distance,
        zone_radius: params.zone_radius().max(0.05),
        valid,
        too_far,
        too_close,
    }
}

/// Solve a zone-cast aim from an absolute floor aim-point.
pub fn solve_zone_aim_at(
    params: &impl ZoneAimReach,
    origin: [f32; 2],
    aim_point: [f32; 2],
) -> ZoneAimSolution {
    let dx = aim_point[0] - origin[0];
    let dz = aim_point[1] - origin[1];
    let raw = libm::sqrtf(dx * dx + dz * dz);
    let direction = if raw > 1e-6 {
        [dx / raw, dz / raw]
    } else {
        [0.0, 1.0]
    };
    solve_zone_aim(params, origin, direction, raw)
}

#[cfg(test)]
mod zone_tests {
    use super::*;
    use crate::params::VoltaicSnareParams;

    #[test]
    fn zone_refuses_too_far_and_too_close() {
        let p = VoltaicSnareParams::default();
        // Default min_range is 0, so plant a temporary floor.
        let mut tight = p.clone();
        tight.min_range = 3.0;
        let near = solve_zone_aim(&tight, [0.0, 0.0], [0.0, 1.0], 1.0);
        assert!(!near.valid);
        assert!(near.too_close);
        assert!(!near.too_far);

        let far = solve_zone_aim(&p, [0.0, 0.0], [0.0, 1.0], 99.0);
        assert!(!far.valid);
        assert!(far.too_far);
        assert!(!far.too_close);

        let mid = solve_zone_aim(&p, [0.0, 0.0], [0.0, 1.0], 12.0);
        assert!(mid.valid);
        assert!((mid.center[1] - 12.0).abs() < 1e-4);
        assert!((mid.zone_radius - 4.4).abs() < 1e-4);
    }

    #[test]
    fn zone_aim_at_matches_dir_form() {
        let p = VoltaicSnareParams::default();
        let a = solve_zone_aim_at(&p, [1.0, 2.0], [1.0, 14.0]);
        let b = solve_zone_aim(&p, [1.0, 2.0], [0.0, 1.0], 12.0);
        assert_eq!(a.valid, b.valid);
        assert!((a.center[0] - b.center[0]).abs() < 1e-4);
        assert!((a.center[1] - b.center[1]).abs() < 1e-4);
    }
}
