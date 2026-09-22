//! Storm Lance filament records and polyline sampling.
//!
//! Ports the CPU-facing half of `ThunderAbility.js` + `LightningMaterial.js`:
//! a cast captures **dice-only** strand records (index / radial); every metre
//! of kink, fan and sag resolves against live [`StormLanceParams`] at sample
//! time. Restrike re-rolls the per-strand shape seed from
//! `(cast_seed, strand, floor(age * restrike))`, so [`StormLance::seek_abs`](crate::storm::StormLance)
//! is deterministic and param edits reshape a standing bolt while paused.

extern crate alloc;

use alloc::vec::Vec;

use crate::math::{hash11, lerp, saturate, smoothstep};
use crate::params::{StormLanceParams, STRAND_NODES};

const TAU: f32 = core::f32::consts::TAU;
const PI: f32 = core::f32::consts::PI;

/// Dice-only record for one lightning filament.
///
/// No metres — only the strand's identity inside the bundle. Shape, fan and
/// kink all resolve at sample time against live params + the cast seed + the
/// current restrike tick.
#[derive(Clone, Debug, PartialEq)]
pub struct StrandRecord {
    /// Strand index inside the bundle (`0..strand_budget`).
    pub index: u16,
    /// `0..1` radial across the bundle (`0` = spine). Stored so a paused
    /// re-sample of a standing cast keeps strand identity even if the live
    /// `strands` slider moves; live sampling still re-derives radial from the
    /// current budget when preferred.
    pub radial: f32,
}

/// One node on a resolved filament polyline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrandNode {
    /// World x.
    pub x: f32,
    /// World y (height above the floor).
    pub y: f32,
    /// World z.
    pub z: f32,
    /// `0..1` along the filament.
    pub t: f32,
    /// Half-width hint in metres (core pass, pre-camera facing).
    pub half_width: f32,
    /// How much of this node is drawn given the strike front (`0..1`).
    pub drawn: f32,
}

/// Resolved, render-ready sample for one filament at the current age/params.
#[derive(Clone, Debug, PartialEq)]
pub struct StrandSample {
    /// Strand index.
    pub index: u16,
    /// `0..1` radial across the bundle.
    pub radial: f32,
    /// Per-filament blink `0..1` (quantised flicker).
    pub flash: f32,
    /// Branch dim applied to outer filaments.
    pub branch: f32,
    /// Polyline nodes (`STRAND_NODES` long; nodes ahead of the front have
    /// `drawn ≈ 0`).
    pub nodes: Vec<StrandNode>,
}

/// Roll the dice for one cast: `budget` strand records.
///
/// Mirrors the instance setup in `ThunderAbility#_syncUniforms` — the shader
/// only needs a strand index; we store radial too so samples stay stable.
pub fn roll_strands(budget: usize, _seed: u64) -> Vec<StrandRecord> {
    let n = budget.max(1);
    let denom = (n - 1).max(1) as f32;
    (0..n)
        .map(|i| StrandRecord {
            index: i as u16,
            radial: if n <= 1 {
                0.0
            } else {
                i as f32 / denom
            },
        })
        .collect()
}

/// Value noise with a *linear* ramp — piecewise-linear output, sharp corners.
/// Ports `LightningMaterial.js::vnoise`.
#[inline]
fn vnoise(x: f32, seed: f32) -> f32 {
    let i = libm::floorf(x);
    let f = x - i;
    lerp(hash11(i + seed), hash11(i + 1.0 + seed), f) * 2.0 - 1.0
}

