//! Kraken Crown tentacle records and per-arm resolution (Ext).
//!
//! A [`KrakenRecord`] stores **only what the dice decided** — a role, a
//! bearing, a splay and jitters — exactly like `KrakenAbility.js`. Every
//! metre, radian and second (except [`KrakenRecord::last_strike`], an
//! *event*) is resolved against the live
//! [`KrakenCrownParams`](crate::params::KrakenCrownParams) at sample time, so
//! editing [`KrakenCrownParams::zone_radius`] reshapes a standing crown with
//! the clock stopped. Arm length derives from the arc identity
//! `L = reach · (π/2) · R`.

extern crate alloc;

use alloc::vec::Vec;

use crate::math::{in_out_cubic, lerp, out_cubic, saturate};
use crate::params::KrakenCrownParams;
use crate::rng::FxRng;

/// What an arm is for. Decides seat, length, thickness and strike period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TentacleRole {
    /// Heavy limb on the boundary — does the hammering.
    Arm,
    /// Thin lashing cord between the arms.
    Whip,
}

/// Dice-only record for one tentacle.
#[derive(Clone, Debug, PartialEq)]
pub struct KrakenRecord {
    /// Arm or whip.
    pub role: TentacleRole,
    /// Bearing about the centre, radians.
    pub angle: f32,
    /// How far off the radius it strikes, `-1..1`.
    pub splay: f32,
    /// Seat radial jitter, `-1..1`.
    pub seat_jitter: f32,
    /// Length jitter, `-1..1`.
    pub length_jitter: f32,
    /// Thickness jitter, `-1..1`.
    pub thick_jitter: f32,
    /// Strike-turn jitter, `-1..1` (applied one-sided).
    pub turn_jitter: f32,
    /// Twist jitter, `-1..1`.
    pub twist_jitter: f32,
    /// Wave phase, `0..TAU`.
    pub wave_phase: f32,
    /// Wave amplitude jitter, `-1..1`.
    pub wave_jitter: f32,
    /// Where in the ring's rolling wave it strikes, `0..1`.
    pub cycle_phase: f32,
    /// `0..1` stagger / withdraw stagger.
    pub stagger: f32,
    /// Has this arm breached the floor yet.
    pub breached: bool,
    /// Strike timestamp it last threw an impact for, or `-1`.
    pub last_strike: f32,
}

/// Solved pose for one arm at the current clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KrakenPose {
    /// Total turn lean, radians.
    pub lean: f32,
    /// Curl along the arm, radians.
    pub curl: f32,
    /// Travelling-wave amplitude.
    pub wave: f32,
    /// Section twist, radians.
    pub twist: f32,
    /// Landing flash `1 → 0`.
    pub flash: f32,
    /// Thickness squash multiplier.
    pub squash: f32,
}

impl Default for KrakenPose {
    fn default() -> Self {
        Self {
            lean: 0.0,
            curl: 0.0,
            wave: 0.0,
            twist: 0.0,
            flash: 0.0,
            squash: 1.0,
        }
    }
}

/// Current strike schedule answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrikeSchedule {
    /// Local time (since arm delay) when the current strike lands.
    pub time: f32,
    /// `true` when this landing is the synchronised finale.
    pub finale: bool,
    /// Wind-up compression `0..1` into the time left before finale.
    pub wind: f32,
}

/// Resolved, render-ready sample for one tentacle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KrakenSample {
    /// Seat world x on the floor.
    pub x: f32,
    /// Seat world z on the floor.
    pub z: f32,
    /// Tip / impact world x (closed-form arc).
    pub tip_x: f32,
    /// Tip / impact world y (above floor).
    pub tip_y: f32,
    /// Tip / impact world z.
    pub tip_z: f32,
    /// Drawn length, metres.
    pub length: f32,
    /// Base radius, metres.
    pub thickness: f32,
    /// Lean angle, radians.
    pub lean: f32,
    /// Curl, radians.
    pub curl: f32,
    /// Wave amplitude.
    pub wave: f32,
    /// Wave phase (advances with age).
    pub wave_phase: f32,
    /// Wave frequency.
    pub wave_freq: f32,
    /// Twist, radians.
    pub twist: f32,
    /// Emergence `0..1` (reveal).
    pub emerge: f32,
    /// Landing flash `0..1`.
    pub flash: f32,
    /// Withdrawal sink into the rift, metres fraction.
    pub sink: f32,
    /// Yaw so +X points at the middle, radians.
    pub bearing: f32,
    /// Role this sample came from.
    pub role: TentacleRole,
    /// `true` when this sample's strike just landed this frame window.
    pub striking: bool,
    /// `true` when the current strike is the finale.
    pub finale: bool,
}

