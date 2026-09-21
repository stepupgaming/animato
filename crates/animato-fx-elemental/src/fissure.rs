//! Cinder Fall debris + fissure dice and sample-time resolution.
//!
//! Ports the CPU-facing half of `MeteorAbility.js` debris and
//! `GroundFissures.js`: a cast captures **dice-only** chunk / arm / branch
//! records; every metre of ballistic flight and every fissure polyline
//! resolves against live [`CinderFallParams`] at sample time — so dragging
//! `arc` re-lofts a meteor already in the air, and dragging `fissure_radius`
//! re-scales cracks already on the ground.

extern crate alloc;

use alloc::vec::Vec;

use crate::math::{clamp, in_cubic, in_quad, lerp, saturate};
use crate::params::{
    CinderFallParams, FISSURE_STEP, MAX_CHUNKS, MAX_FISSURE_ARMS, MAX_FISSURE_BRANCHES,
};
use crate::rng::FxRng;

const TAU: f32 = core::f32::consts::TAU;
const PI: f32 = core::f32::consts::PI;

/// Dice-only record for one impact debris chunk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkRecord {
    /// Bearing of the ejecta, radians.
    pub angle: f32,
    /// `0..1` of the loft cone.
    pub elevation: f32,
    /// `0..1` speed jitter.
    pub speed: f32,
    /// `0..1` size jitter.
    pub size: f32,
    /// `-1..1` tumble rate.
    pub spin: f32,
    /// Unit tumble axis.
    pub spin_axis: [f32; 3],
}

/// Dice-only record for one main fissure arm (unit-space walk).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FissureArmRecord {
    /// Seed for per-step stagger / width jitter.
    pub seed: u64,
    /// Absolute launch heading, radians.
    pub heading: f32,
    /// Unit-space length (`0.6..1.0`).
    pub length: f32,
    /// Start radius outside the crater mouth (`0.04..0.12`).
    pub start_r: f32,
    /// Steady curvature rolled at spawn (`±wander`).
    pub curvature: f32,
}

/// Dice-only record for one side branch hung off a main arm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FissureBranchRecord {
    /// Seed for the branch walk.
    pub seed: u64,
    /// Parent arm index.
    pub arm_index: u16,
    /// Fraction along the parent arm where the branch forks (`0..1`).
    pub at_frac: f32,
    /// `±1` which side of the parent.
    pub side: f32,
    /// Fork angle off the parent (`0.55..1.25` rad).
    pub fork: f32,
    /// Unit-space branch length (`0.18..0.42`).
    pub length: f32,
    /// Rank used by the density cull (`(b+1)/MAX_BRANCHES`).
    pub rank: f32,
}

/// One node on a resolved fissure polyline (world metres on the floor).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FissureNode {
    /// World x.
    pub x: f32,
    /// World z (floor plane y-up → our 2D `y` maps here).
    pub z: f32,
    /// `0..1` radial distance from impact in unit space.
    pub dist: f32,
    /// Local half-width hint, metres (pre-camera facing).
    pub half_width: f32,
    /// How much of this node has been grown by the crack front (`0..1`).
    pub grown: f32,
}

/// Resolved fissure arm or branch polyline at the current age/params.
#[derive(Clone, Debug, PartialEq)]
pub struct FissureSample {
    /// `0` = main arm, `>0` = branch rank.
    pub rank: f32,
    /// Polyline nodes (clipped by growth + branch-length pinch).
    pub nodes: Vec<FissureNode>,
}

/// Resolved debris chunk pose at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkSample {
    /// World x.
    pub x: f32,
    /// World y (height).
    pub y: f32,
    /// World z.
    pub z: f32,
    /// Radius, metres.
    pub radius: f32,
    /// Seam heat `1 → 0` over `chunk_cool`.
    pub heat: f32,
    /// Tumble angle, radians.
    pub angle: f32,
}

/// Roll tumble axis + chunk dice for one cast.
pub fn roll_chunks(budget: usize, seed: u64) -> ( [f32; 3], Vec<ChunkRecord> ) {
    let mut rng = FxRng::new(seed ^ 0xC1_DE_F1_55);
    let tumble = random_unit_axis(&mut rng);
    let n = budget.min(MAX_CHUNKS);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(ChunkRecord {
            angle: rng.next_f32() * TAU,
            elevation: rng.next_f32(),
            speed: rng.next_f32(),
            size: rng.next_f32(),
            spin: rng.range(-1.0, 1.0),
            spin_axis: random_unit_axis(&mut rng),
        });
    }
    (tumble, out)
}

