//! Pyre Crown blade records and per-blade resolution (Ext).
//!
//! A [`PyreRecord`] stores **only what the dice decided** — a role, a bearing,
//! a unitless radial fraction and jitters — exactly like `PyreAbility.js`.
//! Every metre, radian and second (except [`PyreRecord::erupt_time`], an
//! *event*) is resolved against the live
//! [`PyreCrownParams`](crate::params::PyreCrownParams) at sample time, so
//! editing [`PyreCrownParams::zone_radius`] reshapes a standing crown with the
//! clock stopped.
//!
//! **Deliberate divergence from Glacial Crown:** emergence is strictly
//! monotonic (surge + creep, no overshoot spring), and collapse is burn-out
//! (`char`) sweeping back around the ring rather than shatter.

extern crate alloc;

use alloc::vec::Vec;

use crate::math::{in_cubic, lerp, out_cubic, out_expo, saturate};
use crate::params::PyreCrownParams;
use crate::rng::FxRng;

/// What a blade is for. Decides seat, height, lean and *when* it catches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BladeRole {
    /// Wall of fire-blades on the boundary.
    Ring,
    /// Burning wreckage banked against the ring's foot.
    Skirt,
    /// Pyre in the middle (ships at `core_share = 0`).
    Core,
}

/// Dice-only record for one pyre blade.
#[derive(Clone, Debug, PartialEq)]
pub struct PyreRecord {
    /// Ring / skirt / core.
    pub role: BladeRole,
    /// Bearing around the centre, radians.
    pub angle: f32,
    /// Unitless radial dice (`-1..1` ring, `0..1` skirt/core).
    pub radial: f32,
    /// Demoted to ankle-height wreckage.
    pub rubble: bool,
    /// Held back to catch during the blaze (skirt only).
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
    /// `0..1` of stagger / burn stagger.
    pub stagger: f32,
    /// Absolute age its eruption triggered at, or `-1` while buried.
    pub erupt_time: f32,
}

impl PyreRecord {
    /// `true` once the bloom wave has triggered this blade.
    pub fn erupted(&self) -> bool {
        self.erupt_time >= 0.0
    }
}

/// Resolved, render-ready sample for one blade at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PyreSample {
    /// World x on the floor.
    pub x: f32,
    /// World z on the floor.
    pub z: f32,
    /// Base height offset from emergence / sink.
    pub y_base: f32,
    /// Drawn height in metres (includes creep lengthening).
    pub height: f32,
    /// Base radius in metres.
    pub radius: f32,
    /// Lean angle, radians.
    pub lean_angle: f32,
    /// Fan splay off the radial, radians.
    pub fan_angle: f32,
    /// Yaw in radians.
    pub yaw: f32,
    /// Emergence: `-1` buried, `0..1` rising, `>1` creeping.
    pub emergence: f32,
    /// Birth flash `1 → 0` over `birth_fade`.
    pub birth: f32,
    /// Ignition climb `0..1` up the blade.
    pub ignition: f32,
    /// Burn-down progress `0..1` (fade phase only).
    pub char: f32,
    /// Role this sample came from.
    pub role: BladeRole,
}

/// Molten crater on the floor under the crown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmberFieldSample {
    /// Centre x.
    pub x: f32,
    /// Centre z.
    pub z: f32,
    /// Footprint radius, metres.
    pub radius: f32,
    /// How far the burn has spread, `0..1`.
    pub burn: f32,
    /// Whole-sheet opacity `0..1`.
    pub fade: f32,
    /// Hover height above the floor, metres.
    pub height: f32,
}

/// Wall of flame seated on the ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlameVeilSample {
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

/// Heat-haze proxy (renderer writes into a distortion buffer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatHazeSample {
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
    /// Strength / fade `0..1`.
    pub fade: f32,
}

/// Shortest signed angle from `b` to `a`, radians, `-π..π`.
#[inline]
pub fn pyre_angle_delta(a: f32, b: f32) -> f32 {
    let d = a - b;
    libm::atan2f(libm::sinf(d), libm::cosf(d))
}

