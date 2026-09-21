//! The Nova Beam cast pipeline: charge orb, parametric tube, shock discs and
//! light bookkeeping.
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `BeamAbility.js` (first-class **charge** beat, hand
//! → impact tube axis, ring train, dual muzzle/beam lights). Everything is a
//! pure function of `(seed, params, time)`: [`NovaBeam::seek_abs`]
//! deterministically re-simulates from spawn, so the pipeline can be driven by
//! [`animato_composition::Composition`] — including while paused, with params
//! edited live (ring records store dice only; tube radii and ring poses
//! resolve at sample time).
//!
//! Renderer-owned systems from the original (GPU tube/coil/orb materials,
//! particles, ground decals, burst shells, camera shake, screen flash) are
//! intentionally *not* reimplemented here: the pipeline exposes every stimulus
//! they need ([`NovaBeam::tube`], [`NovaBeam::rings`], [`NovaBeam::orb`],
//! [`NovaBeam::drain_events`], [`NovaBeam::light`], [`NovaBeam::muzzle_light`]).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;
use core::f32::consts::TAU;

use crate::aim::{AimSolution, solve_aim};
use crate::math::{in_cubic, lerp, out_cubic, out_quad, saturate, smoothstep};
use crate::params::{MAX_RINGS, NovaBeamParams, TUBE_SEGMENTS};
use crate::pipeline::{Phase, SEEK_STEP, SpawnError};
use crate::rng::FxRng;

/// Phase-transition events for renderer-owned systems (bursts, decals, shake,
/// flash). Drained via [`NovaBeam::drain_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NovaEvent {
    /// Charge complete; beam released from the hands.
    Release,
    /// Leading edge reached the far end; sustain / burn begins.
    Impact,
    /// Collapse started.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast, mirroring `Ability#_updateLight` with
/// Beam's slow hum (not a quantised gutter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NovaLight {
    /// World x of the light.
    pub x: f32,
    /// World y of the light.
    pub y: f32,
    /// World z of the light.
    pub z: f32,
    /// Effective intensity (base × phase scale × shimmer + boost).
    pub intensity: f32,
    /// Effective radius in metres.
    pub radius: f32,
}

/// Dice captured for one shock disc. Metres resolve at sample time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingRecord {
    /// Phase offset along the beam, `0..1`.
    pub phase: f32,
}

/// Resolved shock-disc pose at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingSample {
    /// Fraction along the column, `0..1`.
    pub s: f32,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// World z.
    pub z: f32,
    /// Inner lip radius, metres.
    pub inner: f32,
    /// Outer lip radius, metres.
    pub outer: f32,
    /// Visibility / residual opacity `0..1`.
    pub alpha: f32,
}

/// One sample of the parametric beam tube centreline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TubeNode {
    /// Fraction along the column, `0..1`.
    pub s: f32,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// World z.
    pub z: f32,
    /// Half-width at this station, metres (live params × width fade).
    pub radius: f32,
    /// How much of the tube exists here (`1` behind the front, `0` ahead).
    pub drawn: f32,
}

/// Charge-orb pose at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbSample {
    /// World x (hand point).
    pub x: f32,
    /// World y.
    pub y: f32,
    /// World z.
    pub z: f32,
    /// Radius, metres.
    pub radius: f32,
    /// Charge progress `0..1`.
    pub charge: f32,
    /// Whether the orb is drawn (hidden after release fades).
    pub visible: bool,
}

/// Roll dice for the shock-disc train.
pub fn roll_rings(count: usize, seed: u64) -> Vec<RingRecord> {
    let n = count.min(MAX_RINGS);
    let mut rng = FxRng::new(seed ^ 0xBEA1_u64);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        // Evenly spaced with a tiny dice jitter so discs never stack.
        let base = (i as f32 + 0.5) / n as f32;
        let jitter = (rng.next_f32() - 0.5) * (0.35 / n as f32);
        out.push(RingRecord {
            phase: saturate(base + jitter),
        });
    }
    out
}