/// Abyss rift on the floor under the crown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbyssFieldSample {
    /// Centre x.
    pub x: f32,
    /// Centre z.
    pub z: f32,
    /// Footprint radius, metres.
    pub radius: f32,
    /// Quad half-extent, metres.
    pub quad_size: f32,
    /// How far the rift has torn, `0..1`.
    pub open: f32,
    /// Whole-sheet opacity `0..1`.
    pub fade: f32,
    /// Hover height above the floor, metres.
    pub height: f32,
    /// Cast seed for shader variation.
    pub seed: f32,
}

/// Curtain of spray hanging over the rim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrineVeilSample {
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

/// Shortest signed angle from `b` to `a`, radians, `-π..π`.
#[inline]
pub fn kraken_angle_delta(a: f32, b: f32) -> f32 {
    let d = a - b;
    libm::atan2f(libm::sinf(d), libm::cosf(d))
}

/// Roll the dice for one cast. Mirrors `KrakenAbility#onSpawn`.
pub fn roll_kraken(params: &KrakenCrownParams, seed: u64) -> Vec<KrakenRecord> {
    let mut rng = FxRng::new(seed);
    let (arm_count, whip_count) = params.arm_budget();
    let wanted = arm_count + whip_count;
    let tau = core::f32::consts::TAU;
    let mut out = Vec::with_capacity(wanted);

    for i in 0..wanted {
        let is_arm = i < arm_count;
        let count = if is_arm {
            arm_count.max(1)
        } else {
            whip_count.max(1)
        };
        let index = if is_arm { i } else { i - arm_count };
        let offset = if is_arm { 0.0 } else { 0.5 };
        let angle = ((index as f32 + offset + rng.range(-0.3, 0.3)) / count as f32) * tau;

        out.push(KrakenRecord {
            role: if is_arm {
                TentacleRole::Arm
            } else {
                TentacleRole::Whip
            },
            angle,
            splay: rng.range(-1.0, 1.0),
            seat_jitter: rng.range(-1.0, 1.0),
            length_jitter: rng.range(-1.0, 1.0),
            thick_jitter: rng.range(-1.0, 1.0),
            turn_jitter: rng.range(-1.0, 1.0),
            twist_jitter: rng.range(-1.0, 1.0),
            wave_phase: rng.range(0.0, tau),
            wave_jitter: rng.range(-1.0, 1.0),
            cycle_phase: rng.next_f32(),
            stagger: rng.next_f32(),
            breached: false,
            last_strike: -1.0,
        });
    }
    out
}

/// Seconds after the rift opens before this arm starts coming out.
pub fn arm_delay(record: &KrakenRecord, params: &KrakenCrownParams, entry_angle: f32) -> f32 {
    let around = libm::fabsf(kraken_angle_delta(record.angle, entry_angle)) / core::f32::consts::PI;
    let role_delay = if record.role == TentacleRole::Arm {
        0.0
    } else {
        params.whip_delay
    };
    around * params.sweep_time + record.stagger * params.stagger + role_delay
}

/// How long one full rear-whip-press-peel cycle takes.
pub fn cycle_period(record: &KrakenRecord, params: &KrakenCrownParams) -> f32 {
    let scale = if record.role == TentacleRole::Arm {
        1.0
    } else {
        params.whip_period
    };
    (params.smash_period * scale).max(0.25)
}

/// How far the four beats must be squeezed to fit the period.
pub fn cycle_fit(params: &KrakenCrownParams, period: f32) -> f32 {
    let beats = params.rear_time + params.strike_time + params.hold_time + params.peel_time;
    if beats > 1e-4 {
        (period / beats).min(1.0)
    } else {
        1.0
    }
}

/// Seat radius from centre, metres.
pub fn seat_radius(record: &KrakenRecord, params: &KrakenCrownParams) -> f32 {
    let r = params.radius();
    match record.role {
        TentacleRole::Arm => r * (params.ring_seat + record.seat_jitter * params.ring_scatter),
        TentacleRole::Whip => r * (params.whip_seat + record.seat_jitter * params.whip_scatter),
    }
}

/// Bearing the arm bends along (inward + splay).
pub fn bend_bearing(record: &KrakenRecord, params: &KrakenCrownParams) -> f32 {
    record.angle + core::f32::consts::PI + record.splay * params.splay
}