/// Roll the dice for one cast. Mirrors `PyreAbility#onSpawn`.
pub fn roll_pyre(params: &PyreCrownParams, seed: u64) -> Vec<PyreRecord> {
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
                BladeRole::Ring,
                angle,
                rng.range(-1.0, 1.0),
                rng.next_f32() < params.rubble * 0.35,
                false,
            )
        } else if i < ring_count + core_count {
            (
                BladeRole::Core,
                rng.range(0.0, tau),
                libm::sqrtf(rng.next_f32()),
                false,
                false,
            )
        } else {
            let late = i >= wanted.saturating_sub(late_count);
            (
                BladeRole::Skirt,
                rng.range(0.0, tau),
                rng.next_f32(),
                rng.next_f32() < params.rubble,
                late,
            )
        };

        out.push(PyreRecord {
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

/// Hand every blade the moment it catches. Timestamps only — mirrors
/// `PyreAbility#_scheduleEruption`. `age` is the cast age at impact.
pub fn schedule_pyre_eruption(
    records: &mut [PyreRecord],
    params: &PyreCrownParams,
    age: f32,
    entry_angle: f32,
) {
    let hold = params.impact_duration();
    for record in records.iter_mut() {
        let delay = if record.late {
            hold * (0.12 + record.stagger * saturate(params.bloom_spread))
        } else if record.role == BladeRole::Ring {
            let around =
                libm::fabsf(pyre_angle_delta(record.angle, entry_angle)) / core::f32::consts::PI;
            around * params.sweep_time + record.stagger * params.stagger
        } else if record.role == BladeRole::Core {
            params.core_delay + record.stagger * params.stagger
        } else {
            params.skirt_delay
                + record.radial * params.skirt_wave
                + record.stagger * params.stagger
        };
        record.erupt_time = age + delay;
    }
}

/// Where a blade stands, at the live footprint.
pub fn blade_position(
    record: &PyreRecord,
    params: &PyreCrownParams,
    center: [f32; 2],
) -> [f32; 2] {
    let r_zone = params.radius();
    let r = match record.role {
        BladeRole::Ring => r_zone * (params.ring_seat + record.radial * params.ring_scatter),
        BladeRole::Core => r_zone * params.core_spread * record.radial,
        BladeRole::Skirt => {
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

/// Full height of a blade, metres (before creep lengthening).
pub fn blade_height(record: &PyreRecord, params: &PyreCrownParams, seed: f32) -> f32 {
    let mut h = match record.role {
        BladeRole::Ring => {
            let wave = libm::sinf(record.angle * 3.0 + seed) * 0.62
                + libm::sinf(record.angle * 5.0 - seed * 2.1) * 0.38;
            params.ring_height * (1.0 + params.ring_wave * wave)
        }
        BladeRole::Core => params.core_height * lerp(1.0, 0.55, record.radial),
        BladeRole::Skirt => {
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

/// Base radius of a blade, metres.
pub fn blade_radius(record: &PyreRecord, params: &PyreCrownParams) -> f32 {
    let role = match record.role {
        BladeRole::Ring => 1.0,
        BladeRole::Core => 1.35,
        BladeRole::Skirt => 0.85,
    };
    let jitter = 1.0 + record.radius_jitter * params.radius_jitter * params.randomness;
    let rubble = if record.rubble { 1.35 } else { 1.0 };
    (params.blade_radius * role * jitter * rubble).max(0.01)
}

/// How far a blade leans, radians.
pub fn blade_lean(record: &PyreRecord, params: &PyreCrownParams) -> f32 {
    let base = match record.role {
        BladeRole::Ring => params.ring_lean,
        BladeRole::Core => params.core_lean,
        BladeRole::Skirt => params.skirt_lean,
    };
    base * (1.0 + record.lean_jitter * params.lean_jitter * params.randomness)
}

/// Fan splay off the radial, radians.
pub fn blade_fan(record: &PyreRecord, params: &PyreCrownParams) -> f32 {
    params.fan * record.fan_jitter
}

/// Monotonic emergence (`PyreAbility#_emergence`): surge then creep — never a
/// spring overshoot.
pub fn pyre_emergence(record: &PyreRecord, params: &PyreCrownParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return -1.0;
    }
    let elapsed = age - record.erupt_time;
    if elapsed < 0.0 {
        return -1.0;
    }
    let rise_time = params.rise_time.max(0.02);
    if elapsed <= rise_time {
        let x = saturate(elapsed / rise_time);
        return lerp(out_cubic(x), out_expo(x), saturate(params.rise_snap));
    }
    let after = elapsed - rise_time;
    1.0 + params.creep * (1.0 - libm::expf(-after / params.creep_time.max(0.05)))
}

/// How far the fire has climbed the blade (`PyreAbility#_ignition`).
pub fn ignition(record: &PyreRecord, params: &PyreCrownParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return 0.0;
    }
    let elapsed = age - record.erupt_time;
    saturate(elapsed / (params.rise_time * 0.8).max(0.02))
}

/// Burn-down progress (`PyreAbility#_charAmount`). Sweep runs *back* the way
/// the bloom came for ring blades.
pub fn char_amount(
    record: &PyreRecord,
    params: &PyreCrownParams,
    fade_time: f32,
    fading: bool,
    entry_angle: f32,
) -> f32 {
    if !fading {
        return 0.0;
    }
    let mut delay = params.burn_delay + record.stagger * params.burn_stagger;
    if record.role == BladeRole::Ring {
        let around =
            libm::fabsf(pyre_angle_delta(record.angle, entry_angle)) / core::f32::consts::PI;
        delay += (1.0 - around) * params.burn_sweep;
    }
    saturate((fade_time - delay) / params.ash_time.max(0.05))
}

/// Birth flash `1 → 0`.
pub fn pyre_birth_flash(record: &PyreRecord, params: &PyreCrownParams, age: f32) -> f32 {
    if record.erupt_time < 0.0 {
        return 0.0;
    }
    let elapsed = age - record.erupt_time;
    if elapsed < 0.0 {
        return 0.0;
    }
    1.0 - saturate(elapsed / params.birth_fade.max(0.02))
}

/// Resolve one blade against live params. Returns `None` while still buried.
pub fn sample_blade(
    record: &PyreRecord,
    params: &PyreCrownParams,
    center: [f32; 2],
    seed: f32,
    age: f32,
    fade_time: f32,
    fading: bool,
    entry_angle: f32,
) -> Option<PyreSample> {
    let emerge = pyre_emergence(record, params, age);
    if emerge < 0.0 {
        return None;
    }
    let pos = blade_position(record, params, center);
    let base_height = blade_height(record, params, seed);
    let char = char_amount(record, params, fade_time, fading, entry_angle);
    let surfaced = emerge.min(1.0);
    let creep = (emerge - 1.0).max(0.0);
    let drawn = base_height * (1.0 + creep);
    let sink = in_cubic(char) * drawn * params.sink;
    Some(PyreSample {
        x: pos[0],
        z: pos[1],
        y_base: (surfaced - 1.0) * drawn * 0.92 - sink,
        height: drawn * lerp(0.82, 1.0, surfaced),
        radius: blade_radius(record, params),
        lean_angle: blade_lean(record, params),
        fan_angle: blade_fan(record, params),
        yaw: record.yaw * params.twist,
        emergence: emerge,
        birth: pyre_birth_flash(record, params, age),
        ignition: ignition(record, params, age),
        char,
        role: record.role,
    })
}

/// Resolve every erupted blade.
pub fn sample_pyre(
    records: &[PyreRecord],
    params: &PyreCrownParams,
    center: [f32; 2],
    seed: f32,
    age: f32,
    fade_time: f32,
    fading: bool,
    entry_angle: f32,
) -> Vec<PyreSample> {
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        if let Some(s) = sample_blade(
            record,
            params,
            center,
            seed,
            age,
            fade_time,
            fading,
            entry_angle,
        ) {
            out.push(s);
        }
    }
    out
}

/// Crater burn / cool amounts. `open` is `_openAmount`; `cool` is `_coolAmount`.
pub fn sample_ember_field(
    params: &PyreCrownParams,
    center: [f32; 2],
    open: f32,
    cool: f32,
    fade: f32,
) -> EmberFieldSample {
    let burn = saturate(open) * (1.0 - saturate(cool));
    EmberFieldSample {
        x: center[0],
        z: center[1],
        radius: params.radius(),
        burn,
        fade: fade * burn,
        height: params.field_height,
    }
}

/// Flame-veil sample. Hidden while travelling / fully cooled.
pub fn sample_flame_veil(
    params: &PyreCrownParams,
    center: [f32; 2],
    open: f32,
    cool: f32,
    fade: f32,
    age: f32,
    seed: f32,
) -> Option<FlameVeilSample> {
    if params.veil < 0.001 || open < 0.02 {
        return None;
    }
    let opacity = fade * (1.0 - cool) * params.veil;
    if opacity < 0.004 {
        return None;
    }
    let height = (params.veil_height * out_cubic(saturate(open))).max(0.05);
    Some(FlameVeilSample {
        x: center[0],
        y: height * 0.5,
        z: center[1],
        radius: params.radius() * params.veil_radius,
        height,
        opacity,
        spin: seed + age * params.veil_spin * core::f32::consts::TAU,
    })
}

/// Heat-haze sample (`haze` ships at 0).
pub fn sample_heat_haze(
    params: &PyreCrownParams,
    center: [f32; 2],
    open: f32,
    cool: f32,
    fade: f32,
) -> Option<HeatHazeSample> {
    if params.haze < 0.001 || open < 0.02 {
        return None;
    }
    let haze_fade = fade * (1.0 - cool * 0.7) * params.haze;
    if haze_fade < 0.004 {
        return None;
    }
    let height = (params.haze_height * out_cubic(saturate(open))).max(0.05);
    Some(HeatHazeSample {
        x: center[0],
        y: height * 0.5,
        z: center[1],
        radius: params.radius() * params.haze_radius,
        height,
        fade: haze_fade,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_is_deterministic_and_budgeted() {
        let p = PyreCrownParams::default();
        let a = roll_pyre(&p, 7);
        let b = roll_pyre(&p, 7);
        assert_eq!(a, b);
        assert_eq!(a.len(), p.spike_budget());
        let ring = a.iter().filter(|r| r.role == BladeRole::Ring).count();
        let expected_ring =
            libm::roundf(p.spike_budget() as f32 * saturate(p.ring_share)) as usize;
        assert_eq!(ring, expected_ring);
        assert!(a.iter().all(|r| r.erupt_time < 0.0));
        assert_eq!(a.iter().filter(|r| r.role == BladeRole::Core).count(), 0);
    }

    #[test]
    fn emergence_is_monotonic_no_overshoot_spring() {
        let p = PyreCrownParams::default();
        let mut records = roll_pyre(&p, 11);
        schedule_pyre_eruption(&mut records, &p, 0.5, 0.0);
        let r = &records[0];
        assert!(r.erupt_time >= 0.0);
        let mut prev = -1.0f32;
        for i in 0..40 {
            let age = r.erupt_time + i as f32 * 0.02;
            let e = pyre_emergence(r, &p, age);
            if e >= 0.0 {
                assert!(e + 1e-5 >= prev, "emergence must not turn around");
                prev = e;
            }
        }
        // After rise_time, still creeping toward 1+creep from below — never a
        // damped spring bounce below the previous peak after overshoot.
        let at_rise = pyre_emergence(r, &p, r.erupt_time + p.rise_time);
        assert!((at_rise - 1.0).abs() < 1e-3);
        let later = pyre_emergence(r, &p, r.erupt_time + p.rise_time + 0.5);
        assert!(later > 1.0 && later <= 1.0 + p.creep + 1e-3);
    }

    #[test]
    fn zone_radius_scales_samples() {
        let mut p = PyreCrownParams::default();
        let mut records = roll_pyre(&p, 11);
        schedule_pyre_eruption(&mut records, &p, 0.5, 0.0);
        let center = [0.0, 12.0];
        let a = sample_pyre(&records, &p, center, 7.0, 1.5, 0.0, false, 0.0);
        assert!(!a.is_empty());
        p.zone_radius *= 2.0;
        let b = sample_pyre(&records, &p, center, 7.0, 1.5, 0.0, false, 0.0);
        assert_eq!(a.len(), b.len());
        let dist = |s: &PyreSample, c: [f32; 2]| {
            let dx = s.x - c[0];
            let dz = s.z - c[1];
            libm::sqrtf(dx * dx + dz * dz)
        };
        assert!(a.iter().zip(b.iter()).any(|(sa, sb)| {
            (dist(sb, center) - dist(sa, center)).abs() > 0.2
        }));
    }
}
