//! Spike records and per-spike resolution.
//!
//! A [`SpikeRecord`] stores **only what the dice decided** — position
//! fractions along/across the cast line plus unitless jitters — exactly like
//! `IceAbility.js`. Every metre, radian and second is resolved against the
//! live [`FrostLanceParams`](crate::params::FrostLanceParams) by the functions
//! in this module, so param edits re-shape a standing field with the clock
//! stopped.

use crate::math::{in_cubic, lerp, out_quint, saturate, smoothstep};
use crate::params::FrostLanceParams;
use crate::rng::FxRng;

/// Dice-only record for one crystal. No metres, radians or seconds — except
/// [`SpikeRecord::erupt_time`], which is an *event* (the moment its own
/// eruption triggered), mirroring the original.
#[derive(Clone, Debug, PartialEq)]
pub struct SpikeRecord {
    /// `0..1` down the cast line (`1` for impact-cluster members).
    pub along: f32,
    /// `-1..1` across the band, before clumping (line members only).
    pub lateral: f32,
    /// `-1..1` extra lateral jitter.
    pub scatter: f32,
    /// Impact-cluster bearing, radians.
    pub angle: f32,
    /// Impact-cluster distance, `0..1` (sqrt-distributed for even density).
    pub radial: f32,
    /// Held back for the cluster at the far end.
    pub impact: bool,
    /// Demoted to ankle-height wreckage.
    pub rubble: bool,
    /// Unitless jitters in `[-1, 1]`.
    pub height_jitter: f32,
    /// Unitless jitters in `[-1, 1]`.
    pub radius_jitter: f32,
    /// Unitless jitters in `[-1, 1]`.
    pub lean_jitter: f32,
    /// Full-turn random spin (`0..TAU`).
    pub yaw: f32,
    /// `0..1` of `rise_stagger`.
    pub stagger: f32,
    /// Absolute age its eruption triggered at, or `-1` while buried.
    pub erupt_time: f32,
}

impl SpikeRecord {
    /// `true` once the fracture front has triggered this spike.
    pub fn erupted(&self) -> bool {
        self.erupt_time >= 0.0
    }
}

/// Resolved, render-ready sample for one spike at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpikeSample {
    /// World x on the floor (flat floor at `y = 0`, like the original).
    pub x: f32,
    /// World z on the floor.
    pub z: f32,
    /// Base height offset: `(emerge - 1) * height * 0.85` minus sink.
    pub y_base: f32,
    /// Full height in metres.
    pub height: f32,
    /// Base radius in metres.
    pub radius: f32,
    /// Lean angle in radians (about the axis perpendicular to the lean dir).
    pub lean_angle: f32,
    /// Yaw in radians (`yaw * twist`).
    pub yaw: f32,
    /// `0 → 1` eruption progress with springy overshoot past `1`;
    /// negative while still buried.
    pub emergence: f32,
    /// Birth flash `1 → 0` over `birth_fade`.
    pub birth: f32,
    /// Whole-field withdrawal `0..1` (fade phase only).
    pub retract: f32,
}

/// Roll the dice for one cast: `wanted` records, the last `impact_count` held
/// back for the terminal cluster. Mirrors `IceAbility#onSpawn`.
pub fn roll_spikes(
    params: &FrostLanceParams,
    wanted: usize,
    impact_count: usize,
    seed: u64,
) -> alloc::vec::Vec<SpikeRecord> {
    let mut rng = FxRng::new(seed);
    let impact_start = wanted.saturating_sub(impact_count);
    let mut out = alloc::vec::Vec::with_capacity(wanted);
    for i in 0..wanted {
        let impact = i >= impact_start;
        let (along, lateral, angle, radial) = if impact {
            (
                1.0,
                0.0,
                rng.range(0.0, core::f32::consts::TAU),
                // sqrt keeps the cluster evenly dense, not piled in the middle.
                libm::sqrtf(rng.next_f32()),
            )
        } else {
            // `front_bias` < 1 crowds the field toward the impact point.
            let t = (i as f32 + rng.next_f32()) / core::cmp::max(1, impact_start) as f32;
            (libm::powf(t, params.front_bias), rng.range(-1.0, 1.0), 0.0, 0.0)
        };
        out.push(SpikeRecord {
            along,
            lateral,
            scatter: rng.range(-1.0, 1.0),
            angle,
            radial,
            impact,
            rubble: rng.next_f32() < params.rubble,
            height_jitter: rng.range(-1.0, 1.0),
            radius_jitter: rng.range(-1.0, 1.0),
            lean_jitter: rng.range(-1.0, 1.0),
            yaw: rng.range(0.0, core::f32::consts::TAU),
            stagger: rng.next_f32(),
            erupt_time: -1.0,
        });
    }
    out
}