/// Half-width of the column at `s`, metres — mirrors `BeamAbility#_beamRadius`.
pub fn beam_radius(s: f32, params: &NovaBeamParams) -> f32 {
    let t = saturate(s);
    let curve = params.radius_curve.max(0.01);
    let r = lerp(
        params.radius_near,
        params.radius,
        libm::powf(t, curve),
    );
    let flare_w = params.flare_width.max(1e-3);
    r * (1.0 + params.flare * smoothstep(1.0 - flare_w, 1.0, t))
}

/// Axis point at `s` along the column (hand → impact), mirrors
/// `BeamAbility#_axisPoint` (no wander).
pub fn axis_point(
    s: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    params: &NovaBeamParams,
) -> [f32; 3] {
    let t = saturate(s);
    let along = lerp(params.hand_forward, length, t);
    let lateral = params.hand_side * (1.0 - t);
    [
        origin[0] + direction[0] * along + side[0] * lateral,
        lerp(params.hand_height, params.end_height, t),
        origin[1] + direction[1] * along + side[1] * lateral,
    ]
}

/// Resolve one shock disc against live params.
pub fn sample_ring(
    record: &RingRecord,
    params: &NovaBeamParams,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    age: f32,
    progress: f32,
    fade: f32,
) -> RingSample {
    let speed = params.ring_speed.max(0.0);
    let s = {
        let raw = age * speed + record.phase;
        raw - libm::floorf(raw)
    };
    let drawn = if s <= progress + 1e-4 { 1.0 } else { 0.0 };
    let p = axis_point(s, origin, direction, side, length, params);
    let local_r = beam_radius(s, params);
    let swell = 1.0 + params.ring_swell * s;
    let residual = lerp(1.0, params.ring_fade.max(0.0), s);
    RingSample {
        s,
        x: p[0],
        y: p[1],
        z: p[2],
        inner: local_r * params.ring_inner * swell,
        outer: local_r * params.ring_outer * swell,
        alpha: fade * residual * drawn,
    }
}

/// One complete Nova Beam pipeline: dice + phase machine + parametric tube.
///
/// `Playable + Send + 'static`, so it slots directly into
/// `animato_composition::Composition::add` / `animato_timeline::Timeline`.
#[derive(Clone, Debug)]
pub struct NovaBeam {
    params: NovaBeamParams,
    /// Cast seed as `f32` so it plugs straight into the shader-style hashes.
    seed: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    length: f32,
    front: f32,
    u: f32,
    age: f32,
    impact_time: f32,
    fade_time: f32,
    light_boost: f32,
    phase: Phase,
    /// `true` once the charge beat released the beam (Release event fired).
    fired: bool,
    rings: Vec<RingRecord>,
    events: Vec<NovaEvent>,
}