/// Roll fissure arm + branch dice for one cast.
pub fn roll_fissures(
    arm_budget: usize,
    wander: f32,
    seed: u64,
) -> (Vec<FissureArmRecord>, Vec<FissureBranchRecord>) {
    let mut rng = FxRng::new(seed ^ 0xF155_0BE5);
    let arms_n = arm_budget.clamp(2, MAX_FISSURE_ARMS);
    let spin = rng.next_f32() * TAU;
    let mut arms = Vec::with_capacity(arms_n);
    for i in 0..arms_n {
        let heading =
            spin + (i as f32 / arms_n as f32) * TAU + rng.range(-0.45, 0.45);
        arms.push(FissureArmRecord {
            seed: rng.next_u64(),
            heading,
            length: rng.range(0.6, 1.0),
            start_r: rng.range(0.04, 0.12),
            curvature: rng.range(-wander, wander),
        });
    }
    let mut branches = Vec::with_capacity(MAX_FISSURE_BRANCHES);
    for b in 0..MAX_FISSURE_BRANCHES {
        let arm_index = libm::floorf(rng.next_f32() * arms_n as f32) as u16;
        let arm_index = arm_index.min((arms_n - 1) as u16);
        branches.push(FissureBranchRecord {
            seed: rng.next_u64(),
            arm_index,
            at_frac: rng.range(0.15, 0.85),
            side: if rng.next_f32() < 0.5 { 1.0 } else { -1.0 },
            fork: rng.range(0.55, 1.25),
            length: rng.range(0.18, 0.42),
            rank: (b as f32 + 1.0) / MAX_FISSURE_BRANCHES as f32,
        });
    }
    (arms, branches)
}

fn random_unit_axis(rng: &mut FxRng) -> [f32; 3] {
    let phi = libm::acosf(rng.range(-1.0, 1.0));
    let theta = rng.next_f32() * TAU;
    let s = libm::sinf(phi);
    [s * libm::cosf(theta), libm::cosf(phi), s * libm::sinf(theta)]
}

/// Point on the ballistic arc, `s` from 0 (hand) to 1 (impact).
///
/// Mirrors `MeteorAbility#_arcPoint`: floor aiming stays on `origin` +
/// `direction * length`, while the rock is lofted from the hand offsets.
pub fn arc_point(
    s: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    params: &CinderFallParams,
) -> [f32; 3] {
    let t = saturate(s);
    let along = lerp(params.hand_forward, length, t);
    let lateral = params.hand_side * (1.0 - t);
    let curve = params.arc_curve.max(0.05);
    // fabs: sin(π) can go slightly negative in f32 and poison powf → NaN.
    let lob = params.arc * libm::powf(libm::fabsf(libm::sinf(t * PI)), curve);
    [
        origin[0] + direction[0] * along + side[0] * lateral,
        lerp(params.hand_height, params.end_height, t) + lob,
        origin[1] + direction[1] * along + side[1] * lateral,
    ]
}