/// Half-width of the band at `s` along the line, metres.
pub fn half_width(s: f32, params: &FrostLanceParams) -> f32 {
    lerp(
        params.width_near,
        params.width,
        libm::powf(saturate(s), params.width_curve),
    )
}

/// Signed lateral offset as a fraction of the local half-width.
pub fn lateral_norm(record: &SpikeRecord, params: &FrostLanceParams) -> f32 {
    let raw = record.lateral;
    let sign = if raw < 0.0 { -1.0 } else { 1.0 };
    sign * libm::powf(libm::fabsf(raw), params.clumping) + record.scatter * params.scatter
}

/// Full height of a spike in metres.
pub fn spike_height(record: &SpikeRecord, params: &FrostLanceParams) -> f32 {
    let mut h = lerp(
        params.height_near,
        params.height,
        libm::powf(saturate(record.along), params.height_curve),
    );
    // The swell at the impact point.
    h *= 1.0 + (params.peak - 1.0) * smoothstep(1.0 - params.peak_width, 1.0, record.along);
    // Domed silhouette: flank blades are shorter than the spine.
    let edge = if record.impact {
        record.radial
    } else {
        saturate(libm::fabsf(lateral_norm(record, params)))
    };
    h *= lerp(1.0, 1.0 - saturate(params.crown), libm::powf(edge, 1.4));
    h *= 1.0 + record.height_jitter * params.height_jitter * params.randomness;
    if record.rubble {
        h *= params.rubble_scale;
    }
    h.max(0.02)
}

/// Base radius of a spike in metres.
pub fn spike_radius(record: &SpikeRecord, params: &FrostLanceParams) -> f32 {
    let grow = lerp(0.72, 1.15, libm::powf(saturate(record.along), 0.6));
    let jitter = 1.0 + record.radius_jitter * params.radius_jitter * params.randomness;
    (params.radius * grow * jitter * if record.rubble { 1.25 } else { 1.0 }).max(0.01)
}

/// How far out of the ground a spike is: `0 → 1` with springy overshoot past
/// `1`; negative while buried/waiting. Mirrors `IceAbility#_emergence`.
pub fn emergence(record: &SpikeRecord, params: &FrostLanceParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return -1.0;
    }
    let elapsed = age - record.erupt_time;
    if elapsed < 0.0 {
        return -1.0;
    }
    let rise_time = params.rise_time.max(0.02);
    let rise = out_quint(saturate(elapsed / rise_time));
    if elapsed <= rise_time {
        return rise;
    }
    // The punch-through carries past full height and settles back.
    let after = elapsed - rise_time;
    let spring = libm::sinf(after * 14.0) * libm::expf(-after / params.settle.max(0.05));
    1.0 + params.rise_overshoot * spring
}

/// Birth flash `1 → 0` over `birth_fade`; `0` while buried.
pub fn birth(record: &SpikeRecord, params: &FrostLanceParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return 0.0;
    }
    saturate(1.0 - (age - record.erupt_time) / params.birth_fade.max(0.02))
}

