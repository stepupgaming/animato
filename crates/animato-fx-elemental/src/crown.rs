//! Crown shard records and per-shard resolution for Glacial Crown.
//!
//! A [`CrownRecord`] stores **only what the dice decided** — a role, a bearing,
//! a unitless radial fraction and jitters — exactly like `GlacierAbility.js`.
//! Every metre, radian and second (except [`CrownRecord::erupt_time`], an
//! *event*) is resolved against the live
//! [`GlacialCrownParams`](crate::params::GlacialCrownParams) at sample time,
//! so editing [`GlacialCrownParams::zone_radius`] reshapes a standing crown
//! with the clock stopped.

extern crate alloc;

use alloc::vec::Vec;

use crate::math::{lerp, out_quint, saturate};
use crate::params::GlacialCrownParams;
use crate::rng::FxRng;

/// What a shard is for. Decides seat, height, lean and *when* it goes up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardRole {
    /// Wall of blades on the boundary.
    Ring,
    /// Broken shards banked against the ring's foot.
    Skirt,
    /// Spire in the middle (ships at `core_share = 0`).
    Core,
}

/// Dice-only record for one crown shard.
#[derive(Clone, Debug, PartialEq)]
pub struct CrownRecord {
    /// Ring / skirt / core.
    pub role: ShardRole,
    /// Bearing around the centre, radians.
    pub angle: f32,
    /// Unitless radial dice (`-1..1` ring, `0..1` skirt/core).
    pub radial: f32,
    /// Demoted to ankle-height wreckage.
    pub rubble: bool,
    /// Held back to push up during the hold (skirt only).
    pub late: bool,
    /// Unitless jitters in `[-1, 1]`.
    pub height_jitter: f32,
    /// Unitless jitters in `[-1, 1]`.
    pub radius_jitter: f32,
    /// Unitless jitters in `[-1, 1]`.
    pub lean_jitter: f32,
    /// Unitless fan splay jitter in `[-1, 1]`.
    pub fan_jitter: f32,
    /// Full-turn random spin (`0..TAU`).
    pub yaw: f32,
    /// `0..1` of stagger / shatter stagger.
    pub stagger: f32,
    /// Absolute age its eruption triggered at, or `-1` while buried.
    pub erupt_time: f32,
}

impl CrownRecord {
    /// `true` once the bloom wave has triggered this shard.
    pub fn erupted(&self) -> bool {
        self.erupt_time >= 0.0
    }
}

/// Resolved, render-ready sample for one shard at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrownSample {
    /// World x on the floor.
    pub x: f32,
    /// World z on the floor.
    pub z: f32,
    /// Base height offset from emergence / sink.
    pub y_base: f32,
    /// Full height in metres.
    pub height: f32,
    /// Base radius in metres.
    pub radius: f32,
    /// Lean angle away from centre, radians.
    pub lean_angle: f32,
    /// Fan splay off the radial, radians.
    pub fan_angle: f32,
    /// Yaw in radians.
    pub yaw: f32,
    /// `0 → 1` with springy overshoot; negative while buried.
    pub emergence: f32,
    /// Birth flash `1 → 0` over `birth_fade`.
    pub birth: f32,
    /// Freeze-front crystallisation `0..1` up the blade.
    pub growth: f32,
    /// Shatter progress `0..1` (fade phase only).
    pub shatter: f32,
    /// Whole-crown withdrawal `0..1` (fade phase only).
    pub retract: f32,
    /// Role this sample came from.
    pub role: ShardRole,
}

/// Curtain of cold air seated on the ring (renderer-owned mesh; CPU exposes
/// seat / scale / opacity so a standing crown re-scales under `zone_radius`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VeilSample {
    /// Centre x.
    pub x: f32,
    /// Centre y (half height).
    pub y: f32,
    /// Centre z.
    pub z: f32,
    /// Cylinder radius, metres.
    pub radius: f32,
    /// Cylinder height, metres.
    pub height: f32,
    /// Opacity `0..1`.
    pub opacity: f32,
    /// Yaw, radians.
    pub spin: f32,
}