/// Arm length from the arc identity, metres.
pub fn arm_length(record: &KrakenRecord, params: &KrakenCrownParams) -> f32 {
    let base = params.radius() * (core::f32::consts::PI / 2.0) * params.reach;
    let role_scale = if record.role == TentacleRole::Arm {
        1.0
    } else {
        params.whip_length
    };
    (base
        * role_scale
        * (1.0 + record.length_jitter * params.length_jitter * params.randomness))
    .max(0.2)
}

/// Base radius where the arm leaves the rift, metres.
pub fn arm_thickness(record: &KrakenRecord, params: &KrakenCrownParams) -> f32 {
    let role_scale = if record.role == TentacleRole::Arm {
        1.0
    } else {
        params.whip_thickness
    };
    (params.thickness
        * role_scale
        * (1.0 + record.thick_jitter * params.thickness_jitter * params.randomness))
    .max(0.01)
}

/// Total turn of this arm's strike, radians (≥ authored turn).
pub fn strike_turn(record: &KrakenRecord, params: &KrakenCrownParams) -> f32 {
    params.strike_turn * (1.0 + libm::fabsf(record.turn_jitter) * params.turn_jitter)
}

/// Closed-form tip / impact point in world space.
pub fn strike_point(
    record: &KrakenRecord,
    params: &KrakenCrownParams,
    centre: [f32; 2],
) -> [f32; 3] {
    let seat = seat_radius(record, params);
    let bearing = bend_bearing(record, params);
    let length = arm_length(record, params);
    let turn = strike_turn(record, params).max(0.05);
    let along = (length * (1.0 - libm::cosf(turn))) / turn;
    let up = (length * libm::sinf(turn)) / turn;
    [
        centre[0] + libm::cosf(record.angle) * seat + libm::cosf(bearing) * along,
        up.max(0.0),
        centre[1] + libm::sinf(record.angle) * seat + libm::sinf(bearing) * along,
    ]
}

/// When this arm's current strike lands (written like `KrakenAbility#_strikeTime`).
pub fn strike_schedule(
    record: &KrakenRecord,
    params: &KrakenCrownParams,
    local: f32,
    delay: f32,
    impact_duration: f32,
) -> StrikeSchedule {
    let period = cycle_period(record, params);
    let fit = cycle_fit(params, period);
    let rise = params.rise_time.max(0.05);
    let tail = (params.hold_time + params.peel_time) * fit;
    let windup = (params.rear_time + params.strike_time) * fit;

    let first = rise + windup + record.cycle_phase * period * saturate(params.cycle_scatter);
    let finale = impact_duration - params.finale_lead - delay;

    if finale > 0.0 {
        let n = libm::floorf((finale - tail - windup - first) / period);
        let handover = if n >= 0.0 {
            first + n * period + tail
        } else {
            rise
        };
        if local >= handover {
            return StrikeSchedule {
                time: finale,
                finale: true,
                wind: saturate(
                    (finale - handover).max(0.02) / windup.max(0.02),
                ),
            };
        }
    }

    let time = if local < first {
        first
    } else {
        let current = first + libm::floorf((local - first) / period) * period;
        if local < current + tail {
            current
        } else {
            current + period
        }
    };
    StrikeSchedule {
        time,
        finale: false,
        wind: 1.0,
    }
}