/// Resolve one spike against the live params.
///
/// `origin`/`direction`/`side` describe the cast line on the floor plane;
/// `length` is the cast distance in metres.
pub fn sample_spike(
    record: &SpikeRecord,
    params: &FrostLanceParams,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    age: f32,
    retract: f32,
) -> SpikeSample {
    let height = spike_height(record, params);
    let radius = spike_radius(record, params);
    let emerge = emergence(record, params, age);

    // Position at the live footprint settings.
    let (x, z) = if record.impact {
        let reach = half_width(1.0, params) * 1.25 * record.radial;
        (
            origin[0] + direction[0] * length + libm::cosf(record.angle) * reach,
            origin[1] + direction[1] * length + libm::sinf(record.angle) * reach,
        )
    } else {
        let lateral = lateral_norm(record, params) * half_width(record.along, params);
        (
            origin[0] + direction[0] * record.along * length + side[0] * lateral,
            origin[1] + direction[1] * record.along * length + side[1] * lateral,
        )
    };

    // Lean: away from the caster, and outward across the band.
    let outward = if record.impact {
        (if libm::cosf(record.angle) < 0.0 {
            -1.0
        } else {
            1.0
        }) * record.radial
    } else {
        lateral_norm(record, params)
    };
    // Blend upright growth with outward lean (mirrors IceAbility.js lean mix).
    let lean_mix = 0.75 + outward * 0.85;
    let lean_angle = params.lean
        * lean_mix
        * (0.35 + 0.65 * record.along)
        * (1.0 + record.lean_jitter * params.lean_jitter * params.randomness);

    let mut y_base = (emerge - 1.0) * height * 0.85;
    if retract > 0.0 {
        let sink = in_cubic(retract);
        y_base -= sink * (height + radius + 0.4);
    }

    SpikeSample {
        x,
        z,
        y_base,
        height,
        radius,
        lean_angle,
        yaw: record.yaw * params.twist,
        emergence: emerge,
        birth: birth(record, params, age),
        retract,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::saturate;

    fn params() -> FrostLanceParams {
        FrostLanceParams::default()
    }

    #[test]
    fn roll_splits_line_and_impact() {
        let p = params();
        let spikes = roll_spikes(&p, 190, 42, 1234);
        assert_eq!(spikes.len(), 190);
        let (line, cluster) = (&spikes[..148], &spikes[148..]);
        assert!(line.iter().all(|r| !r.impact));
        assert!(cluster.iter().all(|r| r.impact));
        assert!(line.iter().all(|r| (0.0..=1.0).contains(&r.along)));
        assert!(cluster.iter().all(|r| r.along == 1.0));
        assert!(cluster.iter().all(|r| (0.0..=1.0).contains(&r.radial)));
        assert!(spikes.iter().all(|r| r.erupt_time < 0.0));
    }

    #[test]
    fn roll_is_seed_deterministic() {
        let p = params();
        assert_eq!(
            roll_spikes(&p, 64, 14, 99),
            roll_spikes(&p, 64, 14, 99)
        );
        assert_ne!(
            roll_spikes(&p, 64, 14, 99),
            roll_spikes(&p, 64, 14, 100)
        );
    }

    #[test]
    fn half_width_interpolates_with_curve() {
        let p = params();
        assert!((half_width(0.0, &p) - p.width_near).abs() < 1e-6);
        assert!((half_width(1.0, &p) - p.width).abs() < 1e-6);
        let mid = half_width(0.5, &p);
        assert!(mid > p.width_near && mid < p.width);
    }

    #[test]
    fn heights_dome_and_peak() {
        let p = params();
        let spine = SpikeRecord {
            along: 0.99,
            lateral: 0.0,
            scatter: 0.0,
            angle: 0.0,
            radial: 0.0,
            impact: false,
            rubble: false,
            height_jitter: 0.0,
            radius_jitter: 0.0,
            lean_jitter: 0.0,
            yaw: 0.0,
            stagger: 0.0,
            erupt_time: 0.0,
        };
        let flank = SpikeRecord {
            lateral: 1.0,
            ..spine.clone()
        };
        assert!(spike_height(&spine, &p) > spike_height(&flank, &p));
        // Rubble demotion shrinks below the 2 cm floor guard proportionally.
        let rubble = SpikeRecord {
            rubble: true,
            ..spine.clone()
        };
        assert!(spike_height(&rubble, &p) < spike_height(&spine, &p));
        assert!(spike_height(&rubble, &p) >= 0.02);
        assert!(spike_radius(&rubble, &p) >= 0.01);
    }

    #[test]
    fn emergence_rises_then_overshoots() {
        let p = params();
        let mut r = SpikeRecord {
            along: 0.5,
            lateral: 0.0,
            scatter: 0.0,
            angle: 0.0,
            radial: 0.0,
            impact: false,
            rubble: false,
            height_jitter: 0.0,
            radius_jitter: 0.0,
            lean_jitter: 0.0,
            yaw: 0.0,
            stagger: 0.0,
            erupt_time: 1.0,
        };
        assert_eq!(emergence(&r, &p, 0.5), -1.0);
        assert_eq!(emergence(&r, &p, 1.0), 0.0);
        let mid = emergence(&r, &p, 1.0 + p.rise_time * 0.5);
        assert!((0.0..1.0).contains(&mid));
        assert!((emergence(&r, &p, 1.0 + p.rise_time) - 1.0).abs() < 1e-5);
        // Springy overshoot carries past 1 shortly after the rise.
        let over = emergence(&r, &p, 1.0 + p.rise_time + 0.05);
        assert!(over > 1.0);
        assert!(saturate(birth(&r, &p, 1.0)) == 1.0);
        assert_eq!(birth(&r, &p, 1.0 + p.birth_fade + 1.0), 0.0);
        r.erupt_time = -1.0;
        assert_eq!(birth(&r, &p, 5.0), 0.0);
    }
}
