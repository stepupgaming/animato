//! Dice-only cage filament records and live sampling for Voltaic Snare.
//!
//! Ports the CPU-facing half of `SnareAbility.js` + `SnareMaterial.js`: a cast
//! captures **dice-only** filament records (role + index + fan); every metre of
//! path, kink and width resolves against live [`VoltaicSnareParams`] at sample
//! time — including the footprint [`VoltaicSnareParams::zone_radius`], so
//! editing the radius on a standing trap reshapes the cage with the clock
//! stopped. Restrike re-rolls the per-filament shape seed from
//! `(cast_seed, index, floor(age * restrike))`.

extern crate alloc;

use alloc::vec::Vec;

use crate::math::{hash11, lerp, out_cubic, saturate, smoothstep};
use crate::params::{CAGE_NODES, VoltaicSnareParams};
use crate::rng::FxRng;

const TAU: f32 = core::f32::consts::TAU;
const PI: f32 = core::f32::consts::PI;

/// Which parametric path a cage filament follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilamentRole {
    /// Whip from the hand to the travelling tip (travel phase only).
    Leash,
    /// Twisting climb out of the planted centre.
    Column,
    /// Ground meander running out to the boundary.
    Tendril,
    /// Arc hopping around the rim.
    Rim,
}

/// Dice-only record for one cage filament.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CageRecord {
    /// Role this filament wears.
    pub role: FilamentRole,
    /// Index inside its role (`0..budget`).
    pub index: u16,
    /// `0..1` fan fraction within the role.
    pub fan: f32,
}

/// One node on a resolved cage filament polyline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CageNode {
    /// World x.
    pub x: f32,
    /// World y (height above the floor).
    pub y: f32,
    /// World z.
    pub z: f32,
    /// `0..1` along the filament.
    pub t: f32,
    /// Half-width hint in metres.
    pub half_width: f32,
}

/// Resolved, render-ready sample for one cage filament.
#[derive(Clone, Debug, PartialEq)]
pub struct CageSample {
    /// Role.
    pub role: FilamentRole,
    /// Index inside the role.
    pub index: u16,
    /// Fan fraction.
    pub fan: f32,
    /// Per-filament blink `0..1`.
    pub flash: f32,
    /// Role dim multiplier.
    pub dim: f32,
    /// Polyline nodes.
    pub nodes: Vec<CageNode>,
}

/// Live sampling context (metres resolved from the standing cast).
#[derive(Clone, Copy, Debug)]
pub struct CageContext {
    /// Planted centre on the floor (`x`, `z`).
    pub center: [f32; 2],
    /// Hand origin in world space.
    pub hand: [f32; 3],
    /// Travelling leash tip in world space.
    pub front: [f32; 3],
    /// Live footprint radius (open-scaled), metres.
    pub radius: f32,
    /// Live column height (climb-scaled), metres.
    pub height: f32,
    /// Cast age in seconds.
    pub age: f32,
    /// Cast seed as `f32`.
    pub seed: f32,
    /// Whole-cage fade `0..1`.
    pub fade: f32,
}

/// Roll dice for one cast: leash + column + tendril + rim records.
pub fn roll_cage(params: &VoltaicSnareParams, seed: u64) -> Vec<CageRecord> {
    let mut rng = FxRng::new(seed ^ 0xCA6E_5A17);
    let mut out = Vec::with_capacity(
        params.leash_budget()
            + params.column_budget()
            + params.tendril_budget()
            + params.rim_budget(),
    );
    push_role(&mut out, &mut rng, FilamentRole::Leash, params.leash_budget());
    push_role(&mut out, &mut rng, FilamentRole::Column, params.column_budget());
    push_role(&mut out, &mut rng, FilamentRole::Tendril, params.tendril_budget());
    push_role(&mut out, &mut rng, FilamentRole::Rim, params.rim_budget());
    let _ = rng.next_u64();
    out
}

fn push_role(out: &mut Vec<CageRecord>, rng: &mut FxRng, role: FilamentRole, n: usize) {
    if n == 0 {
        return;
    }
    let denom = n as f32;
    for i in 0..n {
        let fan = (i as f32 + 0.5 + (rng.next_f32() - 0.5) * 0.15) / denom;
        out.push(CageRecord {
            role,
            index: i as u16,
            fan: fan.clamp(0.0, 1.0 - 1e-4),
        });
    }
}

/// Snap-open amount `0..~1.16` (overshoot then settle).
pub fn open_amount(open_time: f32, params: &VoltaicSnareParams) -> f32 {
    let snap = params.snap_duration();
    let t = saturate(open_time / snap);
    let bump = libm::sinf(PI * libm::powf(t, 1.7));
    out_cubic(t) * (1.0 + 0.16 * bump)
}