/// Unit heading along the arc at `s` (numeric derivative).
pub fn heading_at(
    s: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    params: &CinderFallParams,
) -> [f32; 3] {
    let eps = 0.01;
    let a = arc_point(s + eps, origin, direction, side, length, params);
    let b = arc_point(s - eps, origin, direction, side, length, params);
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let len = libm::sqrtf(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
    if len < 1e-4 {
        [direction[0], 0.0, direction[1]]
    } else {
        [d[0] / len, d[1] / len, d[2] / len]
    }
}

fn chunk_radius(record: &ChunkRecord, params: &CinderFallParams) -> f32 {
    (params.rock_radius() * params.chunk_scale * lerp(0.45, 1.15, record.size)).max(0.01)
}

fn chunk_direction(
    record: &ChunkRecord,
    direction: [f32; 2],
    side: [f32; 2],
    params: &CinderFallParams,
) -> [f32; 3] {
    let elevation = lerp(0.12, 1.4, record.elevation) * params.chunk_loft.max(0.0);
    let flat = libm::cosf(elevation);
    let mut out = [
        side[0] * libm::cosf(record.angle) * flat
            + direction[0] * (libm::sinf(record.angle) * flat + params.chunk_forward),
        libm::sinf(elevation),
        side[1] * libm::cosf(record.angle) * flat
            + direction[1] * (libm::sinf(record.angle) * flat + params.chunk_forward),
    ];
    let len = libm::sqrtf(out[0] * out[0] + out[1] * out[1] + out[2] * out[2]);
    if len < 1e-4 {
        [0.0, 1.0, 0.0]
    } else {
        out[0] /= len;
        out[1] /= len;
        out[2] /= len;
        out
    }
}

/// Closed-form chunk flight `elapsed` seconds after impact.
pub fn sample_chunk(
    record: &ChunkRecord,
    params: &CinderFallParams,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    elapsed: f32,
    retract: f32,
) -> ChunkSample {
    let radius = chunk_radius(record, params);
    let speed = params.chunk_speed * lerp(0.45, 1.35, record.speed);
    let gravity = params.chunk_gravity.min(-0.01);
    let rest = radius * 0.8;
    let dir = chunk_direction(record, direction, side, params);
    let impact = arc_point(1.0, origin, direction, side, length, params);
    let vy = dir[1] * speed;
    let y0 = impact[1];
    let discriminant = (vy * vy - 2.0 * gravity * (y0 - rest)).max(0.0);
    let landing = (vy + libm::sqrtf(discriminant)) / -gravity;
    let t = elapsed.min(landing);
    let x = impact[0] + dir[0] * speed * t;
    let mut y = (y0 + vy * t + 0.5 * gravity * t * t).max(rest);
    let z = impact[2] + dir[2] * speed * t;
    if retract > 0.0 {
        y -= in_cubic(retract) * (radius * 2.0 + 0.5);
    }
    let cool = params.chunk_cool.max(0.05);
    ChunkSample {
        x,
        y,
        z,
        radius,
        heat: saturate(1.0 - elapsed / cool),
        angle: record.spin * params.chunk_spin * t,
    }
}

fn walk_crack(
    start: [f32; 2],
    heading: f32,
    length: f32,
    curvature: f32,
    seed: u64,
    origin_dist: f32,
    width_scale: f32,
    rank: f32,
    params: &CinderFallParams,
    grown: f32,
    branch_len_frac: f32,
) -> Vec<FissureNode> {
    let mut rng = FxRng::new(seed);
    let mut nodes = Vec::new();
    let mut x = start[0];
    let mut z = start[1];
    let mut angle = heading;
    let mut jitter = 1.0f32;
    let radius = params.fissure_radius.max(0.05);
    let half_w = params.fissure_width.max(0.01) * 0.5;

    let mut travelled = 0.0f32;
    while travelled <= length + 1e-6 {
        jitter = clamp(jitter + rng.range(-0.18, 0.18), 0.6, 1.45);
        let dist = saturate(origin_dist + travelled);
        let tip = libm::powf(saturate((length - travelled) / 0.22), 0.65);
        let width_u = jitter * width_scale * if rank > 0.0 { 0.62 } else { tip };

        // Branch length pinch (live slider).
        let walk_taper = if rank > 0.0 {
            let max_walk = length * branch_len_frac.max(1e-4);
            libm::powf(saturate(1.0 - travelled / max_walk), 0.7)
        } else {
            1.0
        };
        if walk_taper <= 1e-3 {
            break;
        }

        if dist <= grown + 0.05 {
            nodes.push(FissureNode {
                x: x * radius,
                z: z * radius,
                dist,
                half_width: half_w * width_u * walk_taper,
                grown: if dist <= grown { 1.0 } else { saturate(1.0 - (dist - grown) / 0.08) },
            });
        }

        let stagger = rng.range(-0.35, 0.35);
        x += libm::cosf(angle) * FISSURE_STEP;
        z += libm::sinf(angle) * FISSURE_STEP;
        angle += curvature * FISSURE_STEP + stagger * FISSURE_STEP;
        angle = lerp(angle, heading, 0.06);
        travelled += FISSURE_STEP;
    }
    nodes
}

fn arm_polyline_unit(arm: &FissureArmRecord) -> Vec<(f32, f32, f32, f32)> {
    // Returns (x, z, angle, dist) in unit space for branch attachment.
    let mut rng = FxRng::new(arm.seed);
    let mut out = Vec::new();
    let mut x = libm::cosf(arm.heading) * arm.start_r;
    let mut z = libm::sinf(arm.heading) * arm.start_r;
    let mut angle = arm.heading;
    let mut travelled = 0.0f32;
    while travelled <= arm.length + 1e-6 {
        let _ = rng.range(-0.18, 0.18); // keep RNG in lockstep with walk_crack
        let dist = saturate(arm.start_r + travelled);
        out.push((x, z, angle, dist));
        let stagger = rng.range(-0.35, 0.35);
        x += libm::cosf(angle) * FISSURE_STEP;
        z += libm::sinf(angle) * FISSURE_STEP;
        angle += arm.curvature * FISSURE_STEP + stagger * FISSURE_STEP;
        angle = lerp(angle, arm.heading, 0.06);
        travelled += FISSURE_STEP;
    }
    out
}

/// Resolve every active fissure arm + branch against live params.
pub fn sample_fissures(
    arms: &[FissureArmRecord],
    branches: &[FissureBranchRecord],
    params: &CinderFallParams,
    since_impact: f32,
    fissure_age: f32,
) -> Vec<FissureSample> {
    let radius = params.fissure_radius.max(0.05);
    let grown = saturate((since_impact * params.fissure_growth) / radius);
    let life = params.fissure_life.max(0.2);
    let t = saturate(fissure_age / life);
    let fade = 1.0 - in_quad(saturate((t - 0.7) / 0.3));
    let branch_frac = saturate(params.fissure_branches);
    let branch_len = saturate(params.fissure_branch_length);

    let mut samples = Vec::new();
    let arm_paths: Vec<Vec<(f32, f32, f32, f32)>> =
        arms.iter().map(arm_polyline_unit).collect();

    for arm in arms {
        let start = [
            libm::cosf(arm.heading) * arm.start_r,
            libm::sinf(arm.heading) * arm.start_r,
        ];
        let mut nodes = walk_crack(
            start,
            arm.heading,
            arm.length,
            arm.curvature,
            arm.seed,
            arm.start_r,
            1.0,
            0.0,
            params,
            grown,
            1.0,
        );
        if fade < 1.0 {
            for n in &mut nodes {
                n.half_width *= fade;
            }
        }
        if !nodes.is_empty() {
            samples.push(FissureSample { rank: 0.0, nodes });
        }
    }

    for branch in branches {
        if branch.rank > branch_frac {
            continue;
        }
        let Some(path) = arm_paths.get(branch.arm_index as usize) else {
            continue;
        };
        if path.len() < 4 {
            continue;
        }
        let idx = ((branch.at_frac * (path.len() - 1) as f32) as usize).clamp(1, path.len() - 2);
        let (ax, az, a_angle, a_dist) = path[idx];
        let heading = a_angle + branch.side * branch.fork;
        let mut nodes = walk_crack(
            [ax, az],
            heading,
            branch.length,
            0.0, // branches inherit no steady curvature; stagger still applies
            branch.seed,
            a_dist,
            0.8,
            branch.rank,
            params,
            grown,
            branch_len,
        );
        if fade < 1.0 {
            for n in &mut nodes {
                n.half_width *= fade;
            }
        }
        if !nodes.is_empty() {
            samples.push(FissureSample {
                rank: branch.rank,
                nodes,
            });
        }
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_roll_is_seed_deterministic() {
        let (a_ax, a) = roll_chunks(18, 7);
        let (b_ax, b) = roll_chunks(18, 7);
        assert_eq!(a_ax, b_ax);
        assert_eq!(a, b);
        assert_eq!(a.len(), 18);
        let (_, c) = roll_chunks(18, 8);
        assert_ne!(a[0].angle, c[0].angle);
    }

    #[test]
    fn fissure_roll_is_seed_deterministic() {
        let p = CinderFallParams::default();
        let (a_arms, a_br) = roll_fissures(p.fissure_arm_budget(), p.fissure_wander, 7);
        let (b_arms, b_br) = roll_fissures(p.fissure_arm_budget(), p.fissure_wander, 7);
        assert_eq!(a_arms, b_arms);
        assert_eq!(a_br, b_br);
        assert_eq!(a_arms.len(), 6);
        assert_eq!(a_br.len(), MAX_FISSURE_BRANCHES);
    }

    #[test]
    fn fissure_sample_scales_with_radius() {
        let p = CinderFallParams::default();
        let (arms, branches) = roll_fissures(p.fissure_arm_budget(), p.fissure_wander, 7);
        let a = sample_fissures(&arms, &branches, &p, 2.0, 2.0);
        let mut wide = p.clone();
        wide.fissure_radius = p.fissure_radius * 2.0;
        let b = sample_fissures(&arms, &branches, &wide, 2.0, 2.0);
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len());
        let da = a[0].nodes[a[0].nodes.len() / 2];
        let db = b[0].nodes[b[0].nodes.len() / 2];
        assert!((db.x / da.x).abs() > 1.5 || (db.z / da.z).abs() > 1.5 || da.x.abs() < 1e-4);
    }

    #[test]
    fn arc_lob_peaks_near_midspan() {
        let p = CinderFallParams::default();
        let mid = arc_point(0.5, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0], 12.0, &p);
        let hand = arc_point(0.0, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0], 12.0, &p);
        let tip = arc_point(1.0, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0], 12.0, &p);
        assert!(mid[1] > hand[1] && mid[1] > tip[1]);
        assert!((tip[2] - 12.0).abs() < 0.05);
    }
}