impl NovaBeam {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(NovaBeamParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: NovaBeamParams) -> Self {
        Self {
            params,
            seed: 0.0,
            origin: [0.0, 0.0],
            direction: [0.0, 1.0],
            side: [1.0, 0.0],
            length: 1.0,
            front: 0.0,
            u: 0.0,
            age: 0.0,
            impact_time: 0.0,
            fade_time: 0.0,
            light_boost: 0.0,
            phase: Phase::Idle,
            fired: false,
            rings: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Live params. Edits apply to the *standing* cast on the next sample —
    /// the edit-while-paused rule from the original.
    pub fn params(&self) -> &NovaBeamParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut NovaBeamParams {
        &mut self.params
    }

    /// Current phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// `true` while a cast is in flight (charge, travel, sustain or fade).
    pub fn is_active(&self) -> bool {
        matches!(
            self.phase,
            Phase::Charge | Phase::Travel | Phase::Impact | Phase::Fade
        )
    }

    /// Metres the leading edge has travelled (0 during charge).
    pub fn front(&self) -> f32 {
        self.front
    }

    /// Front as a fraction of the cast length.
    pub fn u(&self) -> f32 {
        self.u
    }

    /// Cast age in seconds.
    pub fn age(&self) -> f32 {
        self.age
    }

    /// Cast distance in metres.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// Dice seed for the current cast (as `f32`, matching the shader uniform).
    pub fn seed(&self) -> f32 {
        self.seed
    }

    /// Ring dice for the current cast.
    pub fn ring_records(&self) -> &[RingRecord] {
        let budget = self.params.ring_budget().min(self.rings.len());
        &self.rings[..budget]
    }

    /// How far the orb has wound up, `0..1` (mirrors `BeamAbility#charge`).
    pub fn charge(&self) -> f32 {
        saturate(self.age / self.params.charge_duration())
    }

    /// Begin a cast from a solved aim. Refuses aims nearer than `min_range`.
    ///
    /// `seed` fixes the dice: same `(params, seed, aim)` ⇒ same cast.
    pub fn spawn(&mut self, aim: &AimSolution, seed: u64) -> Result<(), SpawnError> {
        if !aim.valid {
            return Err(SpawnError::TooClose {
                raw: aim.raw_distance,
                min: self.params.min_range,
            });
        }
        self.seed = seed as f32;
        self.origin = aim.origin;
        self.direction = aim.direction;
        self.side = [aim.direction[1], -aim.direction[0]];
        self.length = aim.distance.max(0.1);

        let budget = self.params.ring_budget();
        self.rings = roll_rings(budget, seed);

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = 0.0;
        self.fired = false;
        self.phase = Phase::Charge;
        self.events.clear();
        Ok(())
    }

    /// Convenience: solve the aim from raw pointer distance, then spawn.
    pub fn cast(
        &mut self,
        origin: [f32; 2],
        direction: [f32; 2],
        raw_distance: f32,
        seed: u64,
    ) -> Result<(), SpawnError> {
        let aim = solve_aim(&self.params, origin, direction, raw_distance);
        self.spawn(&aim, seed)
    }

    /// A point on the floor cast line; `s` is `0..1` along it.
    pub fn point_at(&self, s: f32) -> [f32; 2] {
        [
            self.origin[0] + self.direction[0] * s * self.length,
            self.origin[1] + self.direction[1] * s * self.length,
        ]
    }

    /// World position of the travelling front on the floor plane.
    pub fn front_position(&self) -> [f32; 2] {
        self.point_at(self.u)
    }

    /// Far-end impact point on the floor plane.
    pub fn impact_position(&self) -> [f32; 2] {
        self.point_at(1.0)
    }

    /// Hand origin in world space (3D), mirrors `BeamAbility#_handPoint`.
    pub fn hand_point(&self) -> [f32; 3] {
        let p = &self.params;
        [
            self.origin[0] + self.direction[0] * p.hand_forward + self.side[0] * p.hand_side,
            p.hand_height,
            self.origin[1] + self.direction[1] * p.hand_forward + self.side[1] * p.hand_side,
        ]
    }

    /// Impact point in world space (3D).
    pub fn impact_point_3d(&self) -> [f32; 3] {
        let p = self.point_at(1.0);
        [p[0], self.params.end_height, p[1]]
    }

    /// Axis point at `s` along the column (hand → impact).
    pub fn axis_at(&self, s: f32) -> [f32; 3] {
        axis_point(
            s,
            self.origin,
            self.direction,
            self.side,
            self.length,
            &self.params,
        )
    }

    /// How much of the beam exists (`0` while charging, `u` while travelling,
    /// else `1`).
    pub fn progress(&self) -> f32 {
        match self.phase {
            Phase::Charge | Phase::Idle => 0.0,
            Phase::Travel => self.u,
            Phase::Impact | Phase::Fade | Phase::Done => 1.0,
        }
    }

    /// Beam fade `1 → 0` through the collapse (cubic; hangs on then goes).
    pub fn beam_fade(&self) -> f32 {
        match self.phase {
            Phase::Charge | Phase::Travel | Phase::Impact => 1.0,
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                1.0 - in_cubic(t)
            }
            Phase::Idle | Phase::Done => 0.0,
        }
    }