/// Column climb amount `0..1` — slower off the mark than the ring.
pub fn climb_amount(open_time: f32, params: &VoltaicSnareParams) -> f32 {
    let snap = params.snap_duration() * 1.7;
    out_cubic(saturate(open_time / snap))
}

/// Restrike tick: `floor(age * restrike)`.
pub fn restrike_tick(age: f32, params: &VoltaicSnareParams) -> f32 {
    libm::floorf(age * params.restrike.max(0.01))
}

/// Whole-cage brightness stutter (quantised).
pub fn cage_flicker(age: f32, cast_seed: f32, params: &VoltaicSnareParams) -> f32 {
    let step = libm::floorf(age * params.flicker_speed.max(1.0));
    let noise = hash11(step + cast_seed * 0.13);
    1.0 - saturate(params.flicker) * noise
}

#[inline]
fn vnoise(x: f32, seed: f32) -> f32 {
    let i = libm::floorf(x);
    let f = x - i;
    lerp(hash11(i + seed), hash11(i + 1.0 + seed), f) * 2.0 - 1.0
}

fn kink(t: f32, seed: f32, span: f32, params: &VoltaicSnareParams, age: f32) -> [f32; 2] {
    let mut o = [0.0f32, 0.0f32];
    let mut amp = 1.0f32;
    let mut freq = params.jitter_scale.max(0.01) * span;
    let mut scroll = age * params.crawl;
    let octaves = params.octave_count();
    for i in 0..5u32 {
        let on = if i < octaves { 1.0 } else { 0.0 };
        o[0] += on * amp * vnoise(t * freq + scroll, seed + 13.0 * i as f32);
        o[1] += on * amp * vnoise(t * freq + scroll * 1.17, seed + 71.3 + 13.0 * i as f32);
        amp *= params.jitter_falloff;
        freq *= 2.0;
        scroll *= 1.63;
    }
    o
}

fn filament_seed(record: &CageRecord, cast_seed: f32, tick: f32) -> f32 {
    hash11(record.index as f32 * 7.13 + cast_seed + tick * 3.77) * 97.0
}

fn path_at(
    role: FilamentRole,
    fan: f32,
    seed: f32,
    t: f32,
    params: &VoltaicSnareParams,
    ctx: &CageContext,
) -> [f32; 3] {
    match role {
        FilamentRole::Leash => {
            let x = lerp(ctx.hand[0], ctx.front[0], t);
            let y = lerp(ctx.hand[1], ctx.front[1], t);
            let z = lerp(ctx.hand[2], ctx.front[2], t);
            let y = (y + params.leash_sag * libm::sinf(t * PI)).max(params.leash_cling);
            [x, y, z]
        }
        FilamentRole::Column => {
            let a = fan * TAU + (t * params.column_twist + ctx.age * params.column_spin) * TAU;
            let r = ctx.radius
                * (lerp(
                    params.throat,
                    params.column_spread,
                    libm::powf(t, params.column_curve.max(0.01)),
                ) + params.column_flare * smoothstep(0.72, 1.0, t));
            let y = libm::powf(t, params.height_curve.max(0.01)) * ctx.height;
            [
                ctx.center[0] + libm::cosf(a) * r,
                y,
                ctx.center[1] + libm::sinf(a) * r,
            ]
        }
        FilamentRole::Tendril => {
            let veer = (hash11(seed + 5.0) - 0.5) * 2.0 * params.tendril_wander;
            let a = fan * TAU
                + ctx.age * params.tendril_spin * TAU
                + hash11(seed) * 0.4
                + veer * libm::powf(t, 1.4);
            let r = ctx.radius
                * lerp(
                    params.tendril_inner,
                    params.tendril_reach,
                    libm::powf(t, params.tendril_curve.max(0.01)),
                );
            let y = params.tendril_hug + params.tendril_arch * libm::sinf(t * PI);
            [
                ctx.center[0] + libm::cosf(a) * r,
                y,
                ctx.center[1] + libm::sinf(a) * r,
            ]
        }
        FilamentRole::Rim => {
            let a = (fan + ctx.age * params.rim_speed) * TAU
                + hash11(seed) * 0.3
                + t * params.rim_span * TAU;
            let r = ctx.radius * (1.0 + params.rim_jitter * 0.25 * libm::sinf(t * 6.0 + seed));
            let y = params.tendril_hug + params.rim_height * libm::sinf(t * PI);
            [
                ctx.center[0] + libm::cosf(a) * r,
                y,
                ctx.center[1] + libm::sinf(a) * r,
            ]
        }
    }
}