/// Solve the arm's pose for this frame (`KrakenAbility#_solvePose`).
pub fn solve_pose(
    record: &KrakenRecord,
    params: &KrakenCrownParams,
    d: f32,
    emerge: f32,
    fit: f32,
    wind: f32,
) -> KrakenPose {
    let wave = params.wave_idle * (1.0 + record.wave_jitter * 0.4);

    if emerge < 1.0 {
        let x = out_cubic(emerge);
        return KrakenPose {
            lean: lerp(params.coil_lean, params.idle_lean, x),
            curl: lerp(params.coil_curl, params.idle_curl, x),
            wave: lerp(params.wave_coil, wave, x),
            twist: params.twist * (1.0 + record.twist_jitter * 0.5) * lerp(1.6, 1.0, x),
            flash: 0.0,
            squash: lerp(1.35, 1.0, x),
        };
    }

    let mut pose = KrakenPose {
        lean: 0.0,
        curl: 0.0,
        wave: 0.0,
        twist: params.twist * (1.0 + record.twist_jitter * 0.5),
        flash: 0.0,
        squash: 1.0,
    };

    let budget = (params.rear_time + params.strike_time) * fit * wind;
    let strike_dur = (params.strike_time * fit)
        .min(budget * 0.6)
        .max(0.02);
    let rear_dur = (budget - strike_dur).max(0.02);
    let hold_dur = (params.hold_time * fit).max(0.02);
    let peel_dur = (params.peel_time * fit).max(0.02);
    let turn = strike_turn(record, params);

    if d < -(rear_dur + strike_dur) {
        pose.lean = params.idle_lean;
        pose.curl = params.idle_curl;
        pose.wave = wave;
    } else if d < -strike_dur {
        let x = out_cubic(saturate((d + rear_dur + strike_dur) / rear_dur));
        pose.lean = lerp(params.idle_lean, params.rear_lean, x);
        pose.curl = lerp(params.idle_curl, params.rear_curl, x);
        pose.wave = lerp(wave, params.wave_rear, x);
    } else if d < 0.0 {
        let x = saturate(1.0 + d / strike_dur);
        let e = x * x;
        pose.lean = lerp(params.rear_lean, turn, e);
        pose.curl = lerp(params.rear_curl, 0.0, e);
        pose.wave = lerp(params.wave_rear, params.wave_strike, e);
        pose.squash = 1.0 + e * params.strike_squash;
    } else if d < hold_dur {
        let x = saturate(d / hold_dur);
        let ring = libm::expf(-x * 5.0)
            * libm::sinf(d * params.settle_speed)
            * params.settle;
        pose.lean = turn;
        pose.curl = ring;
        pose.wave = params.wave_strike;
        pose.flash = 1.0 - crate::math::out_quad(x);
        pose.squash = 1.0 + params.strike_squash * (1.0 - x) * 0.6;
    } else {
        let x = in_out_cubic(saturate((d - hold_dur) / peel_dur));
        pose.lean = lerp(turn, params.idle_lean, x);
        pose.curl = lerp(0.0, params.idle_curl, x);
        pose.wave = lerp(params.wave_strike, wave, x);
    }
    pose
}

/// Resolve one tentacle against live params / clock.
#[allow(clippy::too_many_arguments)]
pub fn sample_tentacle(
    record: &KrakenRecord,
    params: &KrakenCrownParams,
    centre: [f32; 2],
    open_time: f32,
    age: f32,
    fade_time: f32,
    withdrawing: bool,
    entry_angle: f32,
    impact_duration: f32,
) -> Option<KrakenSample> {
    let delay = arm_delay(record, params, entry_angle);
    let local = open_time - delay;
    if local < 0.0 {
        return None;
    }
    let rise = params.rise_time.max(0.05);
    let emerge = saturate(local / rise);
    let period = cycle_period(record, params);
    let fit = cycle_fit(params, period);
    let solved = strike_schedule(record, params, local, delay, impact_duration);
    let d = local - solved.time;
    let pose = solve_pose(record, params, d, emerge, fit, solved.wind);

    let mut retract = 0.0;
    if withdrawing {
        let start = params.withdraw_delay + record.stagger * params.withdraw_stagger;
        retract = in_out_cubic(saturate(
            (fade_time - start) / params.withdraw_time.max(0.05),
        ));
    }

    let length = arm_length(record, params) * (1.0 - retract * 0.96);
    let reveal = emerge * (1.0 - retract * 0.15);
    let thickness = arm_thickness(record, params) * pose.squash * (1.0 - retract * 0.3);
    let seat = seat_radius(record, params);
    let bearing = bend_bearing(record, params);
    let tip = strike_point(record, params, centre);
    let sink = retract * params.withdraw_sink;
    let striking = d >= 0.0 && emerge >= 1.0 && record.last_strike != solved.time;

    let tau = core::f32::consts::TAU;
    Some(KrakenSample {
        x: centre[0] + libm::cosf(record.angle) * seat,
        z: centre[1] + libm::sinf(record.angle) * seat,
        tip_x: tip[0],
        tip_y: tip[1],
        tip_z: tip[2],
        length,
        thickness,
        lean: pose.lean,
        curl: pose.curl,
        wave: pose.wave,
        wave_phase: record.wave_phase - age * params.wave_speed * tau,
        wave_freq: (params.wave_freq * (1.0 + record.wave_jitter * 0.25)).max(0.1),
        twist: pose.twist,
        emerge: reveal,
        flash: pose.flash,
        sink,
        bearing,
        role: record.role,
        striking,
        finale: solved.finale && striking,
    })
}