    /// Width collapse — snaps to a thread faster than the dim (mirrors
    /// `BeamAbility` `widthFade = 1 - inCubic(saturate(t * 1.55))`).
    pub fn width_fade(&self) -> f32 {
        match self.phase {
            Phase::Charge | Phase::Travel | Phase::Impact => 1.0,
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                1.0 - in_cubic(saturate(t * 1.55))
            }
            Phase::Idle | Phase::Done => 0.0,
        }
    }

    /// Seconds from spawn through charge + travel to impact.
    pub fn travel_duration(&self) -> f32 {
        let charge = self.params.charge_duration();
        let speed = (self.params.speed * self.params.speed_scale).max(1e-6);
        let mut front = 0.0f32;
        let mut age = 0.0f32;
        loop {
            age += SEEK_STEP;
            if age < charge {
                continue;
            }
            let since = age - charge;
            let ease_in = out_quad(saturate(since / 0.05));
            front += speed * ease_in * SEEK_STEP;
            if front >= self.length || age > 3600.0 {
                break;
            }
        }
        age
    }

    /// Total finite duration: charge + travel + sustain + fade.
    ///
    /// Adds two [`SEEK_STEP`]s of slack so `seek_abs(total_duration())` and
    /// `Playable::seek_to(1.0)` land on [`Phase::Done`].
    pub fn total_duration(&self) -> f32 {
        self.travel_duration()
            + self.params.impact_duration()
            + self.params.fade_duration()
            + SEEK_STEP * 2.0
    }

    fn advance(&mut self, dt: f32) -> bool {
        let charge = self.params.charge_duration();
        if self.age < charge {
            return false;
        }
        let speed = self.params.speed * self.params.speed_scale;
        let since = self.age - charge;
        let ease_in = out_quad(saturate(since / 0.05));
        self.front += speed * ease_in * dt;
        let previous = self.u;
        self.u = saturate(self.front / self.length);
        previous < 1.0 && self.u >= 1.0
    }

    fn step(&mut self, dt: f32) {
        if !self.is_active() {
            return;
        }
        self.age += dt;
        match self.phase {
            Phase::Charge => {
                self.update_light(dt);
                if self.age >= self.params.charge_duration() {
                    self.phase = Phase::Travel;
                    self.fired = true;
                    self.light_boost = self.params.light_intensity * 0.55;
                    self.events.push(NovaEvent::Release);
                    // Consume the same dt's travel advance so seek parity
                    // matches BeamAbility's hold-then-release within one tick.
                    let reached = self.advance(dt);
                    if reached {
                        self.phase = Phase::Impact;
                        self.impact_time = 0.0;
                        self.light_boost = self.params.light_intensity * 1.2;
                        self.events.push(NovaEvent::Impact);
                    }
                }
            }
            Phase::Travel => {
                if !self.fired {
                    self.fired = true;
                    self.light_boost = self.params.light_intensity * 0.55;
                    self.events.push(NovaEvent::Release);
                }
                let reached = self.advance(dt);
                self.update_light(dt);
                if reached {
                    self.phase = Phase::Impact;
                    self.impact_time = 0.0;
                    self.light_boost = self.params.light_intensity * 1.2;
                    self.events.push(NovaEvent::Impact);
                }
            }
            Phase::Impact => {
                self.impact_time += dt;
                let t = saturate(self.impact_time / self.params.impact_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Fade;
                    self.fade_time = 0.0;
                    self.events.push(NovaEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(NovaEvent::Done);
                }
            }
            Phase::Idle | Phase::Done => {}
        }
    }

    fn update_light(&mut self, dt: f32) {
        self.light_boost = (self.light_boost - self.light_boost * 4.5 * dt - 0.5 * dt).max(0.0);
    }

    /// Beam hum — a slow sine, not a quantised stutter. Mirrors
    /// `BeamAbility#lightShimmer`.
    fn shimmer(&self) -> f32 {
        let pulse = saturate(self.params.light_pulse);
        let speed = self.params.light_pulse_speed.max(0.0);
        1.0 - pulse * (0.5 - 0.5 * libm::cosf(self.age * speed * TAU))
    }

    fn light_scale(&self) -> f32 {
        match self.phase {
            Phase::Charge => 0.35 + 0.65 * self.charge(),
            Phase::Travel => 1.0,
            Phase::Impact => {
                let t = saturate(self.impact_time / self.params.impact_duration());
                1.0 - in_cubic(t) * 0.25
            }
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                (1.0 - t) * 0.4
            }
            Phase::Idle | Phase::Done => 0.0,
        }
    }