/// Offset of one filament from the axis, in the perpendicular plane.
/// Ports `LightningMaterial.js::kink`.
fn kink(t: f32, seed: f32, span: f32, params: &StormLanceParams, age: f32) -> [f32; 2] {
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

/// Axis point at `t` along the bolt (hand → impact), including sag.
///
/// Mirrors `ThunderAbility#_axisPoint` / the first stage of `boltPoint`.
pub fn axis_point(
    t: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    params: &StormLanceParams,
) -> [f32; 3] {
    let t = saturate(t);
    let along = lerp(params.hand_forward, length, t);
    let lateral = params.hand_side * (1.0 - t);
    let x = origin[0] + direction[0] * along + side[0] * lateral;
    let z = origin[1] + direction[1] * along + side[1] * lateral;
    let y = lerp(params.hand_height, params.end_height, t) + params.sag * libm::sinf(t * PI);
    [x, y, z]
}

/// World-space point on one filament at parameter `t`.
///
/// Ports `LightningMaterial.js::boltPoint` (without camera-facing ribbon width).
pub fn bolt_point(
    t: f32,
    shape_seed: f32,
    radial: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    params: &StormLanceParams,
    age: f32,
) -> [f32; 3] {
    let t = saturate(t);
    let hand = [
        origin[0] + direction[0] * params.hand_forward + side[0] * params.hand_side,
        params.hand_height,
        origin[1] + direction[1] * params.hand_forward + side[1] * params.hand_side,
    ];
    let impact = [
        origin[0] + direction[0] * length,
        params.end_height,
        origin[1] + direction[1] * length,
    ];
    let mut axis = [
        lerp(hand[0], impact[0], t),
        lerp(hand[1], impact[1], t) + params.sag * libm::sinf(t * PI),
        lerp(hand[2], impact[2], t),
    ];

    let dx = impact[0] - hand[0];
    let dy = impact[1] - hand[1];
    let dz = impact[2] - hand[2];
    let span = libm::sqrtf(dx * dx + dy * dy + dz * dz).max(0.01);
    let dir = [dx / span, dy / span, dz / span];

    // Gram-Schmidt: floor `side` is only approximately perpendicular to the
    // tilted hand→impact axis.
    let dot = side[0] * dir[0] + side[1] * dir[2];
    let mut n1 = [side[0] - dir[0] * dot, 0.0 - dir[1] * dot, side[1] - dir[2] * dot];
    let n1_len = libm::sqrtf(n1[0] * n1[0] + n1[1] * n1[1] + n1[2] * n1[2]);
    if n1_len > 1e-4 {
        n1 = [n1[0] / n1_len, n1[1] / n1_len, n1[2] / n1_len];
    } else {
        // cross(dir, up)
        let cx = dir[1] * 0.0 - dir[2] * 1.0;
        let cy = dir[2] * 0.0 - dir[0] * 0.0;
        let cz = dir[0] * 1.0 - dir[1] * 0.0;
        let cl = libm::sqrtf(cx * cx + cy * cy + cz * cz).max(1e-6);
        n1 = [cx / cl, cy / cl, cz / cl];
    }
    // n2 = normalize(cross(dir, n1))
    let mut n2 = [
        dir[1] * n1[2] - dir[2] * n1[1],
        dir[2] * n1[0] - dir[0] * n1[2],
        dir[0] * n1[1] - dir[1] * n1[0],
    ];
    let n2_len = libm::sqrtf(n2[0] * n2[0] + n2[1] * n2[1] + n2[2] * n2[2]).max(1e-6);
    n2 = [n2[0] / n2_len, n2[1] / n2_len, n2[2] / n2_len];

    let pinch = params.pinch.max(1e-3);
    let ends = smoothstep(0.0, pinch, t)
        * lerp(
            1.0,
            smoothstep(0.0, pinch, 1.0 - t),
            saturate(params.converge),
        );

    let jitter = params.jitter * params.randomness;
    let k = kink(t, shape_seed, span, params, age);
    let mut offset = [k[0] * jitter * ends, k[1] * jitter * ends];

    let angle = shape_seed * TAU + (t * params.twist + age * params.twist_speed) * TAU;
    let reach = lerp(
        params.spread_near,
        params.spread,
        libm::powf(t, params.spread_curve.max(0.01)),
    );
    offset[0] += libm::cosf(angle) * reach * radial;
    offset[1] += libm::sinf(angle) * reach * radial;

    axis[0] += n1[0] * offset[0] + n2[0] * offset[1];
    axis[1] += n1[1] * offset[0] + n2[1] * offset[1];
    axis[2] += n1[2] * offset[0] + n2[2] * offset[1];
    axis
}

/// Restrike tick at `age` — `floor(age * restrike)`.
#[inline]
pub fn restrike_tick(age: f32, params: &StormLanceParams) -> f32 {
    libm::floorf(age * params.restrike.max(0.01))
}

/// Per-strand shape seed for a restrike tick. Ports the shader's
/// `hash11(aStrand * 7.13 + uSeed + strike * 3.77) * 97.0`.
#[inline]
pub fn shape_seed(strand_index: u16, cast_seed: f32, tick: f32) -> f32 {
    hash11(strand_index as f32 * 7.13 + cast_seed + tick * 3.77) * 97.0
}

/// Resolve one strand into a polyline against live params.
pub fn sample_strand(
    record: &StrandRecord,
    params: &StormLanceParams,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    age: f32,
    cast_seed: f32,
    progress: f32,
    fade: f32,
    strand_count: usize,
) -> StrandSample {
    let n = strand_count.max(1);
    let radial = if n <= 1 {
        0.0
    } else {
        // Live radial from current budget (edit-while-paused).
        (record.index as usize).min(n - 1) as f32 / (n - 1) as f32
    };
    let tick = restrike_tick(age, params);
    let seed = shape_seed(record.index, cast_seed, tick);

    let flash_q = libm::floorf(age * params.flicker_speed.max(0.01));
    let flash = lerp(
        1.0,
        hash11(flash_q + record.index as f32 * 3.7 + cast_seed),
        params.strand_flash,
    );

    let tip = params.tip_length.max(1e-3);
    let mut nodes = Vec::with_capacity(STRAND_NODES);
    for i in 0..STRAND_NODES {
        let t = i as f32 / (STRAND_NODES - 1) as f32;
        let p = bolt_point(
            t, seed, radial, origin, direction, side, length, params, age,
        );
        let mut half_width = params.width;
        half_width *= lerp(
            1.0,
            params.width_tip,
            libm::powf(t, params.width_curve.max(0.01)),
        );
        half_width *= lerp(params.core_width, 1.0, radial);
        half_width *= flash * fade;

        // Ahead of the strike front there is no bolt yet.
        let drawn = smoothstep(progress, progress - tip, t);
        nodes.push(StrandNode {
            x: p[0],
            y: p[1],
            z: p[2],
            t,
            half_width,
            drawn,
        });
    }

    StrandSample {
        index: record.index,
        radial,
        flash,
        branch: lerp(1.0, saturate(params.branch_dim), radial),
        nodes,
    }
}

/// Whole-bolt brightness stutter (quantised). Ports the fragment shader's
/// `1 - uFlicker * hash11(floor(uTime * uFlickerSpeed) + uSeed)`.
pub fn bolt_flicker(age: f32, cast_seed: f32, params: &StormLanceParams) -> f32 {
    let q = libm::floorf(age * params.flicker_speed.max(0.01));
    1.0 - params.flicker * hash11(q + cast_seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_strands_covers_budget() {
        let records = roll_strands(9, 7);
        assert_eq!(records.len(), 9);
        assert_eq!(records[0].radial, 0.0);
        assert!((records[8].radial - 1.0).abs() < 1e-5);
    }

    #[test]
    fn restrike_tick_quantises() {
        let p = StormLanceParams::default();
        assert_eq!(restrike_tick(0.0, &p), 0.0);
        assert_eq!(restrike_tick(1.0 / 24.0, &p), 1.0);
        assert_eq!(restrike_tick(0.5, &p), 12.0);
    }

    #[test]
    fn shape_seed_changes_with_tick() {
        let a = shape_seed(0, 7.0, 0.0);
        let b = shape_seed(0, 7.0, 1.0);
        assert_ne!(a, b);
        assert_eq!(shape_seed(0, 7.0, 0.0), a);
    }

    #[test]
    fn sample_is_deterministic() {
        let p = StormLanceParams::default();
        let rec = StrandRecord {
            index: 2,
            radial: 0.25,
        };
        let a = sample_strand(
            &rec, &p, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0], 12.0, 0.2, 7.0, 1.0, 1.0, 9,
        );
        let b = sample_strand(
            &rec, &p, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0], 12.0, 0.2, 7.0, 1.0, 1.0, 9,
        );
        assert_eq!(a, b);
        assert_eq!(a.nodes.len(), STRAND_NODES);
        assert!(a.nodes.iter().all(|n| n.x.is_finite() && n.y.is_finite() && n.z.is_finite()));
    }

    #[test]
    fn hash_is_bounded() {
        assert!((0.0..1.0).contains(&hash11(12.3)));
        assert!((0.0..1.0).contains(&hash11(-3.7)));
    }
}