/// Frozen sheet on the floor under the crown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSample {
    /// Centre x.
    pub x: f32,
    /// Centre z.
    pub z: f32,
    /// Footprint radius, metres.
    pub radius: f32,
    /// How far the freeze front has spread, `0..1`.
    pub freeze: f32,
    /// Whole-sheet opacity `0..1`.
    pub fade: f32,
    /// Hover height above the floor, metres.
    pub height: f32,
}

/// Shortest signed angle from `b` to `a`, radians, `-π..π`.
#[inline]
pub fn angle_delta(a: f32, b: f32) -> f32 {
    let d = a - b;
    libm::atan2f(libm::sinf(d), libm::cosf(d))
}

/// Roll the dice for one cast. Mirrors `GlacierAbility#onSpawn`.
pub fn roll_crown(params: &GlacialCrownParams, seed: u64) -> Vec<CrownRecord> {
    let mut rng = FxRng::new(seed);
    let wanted = params.spike_budget();
    let ring_count = libm::roundf(wanted as f32 * saturate(params.ring_share)).max(0.0) as usize;
    let core_count = libm::roundf(wanted as f32 * saturate(params.core_share)).max(0.0) as usize;
    let late_count = libm::roundf(wanted as f32 * saturate(params.late_share)).max(0.0) as usize;
    let tau = core::f32::consts::TAU;
    let mut out = Vec::with_capacity(wanted);

    for i in 0..wanted {
        let yaw = rng.range(0.0, tau);
        let stagger = rng.next_f32();
        let height_jitter = rng.range(-1.0, 1.0);
        let radius_jitter = rng.range(-1.0, 1.0);
        let lean_jitter = rng.range(-1.0, 1.0);
        let fan_jitter = rng.range(-1.0, 1.0);

        let (role, angle, radial, rubble, late) = if i < ring_count {
            let angle = ((i as f32 + rng.range(-0.4, 0.4)) / ring_count.max(1) as f32) * tau;
            (
                ShardRole::Ring,
                angle,
                rng.range(-1.0, 1.0),
                rng.next_f32() < params.rubble * 0.35,
                false,
            )
        } else if i < ring_count + core_count {
            (
                ShardRole::Core,
                rng.range(0.0, tau),
                libm::sqrtf(rng.next_f32()),
                false,
                false,
            )
        } else {
            let late = i >= wanted.saturating_sub(late_count);
            (
                ShardRole::Skirt,
                rng.range(0.0, tau),
                rng.next_f32(),
                rng.next_f32() < params.rubble,
                late,
            )
        };

        out.push(CrownRecord {
            role,
            angle,
            radial,
            rubble,
            late,
            height_jitter,
            radius_jitter,
            lean_jitter,
            fan_jitter,
            yaw,
            stagger,
            erupt_time: -1.0,
        });
    }
    out
}

/// Hand every shard the moment it goes up. Timestamps only — mirrors
/// `GlacierAbility#_scheduleEruption`. `age` is the cast age at impact.
pub fn schedule_eruption(
    records: &mut [CrownRecord],
    params: &GlacialCrownParams,
    age: f32,
    entry_angle: f32,
) {
    let hold = params.impact_duration();
    for record in records.iter_mut() {
        let delay = if record.late {
            hold * (0.12 + record.stagger * saturate(params.bloom_spread))
        } else if record.role == ShardRole::Ring {
            let around = libm::fabsf(angle_delta(record.angle, entry_angle)) / core::f32::consts::PI;
            around * params.sweep_time + record.stagger * params.stagger
        } else if record.role == ShardRole::Core {
            params.core_delay + record.stagger * params.stagger
        } else {
            params.skirt_delay
                + record.radial * params.skirt_wave
                + record.stagger * params.stagger
        };
        record.erupt_time = age + delay;
    }
}

/// Where a shard stands, at the live footprint.
pub fn shard_position(
    record: &CrownRecord,
    params: &GlacialCrownParams,
    center: [f32; 2],
) -> [f32; 2] {
    let r_zone = params.radius();
    let r = match record.role {
        ShardRole::Ring => r_zone * (params.ring_seat + record.radial * params.ring_scatter),
        ShardRole::Core => r_zone * params.core_spread * record.radial,
        ShardRole::Skirt => {
            r_zone
                * (params.skirt_seat
                    + libm::powf(record.radial, params.skirt_bias) * params.skirt_band)
        }
    };
    [
        center[0] + libm::cosf(record.angle) * r,
        center[1] + libm::sinf(record.angle) * r,
    ]
}