    /// Current beam dynamic-light state (tracks the axis tip / impact).
    pub fn light(&self) -> NovaLight {
        let tip = match self.phase {
            Phase::Charge => self.hand_point(),
            _ => self.axis_at(self.progress()),
        };
        NovaLight {
            x: tip[0],
            y: tip[1],
            z: tip[2],
            intensity: self.params.light_intensity * self.light_scale() * self.shimmer()
                + self.light_boost,
            radius: self.params.light_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Muzzle light sitting in the caster's hands (charge + early travel).
    pub fn muzzle_light(&self) -> NovaLight {
        let hand = self.hand_point();
        let fade = self.beam_fade();
        let charge = self.charge();
        let active = matches!(self.phase, Phase::Charge | Phase::Travel | Phase::Impact);
        let intensity = if active {
            self.params.muzzle_light_intensity * charge * fade
        } else {
            0.0
        };
        NovaLight {
            x: hand[0],
            y: hand[1],
            z: hand[2],
            intensity,
            radius: self.params.muzzle_light_radius,
        }
    }

    /// Drain phase-transition events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<NovaEvent> {
        core::mem::take(&mut self.events)
    }

    /// Seek to an absolute cast time in seconds.
    ///
    /// Deterministic: dynamic state resets to spawn (dice are kept) and the
    /// shared step body re-simulates at [`SEEK_STEP`]. Tube / ring metres are
    /// not stored — they resolve at sample time from `(seed, params, time)`.
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle && self.rings.is_empty() {
            return;
        }
        if self.rings.is_empty() {
            return;
        }
        let target = time.max(0.0);
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = 0.0;
        self.fired = false;
        self.phase = Phase::Charge;
        self.events.clear();
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    /// Resolve the parametric tube against live params at the current age.
    pub fn tube(&self) -> Vec<TubeNode> {
        if matches!(self.phase, Phase::Idle) && self.rings.is_empty() {
            return Vec::new();
        }
        let progress = self.progress();
        let width = self.width_fade();
        let mut nodes = Vec::with_capacity(TUBE_SEGMENTS + 1);
        for i in 0..=TUBE_SEGMENTS {
            let s = i as f32 / TUBE_SEGMENTS as f32;
            let p = self.axis_at(s);
            let drawn = if s <= progress + 1e-4 { 1.0 } else { 0.0 };
            nodes.push(TubeNode {
                s,
                x: p[0],
                y: p[1],
                z: p[2],
                radius: beam_radius(s, &self.params) * width,
                drawn,
            });
        }
        nodes
    }

    /// Resolve every active shock disc against the live params at the current
    /// age.
    pub fn rings(&self) -> Vec<RingSample> {
        // During charge the column has not left the hand — discs stay off.
        if matches!(self.phase, Phase::Idle | Phase::Charge) {
            return Vec::new();
        }
        if self.rings.is_empty() {
            return Vec::new();
        }
        let budget = self.params.ring_budget();
        let fade = self.beam_fade();
        let progress = self.progress();
        self.rings
            .iter()
            .take(budget)
            .map(|r| {
                sample_ring(
                    r,
                    &self.params,
                    self.origin,
                    self.direction,
                    self.side,
                    self.length,
                    self.age,
                    progress,
                    fade,
                )
            })
            .filter(|s| s.alpha > 1e-4)
            .collect()
    }

    /// Charge-orb sample (hand pose + size).
    pub fn orb(&self) -> OrbSample {
        let hand = self.hand_point();
        let charge = self.charge();
        let width = self.width_fade();
        let swell = 1.0
            + self.params.orb_throb
                * libm::sinf(self.age * self.params.orb_throb_speed * TAU);
        let size = self.params.orb_size * (0.28 + 0.72 * out_cubic(charge)) * swell * width;
        let visible = matches!(
            self.phase,
            Phase::Charge | Phase::Travel | Phase::Impact | Phase::Fade
        ) && size > 0.01;
        OrbSample {
            x: hand[0],
            y: hand[1],
            z: hand[2],
            radius: size.max(0.01),
            charge,
            visible,
        }
    }

    /// Return to the pool.
    pub fn destroy(&mut self) {
        self.rings.clear();
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = 0.0;
        self.fired = false;
        self.phase = Phase::Idle;
        self.events.clear();
    }
}

impl Default for NovaBeam {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for NovaBeam {
    fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }
}

impl Playable for NovaBeam {
    fn duration(&self) -> f32 {
        self.total_duration()
    }