fn role_amp_width_span_dim(
    role: FilamentRole,
    params: &VoltaicSnareParams,
    ctx: &CageContext,
) -> (f32, f32, f32, f32) {
    match role {
        FilamentRole::Leash => {
            let dx = ctx.front[0] - ctx.hand[0];
            let dy = ctx.front[1] - ctx.hand[1];
            let dz = ctx.front[2] - ctx.hand[2];
            let span = libm::sqrtf(dx * dx + dy * dy + dz * dz).max(0.01);
            (params.leash_kink, params.leash_width, span, 1.0)
        }
        FilamentRole::Column => (
            params.column_kink,
            params.column_width,
            ctx.height.max(0.01),
            1.0,
        ),
        FilamentRole::Tendril => {
            let span =
                (ctx.radius * (params.tendril_reach - params.tendril_inner).max(0.05)).max(0.01);
            (
                params.tendril_kink,
                params.tendril_width,
                span,
                params.tendril_dim,
            )
        }
        FilamentRole::Rim => {
            let span = (ctx.radius * params.rim_span * TAU).max(0.01);
            (params.rim_kink, params.rim_width, span, params.rim_dim)
        }
    }
}

/// Resolve one filament against live params at the current age.
pub fn sample_filament(
    record: &CageRecord,
    params: &VoltaicSnareParams,
    ctx: &CageContext,
) -> CageSample {
    let tick = restrike_tick(ctx.age, params);
    let seed = filament_seed(record, ctx.seed, tick);
    let (amp, width_mul, span, dim) = role_amp_width_span_dim(record.role, params, ctx);
    let pinch = params.pinch.max(1e-3);

    let flash_gate = hash11(
        libm::floorf(ctx.age * params.flicker_speed.max(1.0))
            + record.index as f32 * 3.7
            + ctx.seed,
    );
    let flash = lerp(1.0, flash_gate, params.strand_flash);

    let mut nodes = Vec::with_capacity(CAGE_NODES);
    for i in 0..CAGE_NODES {
        let t = i as f32 / (CAGE_NODES - 1) as f32;
        let here = path_at(record.role, record.fan, seed, t, params, ctx);
        let behind = path_at(
            record.role,
            record.fan,
            seed,
            (t - 0.02).max(0.0),
            params,
            ctx,
        );
        let ahead = path_at(
            record.role,
            record.fan,
            seed,
            (t + 0.02).min(1.0),
            params,
            ctx,
        );

        let mut tangent = [
            ahead[0] - behind[0],
            ahead[1] - behind[1],
            ahead[2] - behind[2],
        ];
        let tlen =
            libm::sqrtf(tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]);
        if tlen > 1e-5 {
            tangent = [tangent[0] / tlen, tangent[1] / tlen, tangent[2] / tlen];
        } else {
            tangent = [0.0, 1.0, 0.0];
        }
        let up_ref = if libm::fabsf(tangent[1]) > 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let mut n1 = [
            tangent[1] * up_ref[2] - tangent[2] * up_ref[1],
            tangent[2] * up_ref[0] - tangent[0] * up_ref[2],
            tangent[0] * up_ref[1] - tangent[1] * up_ref[0],
        ];
        let n1l = libm::sqrtf(n1[0] * n1[0] + n1[1] * n1[1] + n1[2] * n1[2]).max(1e-5);
        n1 = [n1[0] / n1l, n1[1] / n1l, n1[2] / n1l];
        let n2 = [
            tangent[1] * n1[2] - tangent[2] * n1[1],
            tangent[2] * n1[0] - tangent[0] * n1[2],
            tangent[0] * n1[1] - tangent[1] * n1[0],
        ];

        let ends = smoothstep(0.0, pinch, t) * smoothstep(0.0, pinch, 1.0 - t);
        let mut k = kink(t, seed, span, params, ctx.age);
        k[0] *= amp * params.jitter * ends;
        k[1] *= amp * params.jitter * ends;
        if record.role == FilamentRole::Leash {
            let fan = (record.fan - 0.5) * 2.0 * params.leash_spread;
            k[0] += libm::cosf(seed) * fan * ends;
            k[1] += libm::sinf(seed) * fan * ends;
        }
        let mut offset = [
            n1[0] * k[0] + n2[0] * k[1],
            n1[1] * k[0] + n2[1] * k[1],
            n1[2] * k[0] + n2[2] * k[1],
        ];
        if matches!(record.role, FilamentRole::Tendril | FilamentRole::Rim) {
            offset[1] *= 0.3;
        }
        let mut world = [
            here[0] + offset[0],
            here[1] + offset[1],
            here[2] + offset[2],
        ];
        if matches!(record.role, FilamentRole::Tendril | FilamentRole::Rim) {
            world[1] = world[1].max(params.tendril_hug * 0.4);
        }

        let mut half_width = params.width * width_mul;
        half_width *= lerp(
            1.0,
            libm::powf(libm::sinf(t.clamp(0.0, 1.0) * PI).max(0.0), 0.35),
            0.85,
        );
        if record.role == FilamentRole::Column {
            half_width *= lerp(1.0, params.column_taper, t);
        }
        half_width *= flash * ctx.fade;

        nodes.push(CageNode {
            x: world[0],
            y: world[1],
            z: world[2],
            t,
            half_width,
        });
    }

    CageSample {
        role: record.role,
        index: record.index,
        fan: record.fan,
        flash,
        dim,
        nodes,
    }
}