/// Resolve every active tentacle.
#[allow(clippy::too_many_arguments)]
pub fn sample_kraken(
    records: &[KrakenRecord],
    params: &KrakenCrownParams,
    centre: [f32; 2],
    open_time: f32,
    age: f32,
    fade_time: f32,
    withdrawing: bool,
    entry_angle: f32,
    impact_duration: f32,
) -> Vec<KrakenSample> {
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        if let Some(s) = sample_tentacle(
            record,
            params,
            centre,
            open_time,
            age,
            fade_time,
            withdrawing,
            entry_angle,
            impact_duration,
        ) {
            if s.emerge > 0.001 || s.length > 0.01 {
                out.push(s);
            }
        }
    }
    out
}

/// Abyss rift sample.
pub fn sample_abyss_field(
    params: &KrakenCrownParams,
    centre: [f32; 2],
    open: f32,
    close: f32,
    fade: f32,
    seed: f32,
) -> AbyssFieldSample {
    let radius = params.radius();
    let spread = open * (1.0 - close);
    AbyssFieldSample {
        x: centre[0],
        z: centre[1],
        radius,
        quad_size: (radius + params.field_boundary + 0.8) * 2.0,
        open: spread,
        fade: fade * if spread > 0.002 { 1.0 } else { 0.0 },
        height: params.field_height,
        seed,
    }
}

/// Brine veil sample.
pub fn sample_brine_veil(
    params: &KrakenCrownParams,
    centre: [f32; 2],
    open: f32,
    close: f32,
    fade: f32,
    age: f32,
    seed: f32,
) -> Option<BrineVeilSample> {
    if params.veil < 0.001 || open < 0.02 {
        return None;
    }
    let opacity = fade * (1.0 - close) * params.veil;
    if opacity < 0.004 {
        return None;
    }
    let veil_height = (params.veil_height * out_cubic(open)).max(0.05);
    let radius = params.radius() * params.veil_radius;
    let tau = core::f32::consts::TAU;
    Some(BrineVeilSample {
        x: centre[0],
        y: veil_height * 0.5,
        z: centre[1],
        radius,
        height: veil_height,
        opacity,
        spin: seed + age * params.veil_spin * tau,
    })
}

/// Advance strike / breach event flags on records for the current clock.
///
/// Returns `(smash_count, finale_hit, breach_count)` newly observed this call.
pub fn tick_arm_events(
    records: &mut [KrakenRecord],
    params: &KrakenCrownParams,
    open_time: f32,
    entry_angle: f32,
    impact_duration: f32,
) -> (usize, bool, usize) {
    let mut smash = 0usize;
    let mut finale = false;
    let mut breach = 0usize;
    let rise = params.rise_time.max(0.05);
    for record in records.iter_mut() {
        let delay = arm_delay(record, params, entry_angle);
        let local = open_time - delay;
        if local < 0.0 {
            continue;
        }
        let emerge = saturate(local / rise);
        if !record.breached && emerge > 0.15 {
            record.breached = true;
            breach += 1;
        }
        let solved = strike_schedule(record, params, local, delay, impact_duration);
        let d = local - solved.time;
        if d >= 0.0 && record.last_strike != solved.time && emerge >= 1.0 {
            record.last_strike = solved.time;
            smash += 1;
            if solved.finale {
                finale = true;
            }
        }
    }
    (smash, finale, breach)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_is_seed_deterministic() {
        let p = KrakenCrownParams::default();
        let a = roll_kraken(&p, 7);
        let b = roll_kraken(&p, 7);
        assert_eq!(a, b);
        assert_eq!(a.len(), 21);
        assert!(a.iter().any(|r| r.role == TentacleRole::Arm));
        assert!(a.iter().any(|r| r.role == TentacleRole::Whip));
    }

    #[test]
    fn length_scales_with_zone_radius() {
        let mut p = KrakenCrownParams::default();
        let rec = roll_kraken(&p, 3)[0].clone();
        let a = arm_length(&rec, &p);
        p.zone_radius *= 2.0;
        let b = arm_length(&rec, &p);
        assert!((b - a * 2.0).abs() < 1e-3);
    }

    #[test]
    fn strike_point_lands_near_centre_at_pi() {
        let p = KrakenCrownParams::default();
        let mut rec = roll_kraken(&p, 1)[0].clone();
        rec.role = TentacleRole::Arm;
        rec.splay = 0.0;
        rec.turn_jitter = 0.0;
        rec.length_jitter = 0.0;
        rec.seat_jitter = 0.0;
        // With reach≈1 and turn=π, tip sits near centre.
        let tip = strike_point(&rec, &p, [0.0, 0.0]);
        let dist = libm::sqrtf(tip[0] * tip[0] + tip[2] * tip[2]);
        assert!(dist < p.radius() * 0.35, "tip dist {dist}");
        assert!(tip[1] < 0.5);
    }
}