/// Full height of a shard, metres.
pub fn shard_height(record: &CrownRecord, params: &GlacialCrownParams, seed: f32) -> f32 {
    let mut h = match record.role {
        ShardRole::Ring => {
            let wave = libm::sinf(record.angle * 3.0 + seed) * 0.62
                + libm::sinf(record.angle * 5.0 - seed * 2.1) * 0.38;
            params.ring_height * (1.0 + params.ring_wave * wave)
        }
        ShardRole::Core => params.core_height * lerp(1.0, 0.55, record.radial),
        ShardRole::Skirt => {
            params.skirt_height
                * lerp(0.55, 1.2, 1.0 - libm::fabsf(record.radial - 0.5) * 2.0)
        }
    };
    h *= 1.0 + record.height_jitter * params.height_jitter * params.randomness;
    if record.rubble {
        h *= params.rubble_scale;
    }
    h.max(0.02)
}

/// Base radius of a shard, metres.
pub fn shard_radius(record: &CrownRecord, params: &GlacialCrownParams) -> f32 {
    let role = match record.role {
        ShardRole::Ring => 1.0,
        ShardRole::Core => 1.35,
        ShardRole::Skirt => 0.85,
    };
    let jitter = 1.0 + record.radius_jitter * params.radius_jitter * params.randomness;
    let rubble = if record.rubble { 1.3 } else { 1.0 };
    (params.crystal_radius * role * jitter * rubble).max(0.01)
}

/// How far a shard leans away from the middle, radians.
pub fn shard_lean(record: &CrownRecord, params: &GlacialCrownParams) -> f32 {
    let base = match record.role {
        ShardRole::Ring => params.ring_lean,
        ShardRole::Core => params.core_lean,
        ShardRole::Skirt => params.skirt_lean,
    };
    base * (1.0 + record.lean_jitter * params.lean_jitter * params.randomness)
}

/// Fan splay off the radial, radians.
pub fn shard_fan(record: &CrownRecord, params: &GlacialCrownParams) -> f32 {
    params.fan * record.fan_jitter
}

/// How far out of the ground a shard is (`GlacierAbility#_emergence`).
pub fn emergence(record: &CrownRecord, params: &GlacialCrownParams, age: f32) -> f32 {
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
    let after = elapsed - rise_time;
    let spring =
        libm::sinf(after * 14.0) * libm::expf(-after / params.settle.max(0.05));
    1.0 + params.rise_overshoot * spring
}

/// Freeze-front crystallisation up the blade (`GlacierAbility#_growth`).
pub fn growth(record: &CrownRecord, params: &GlacialCrownParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return 0.0;
    }
    let elapsed = age - record.erupt_time;
    saturate(elapsed / (params.rise_time * 0.85).max(0.02))
}

/// Shatter progress (`GlacierAbility#_shatterAmount`). `fade_time` is 0 outside
/// the fade phase.
pub fn shatter_amount(
    record: &CrownRecord,
    params: &GlacialCrownParams,
    fade_time: f32,
    fading: bool,
) -> f32 {
    if !fading {
        return 0.0;
    }
    let delay = params.shatter_delay + record.stagger * params.shatter_stagger;
    saturate((fade_time - delay) / params.sink_time.max(0.05))
}

/// Birth flash `1 → 0`.
pub fn birth_flash(record: &CrownRecord, params: &GlacialCrownParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return 0.0;
    }
    let elapsed = age - record.erupt_time;
    if elapsed < 0.0 {
        return 0.0;
    }
    1.0 - saturate(elapsed / params.birth_fade.max(0.02))
}