    fn reset(&mut self) {
        self.seek_abs(0.0);
    }

    fn seek_to(&mut self, progress: f32) {
        self.seek_abs(saturate(progress) * self.total_duration());
    }

    fn is_complete(&self) -> bool {
        self.phase == Phase::Done
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cast() -> NovaBeam {
        let mut beam = NovaBeam::new();
        beam.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        beam
    }

    #[test]
    fn too_close_is_refused() {
        let mut beam = NovaBeam::new();
        let err = beam.cast([0.0, 0.0], [0.0, 1.0], 1.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooClose {
                raw: 1.0,
                min: 3.0
            }
        );
        assert_eq!(beam.phase(), Phase::Idle);
    }

    #[test]
    fn full_lifecycle_reaches_done() {
        let mut beam = cast();
        assert_eq!(beam.phase(), Phase::Charge);
        let total = beam.total_duration();
        assert!(total > 1.0 && total < 20.0);
        let mut guard = 0;
        while beam.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 60, "pipeline did not finish");
        }
        assert_eq!(beam.phase(), Phase::Done);
        assert!(beam.is_complete());
        assert!(!beam.is_active());
        let events = beam.drain_events();
        assert!(events.contains(&NovaEvent::Release));
        assert!(events.contains(&NovaEvent::Impact));
        assert!(events.contains(&NovaEvent::Fade));
        assert!(events.contains(&NovaEvent::Done));
    }

    #[test]
    fn charge_holds_front_then_releases() {
        let mut beam = cast();
        let charge_t = beam.params().charge_duration();
        beam.seek_abs(charge_t * 0.5);
        assert_eq!(beam.phase(), Phase::Charge);
        assert_eq!(beam.front(), 0.0);
        assert!(beam.charge() > 0.4 && beam.charge() < 0.6);
        assert!(beam.orb().visible);
        assert!(beam.muzzle_light().intensity > 0.0);

        beam.seek_abs(charge_t + 0.02);
        assert_eq!(beam.phase(), Phase::Travel);
        assert!(beam.front() > 0.0 || beam.u() > 0.0);
    }

    #[test]
    fn seek_is_deterministic_and_terminal() {
        let mut a = cast();
        let mut b = cast();
        a.seek_abs(0.55);
        b.seek_abs(0.55);
        assert_eq!(a.front(), b.front());
        assert_eq!(a.u(), b.u());
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.tube(), b.tube());
        assert_eq!(a.rings(), b.rings());
        assert_eq!(a.orb(), b.orb());