/// Sample every active filament for the current phase.
///
/// During travel only leash records are drawn; once the trap stands, leash is
/// retired and column / tendril / rim take over.
pub fn sample_cage(
    records: &[CageRecord],
    params: &VoltaicSnareParams,
    ctx: &CageContext,
    traveling: bool,
) -> Vec<CageSample> {
    records
        .iter()
        .filter(|r| {
            if traveling {
                r.role == FilamentRole::Leash
            } else {
                r.role != FilamentRole::Leash
            }
        })
        .filter(|r| match r.role {
            FilamentRole::Leash => (r.index as usize) < params.leash_budget(),
            FilamentRole::Column => (r.index as usize) < params.column_budget(),
            FilamentRole::Tendril => (r.index as usize) < params.tendril_budget(),
            FilamentRole::Rim => (r.index as usize) < params.rim_budget(),
        })
        .map(|r| sample_filament(r, params, ctx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_is_seed_deterministic() {
        let p = VoltaicSnareParams::default();
        let a = roll_cage(&p, 7);
        let b = roll_cage(&p, 7);
        assert_eq!(a, b);
        assert!(!a.is_empty());
        let c = roll_cage(&p, 8);
        assert_ne!(a, c);
    }

    #[test]
    fn budgets_match_role_counts() {
        let p = VoltaicSnareParams::default();
        let records = roll_cage(&p, 1);
        let leash = records.iter().filter(|r| r.role == FilamentRole::Leash).count();
        let col = records.iter().filter(|r| r.role == FilamentRole::Column).count();
        let ten = records.iter().filter(|r| r.role == FilamentRole::Tendril).count();
        let rim = records.iter().filter(|r| r.role == FilamentRole::Rim).count();
        assert_eq!(leash, p.leash_budget());
        assert_eq!(col, p.column_budget());
        assert_eq!(ten, p.tendril_budget());
        assert_eq!(rim, p.rim_budget());
    }

    #[test]
    fn zone_radius_scales_samples() {
        let mut p = VoltaicSnareParams::default();
        let records = roll_cage(&p, 7);
        let tendril = records
            .iter()
            .find(|r| r.role == FilamentRole::Tendril)
            .unwrap();
        let ctx = CageContext {
            center: [0.0, 10.0],
            hand: [0.0, 1.0, 0.0],
            front: [0.0, 0.12, 10.0],
            radius: p.radius(),
            height: p.height,
            age: 1.0,
            seed: 7.0,
            fade: 1.0,
        };
        let a = sample_filament(tendril, &p, &ctx);
        p.zone_radius *= 2.0;
        let ctx2 = CageContext {
            radius: p.radius(),
            ..ctx
        };
        let b = sample_filament(tendril, &p, &ctx2);
        let tip_a = a.nodes.last().unwrap();
        let tip_b = b.nodes.last().unwrap();
        let dist_a = libm::sqrtf(
            (tip_a.x - ctx.center[0]) * (tip_a.x - ctx.center[0])
                + (tip_a.z - ctx.center[1]) * (tip_a.z - ctx.center[1]),
        );
        let dist_b = libm::sqrtf(
            (tip_b.x - ctx.center[0]) * (tip_b.x - ctx.center[0])
                + (tip_b.z - ctx.center[1]) * (tip_b.z - ctx.center[1]),
        );
        assert!(
            dist_b > dist_a * 1.5,
            "zone_radius must re-scale tendril reach ({dist_a} -> {dist_b})"
        );
    }

    #[test]
    fn open_amount_overshoots_then_settles() {
        let p = VoltaicSnareParams::default();
        let mid = open_amount(p.snap_duration() * 0.5, &p);
        let end = open_amount(p.snap_duration(), &p);
        assert!(mid > 0.0 && mid < 1.3);
        assert!((end - 1.0).abs() < 0.05);
    }
}