/// Resolve one shard against live params. Returns `None` while still buried.
pub fn sample_shard(
    record: &CrownRecord,
    params: &GlacialCrownParams,
    center: [f32; 2],
    seed: f32,
    age: f32,
    fade_time: f32,
    fading: bool,
    retract: f32,
) -> Option<CrownSample> {
    let emerge = emergence(record, params, age);
    if emerge < 0.0 {
        return None;
    }
    let pos = shard_position(record, params, center);
    let height = shard_height(record, params, seed);
    let shatter = shatter_amount(record, params, fade_time, fading);
    let sink = shatter * height * 0.9 + retract * height * 0.15;
    Some(CrownSample {
        x: pos[0],
        z: pos[1],
        y_base: (emerge - 1.0) * height * 0.85 - sink,
        height,
        radius: shard_radius(record, params),
        lean_angle: shard_lean(record, params),
        fan_angle: shard_fan(record, params),
        yaw: record.yaw * params.twist,
        emergence: emerge,
        birth: birth_flash(record, params, age),
        growth: growth(record, params, age),
        shatter,
        retract,
        role: record.role,
    })
}

/// Resolve every erupted shard.
pub fn sample_crown(
    records: &[CrownRecord],
    params: &GlacialCrownParams,
    center: [f32; 2],
    seed: f32,
    age: f32,
    fade_time: f32,
    fading: bool,
    retract: f32,
) -> Vec<CrownSample> {
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        if let Some(s) = sample_shard(
            record, params, center, seed, age, fade_time, fading, retract,
        ) {
            out.push(s);
        }
    }
    out
}

/// Sheet freeze / thaw amounts. `open` is `_openAmount`; `thaw` is `_thawAmount`.
pub fn sample_field(
    params: &GlacialCrownParams,
    center: [f32; 2],
    open: f32,
    thaw: f32,
    fade: f32,
) -> FieldSample {
    let freeze = saturate(open) * (1.0 - saturate(thaw));
    FieldSample {
        x: center[0],
        z: center[1],
        radius: params.radius(),
        freeze,
        fade: fade * freeze,
        height: params.field_height,
    }
}

/// Veil curtain sample. Hidden while travelling / fully thawed.
pub fn sample_veil(
    params: &GlacialCrownParams,
    center: [f32; 2],
    open: f32,
    thaw: f32,
    fade: f32,
    age: f32,
    seed: f32,
) -> Option<VeilSample> {
    if params.veil < 0.001 || open < 0.02 {
        return None;
    }
    let opacity = fade * (1.0 - thaw) * params.veil;
    if opacity < 0.004 {
        return None;
    }
    let height = (params.veil_height * crate::math::out_cubic(saturate(open))).max(0.05);
    Some(VeilSample {
        x: center[0],
        y: height * 0.5,
        z: center[1],
        radius: params.radius() * params.veil_radius,
        height,
        opacity,
        spin: seed + age * params.veil_spin * core::f32::consts::TAU,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_is_deterministic_and_budgeted() {
        let p = GlacialCrownParams::default();
        let a = roll_crown(&p, 7);
        let b = roll_crown(&p, 7);
        assert_eq!(a, b);
        assert_eq!(a.len(), p.spike_budget());
        let ring = a.iter().filter(|r| r.role == ShardRole::Ring).count();
        let expected_ring =
            libm::roundf(p.spike_budget() as f32 * saturate(p.ring_share)) as usize;
        assert_eq!(ring, expected_ring);
        assert!(a.iter().all(|r| r.erupt_time < 0.0));
    }

    #[test]
    fn zone_radius_scales_samples() {
        let mut p = GlacialCrownParams::default();
        let mut records = roll_crown(&p, 11);
        schedule_eruption(&mut records, &p, 0.5, 0.0);
        let center = [0.0, 12.0];
        let a = sample_crown(&records, &p, center, 7.0, 1.5, 0.0, false, 0.0);
        assert!(!a.is_empty());
        p.zone_radius *= 2.0;
        let b = sample_crown(&records, &p, center, 7.0, 1.5, 0.0, false, 0.0);
        assert_eq!(a.len(), b.len());
        let dist = |s: &CrownSample, c: [f32; 2]| {
            let dx = s.x - c[0];
            let dz = s.z - c[1];
            libm::sqrtf(dx * dx + dz * dz)
        };
        assert!(a.iter().zip(b.iter()).any(|(sa, sb)| {
            (dist(sb, center) - dist(sa, center)).abs() > 0.2
        }));
    }
}