        a.seek_abs(0.0);
        assert_eq!(a.phase(), Phase::Charge);
        assert_eq!(a.front(), 0.0);

        a.seek_abs(a.total_duration() + 5.0);
        assert_eq!(a.phase(), Phase::Done);
        assert!(a.is_complete());
    }

    #[test]
    fn seek_matches_fine_stepped_playback() {
        let mut live = cast();
        let total = live.total_duration();
        let target = total * 0.4;
        let dt = 1.0 / 480.0;
        let mut t = 0.0;
        while t < target {
            live.update(dt);
            t += dt;
        }
        let mut sought = cast();
        sought.seek_abs(target);
        assert!((live.front() - sought.front()).abs() < 0.05);
        assert!((live.u() - sought.u()).abs() < 0.01);
        assert_eq!(live.phase(), sought.phase());
    }

    #[test]
    fn ring_sample_determinism() {
        let mut beam = cast();
        beam.seek_abs(beam.travel_duration() + 0.15);
        assert_eq!(beam.phase(), Phase::Impact);
        let a = beam.rings();
        let b = beam.rings();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        let t0 = beam.age();
        beam.seek_abs(t0 + 0.4);
        let c = beam.rings();
        // Rings crawl; at least one station should move.
        assert!(a.iter().zip(c.iter()).any(|(x, y)| (x.s - y.s).abs() > 1e-4));
        beam.seek_abs(t0);
        assert_eq!(beam.rings(), a);
    }

    #[test]
    fn params_edit_reshapes_standing_beam_while_paused() {
        let mut beam = cast();
        beam.seek_abs(beam.travel_duration() + 0.1);
        let before: Vec<f32> = beam.tube().iter().map(|n| n.radius).collect();
        beam.params_mut().radius = 1.6;
        beam.params_mut().flare = 3.0;
        let after: Vec<f32> = beam.tube().iter().map(|n| n.radius).collect();
        assert_eq!(before.len(), after.len());
        assert!(before.iter().zip(after.iter()).any(|(b, a)| (b - a).abs() > 1e-4));
    }

    #[test]
    fn playable_trait_drives_progress() {
        use animato_core::Playable as _;
        let mut beam = cast();
        assert_eq!(Playable::duration(&beam), beam.total_duration());
        Playable::seek_to(&mut beam, 0.5);
        assert!(beam.age() > 0.0 && !beam.is_complete());
        Playable::seek_to(&mut beam, 1.0);
        assert!(beam.is_complete());
        Playable::reset(&mut beam);
        assert_eq!(beam.age(), 0.0);
        assert_eq!(beam.phase(), Phase::Charge);
        assert!(Playable::as_any(&beam).is::<NovaBeam>());
        assert!(Playable::as_any_mut(&mut beam).is::<NovaBeam>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let beam = cast();
        let total = beam.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "nova-beam", beam, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.25);
        let inner = comp.get::<NovaBeam>("nova-beam").unwrap();
        assert!(inner.age() > 0.0);
        assert!(!inner.tube().is_empty());
        comp.seek_abs(total + 1.0);
        assert!(comp.get::<NovaBeam>("nova-beam").unwrap().is_complete());
    }

    #[test]
    fn tube_progress_tracks_front() {
        let mut beam = cast();
        beam.seek_abs(beam.params().charge_duration() + 0.05);
        let prog = beam.progress();
        assert!(prog > 0.0 && prog < 1.0);
        let tube = beam.tube();
        assert!(!tube.is_empty());
        let drawn: Vec<_> = tube.iter().filter(|n| n.drawn > 0.5).collect();
        let ahead: Vec<_> = tube.iter().filter(|n| n.drawn < 0.5).collect();
        assert!(!drawn.is_empty());
        assert!(!ahead.is_empty());
        assert!(drawn.iter().all(|n| n.s <= prog + 1e-3));
    }
}
