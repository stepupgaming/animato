//! The Storm Lance cast pipeline: phase machine, strike front, filament
//! bundle and light bookkeeping.
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `ThunderAbility.js` (hand/impact axis, filament
//! restrike, quantised light flicker). Everything is a pure function of
//! `(seed, params, time)`: [`StormLance::seek_abs`] deterministically
//! re-simulates from spawn, so the pipeline can be driven by
//! [`animato_composition::Composition`] — including while paused, with params
//! edited live (strand records store dice only; polylines resolve at sample
//! time with jitter octaves + crawl, and restrike re-rolls shape from
//! `seed + tick`).
//!
//! Renderer-owned systems from the original (GPU particles, ground decals,
//! burst shells, camera shake, screen flash, ribbon materials) are
//! intentionally *not* reimplemented here: the pipeline exposes every
//! stimulus they need ([`StormLance::front_position`],
//! [`StormLance::samples`], [`StormLance::drain_events`],
//! [`StormLance::light`]).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;

use crate::aim::{AimSolution, solve_aim};
use crate::filament::{
    StrandRecord, StrandSample, axis_point, bolt_flicker, roll_strands, sample_strand,
};
use crate::math::{in_cubic, in_quad, out_quad, saturate};
use crate::params::StormLanceParams;
use crate::pipeline::{Phase, SEEK_STEP, SpawnError};

/// Phase-transition events for renderer-owned systems (bursts, decals, shake,
/// flash). Drained via [`StormLance::drain_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StormEvent {
    /// The strike front reached the far end; fire impact FX at
    /// [`StormLance::impact_position`].
    Impact,
    /// Blow-out started; thin ambient emission.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast, mirroring `Ability#_updateLight` with
/// Thunder's quantised gutter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StormLight {
    /// World x of the light (tracks the axis tip, then the impact point).
    pub x: f32,
    /// World y of the light (lifted onto the bolt axis).
    pub y: f32,
    /// World z of the light.
    pub z: f32,
    /// Effective intensity (base × phase scale × shimmer + boost).
    pub intensity: f32,
    /// Effective radius in metres.
    pub radius: f32,
}

/// One complete Storm Lance pipeline: dice + phase machine + strike front.
///
/// `Playable + Send + 'static`, so it slots directly into
/// `animato_composition::Composition::add` / `animato_timeline::Timeline`.
#[derive(Clone, Debug)]
pub struct StormLance {
    params: StormLanceParams,
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
    strands: Vec<StrandRecord>,
    events: Vec<StormEvent>,
}

impl StormLance {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(StormLanceParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: StormLanceParams) -> Self {
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
            strands: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Live params. Edits apply to the *standing* cast on the next sample —
    /// the edit-while-paused rule from the original.
    pub fn params(&self) -> &StormLanceParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut StormLanceParams {
        &mut self.params
    }

    /// Current phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// `true` while a cast is in flight (travel, impact or fade).
    pub fn is_active(&self) -> bool {
        matches!(self.phase, Phase::Travel | Phase::Impact | Phase::Fade)
    }

    /// Metres the strike front has travelled.
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

    /// Strand dice for the current cast.
    pub fn strands(&self) -> &[StrandRecord] {
        let budget = self.params.strand_budget().min(self.strands.len());
        &self.strands[..budget]
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

        let budget = self.params.strand_budget();
        self.strands = roll_strands(budget, seed);

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        // Muzzle punch, mirrors ThunderAbility#_muzzleFx.
        self.light_boost = self.params.light_intensity * 0.8;
        self.phase = Phase::Travel;
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

    /// Hand origin in world space (3D), mirrors `ThunderAbility#_handPoint`.
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

    /// Axis point at `s` along the bolt (hand → impact), including sag.
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

    /// How much of the bolt exists (`u` while travelling, else `1`).
    pub fn progress(&self) -> f32 {
        if self.phase == Phase::Travel {
            self.u
        } else if matches!(self.phase, Phase::Idle) {
            0.0
        } else {
            1.0
        }
    }

    /// Bolt fade `1 → 0` through the blow-out (cubic; hangs on then goes).
    pub fn bolt_fade(&self) -> f32 {
        match self.phase {
            Phase::Travel | Phase::Impact => 1.0,
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                1.0 - in_cubic(t)
            }
            Phase::Idle | Phase::Done => 0.0,
        }
    }

    /// Seconds the front needs to cross the current length at current speed.
    pub fn travel_duration(&self) -> f32 {
        // Mirror `step`: age advances *before* `advance`, so the ease-in is
        // keyed off the post-increment age (never zero on the first tick).
        let speed = (self.params.speed * self.params.speed_scale).max(1e-6);
        let mut front = 0.0f32;
        let mut age = 0.0f32;
        loop {
            age += SEEK_STEP;
            let ease_in = out_quad(saturate(age / 0.08));
            front += speed * ease_in * SEEK_STEP;
            if front >= self.length || age > 3600.0 {
                break;
            }
        }
        age
    }

    /// Total finite duration: travel + impact + fade.
    ///
    /// Adds two [`SEEK_STEP`]s of slack so `seek_abs(total_duration())` and
    /// `Playable::seek_to(1.0)` land on [`Phase::Done`]: `impact_duration` /
    /// `fade_duration` are not exact multiples of the seek step in `f32`, so a
    /// bare sum can leave the pipeline one tick short of the fade boundary.
    pub fn total_duration(&self) -> f32 {
        self.travel_duration()
            + self.params.impact_duration()
            + self.params.fade_duration()
            + SEEK_STEP * 2.0
    }

    fn advance(&mut self, dt: f32) -> bool {
        let speed = self.params.speed * self.params.speed_scale;
        let ease_in = out_quad(saturate(self.age / 0.08));
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
            Phase::Travel => {
                let reached = self.advance(dt);
                self.update_light(dt);
                if reached {
                    self.phase = Phase::Impact;
                    self.impact_time = 0.0;
                    self.light_boost = self.params.light_intensity * 1.5;
                    self.events.push(StormEvent::Impact);
                }
            }
            Phase::Impact => {
                self.impact_time += dt;
                let t = saturate(self.impact_time / self.params.impact_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Fade;
                    self.fade_time = 0.0;
                    self.events.push(StormEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(StormEvent::Done);
                }
            }
            Phase::Idle | Phase::Done => {}
        }
    }

    fn update_light(&mut self, dt: f32) {
        self.light_boost = (self.light_boost - self.light_boost * 4.5 * dt - 0.5 * dt).max(0.0);
    }

    /// Lightning gutters where ice glints — a hard, quantised stutter.
    /// Mirrors `ThunderAbility#lightShimmer`.
    fn shimmer(&self) -> f32 {
        let step = libm::floorf(self.age * self.params.light_flicker_speed.max(1.0));
        let s = libm::sinf(step * 127.1) * 43758.5453;
        let noise = libm::fabsf(s) - libm::floorf(libm::fabsf(s));
        1.0 - saturate(self.params.light_flicker) * noise
    }

    fn light_scale(&self) -> f32 {
        match self.phase {
            Phase::Travel => 1.0,
            Phase::Impact => {
                let t = saturate(self.impact_time / self.params.impact_duration());
                1.0 - in_quad(t) * 0.45
            }
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                (1.0 - t) * 0.35
            }
            Phase::Idle | Phase::Done => 0.0,
        }
    }

    /// Current dynamic-light state (position tracks the axis tip).
    pub fn light(&self) -> StormLight {
        let tip = self.axis_at(self.progress());
        StormLight {
            x: tip[0],
            y: tip[1],
            z: tip[2],
            intensity: self.params.light_intensity * self.light_scale() * self.shimmer()
                + self.light_boost,
            radius: self.params.light_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Drain phase-transition events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<StormEvent> {
        core::mem::take(&mut self.events)
    }

    /// Seek to an absolute cast time in seconds.
    ///
    /// Deterministic: dynamic state resets to spawn (dice are kept) and the
    /// shared step body re-simulates at [`SEEK_STEP`]. Filament polylines are
    /// not stored — they resolve at sample time from `(seed, restrike tick,
    /// params)`, so the same time always yields the same bolt.
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle && self.strands.is_empty() {
            return;
        }
        if self.strands.is_empty() {
            return;
        }
        let target = time.max(0.0);
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.8;
        self.phase = Phase::Travel;
        self.events.clear();
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    /// Resolve every active strand against the live params at the current age.
    pub fn samples(&self) -> Vec<StrandSample> {
        if matches!(self.phase, Phase::Idle) && self.strands.is_empty() {
            return Vec::new();
        }
        let budget = self.params.strand_budget();
        let fade = self.bolt_fade();
        let progress = self.progress();
        let flicker = bolt_flicker(self.age, self.seed, &self.params);
        self.strands
            .iter()
            .take(budget)
            .map(|r| {
                let mut s = sample_strand(
                    r,
                    &self.params,
                    self.origin,
                    self.direction,
                    self.side,
                    self.length,
                    self.age,
                    self.seed,
                    progress,
                    fade * flicker,
                    budget,
                );
                // Fold whole-bolt flicker into flash so renderers see one number.
                s.flash *= flicker;
                s
            })
            .collect()
    }

    /// Return to the pool.
    pub fn destroy(&mut self) {
        self.strands.clear();
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = 0.0;
        self.phase = Phase::Idle;
        self.events.clear();
    }
}

impl Default for StormLance {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for StormLance {
    fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }
}

impl Playable for StormLance {
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
    use crate::filament::restrike_tick;

    fn cast() -> StormLance {
        let mut lance = StormLance::new();
        lance.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        lance
    }

    #[test]
    fn too_close_is_refused() {
        let mut lance = StormLance::new();
        let err = lance.cast([0.0, 0.0], [0.0, 1.0], 1.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooClose {
                raw: 1.0,
                min: 2.0
            }
        );
        assert_eq!(lance.phase(), Phase::Idle);
    }

    #[test]
    fn full_lifecycle_reaches_done() {
        let mut lance = cast();
        let total = lance.total_duration();
        assert!(total > 0.5 && total < 10.0);
        let mut guard = 0;
        while lance.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 60, "pipeline did not finish");
        }
        assert_eq!(lance.phase(), Phase::Done);
        assert!(lance.is_complete());
        assert!(!lance.is_active());
        let events = lance.drain_events();
        assert!(events.contains(&StormEvent::Impact));
        assert!(events.contains(&StormEvent::Fade));
        assert!(events.contains(&StormEvent::Done));
    }

    #[test]
    fn seek_is_deterministic_and_terminal() {
        let mut a = cast();
        let mut b = cast();
        a.seek_abs(0.35);
        b.seek_abs(0.35);
        assert_eq!(a.front(), b.front());
        assert_eq!(a.u(), b.u());
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.samples(), b.samples());

        a.seek_abs(0.0);
        assert_eq!(a.phase(), Phase::Travel);
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
    fn restrike_sample_determinism() {
        let mut lance = cast();
        // Park in Impact so the full bolt is drawn and restrike is ticking.
        lance.seek_abs(lance.travel_duration() + 0.1);
        assert_eq!(lance.phase(), Phase::Impact);
        let tick = restrike_tick(lance.age(), lance.params());
        let a = lance.samples();
        let b = lance.samples();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        // Advance past the next restrike boundary and confirm the shape moves,
        // then seek back and recover the earlier sample.
        let step = 1.0 / lance.params().restrike.max(1.0);
        let t0 = lance.age();
        lance.seek_abs(t0 + step * 1.5);
        let c = lance.samples();
        assert_ne!(
            restrike_tick(lance.age(), lance.params()),
            tick,
            "must cross a restrike boundary"
        );
        assert_ne!(a, c, "restrike must re-roll filament shape");
        lance.seek_abs(t0);
        assert_eq!(lance.samples(), a);
    }

    #[test]
    fn params_edit_reshapes_standing_bolt_while_paused() {
        let mut lance = cast();
        lance.seek_abs(lance.travel_duration() + 0.1);
        let before: Vec<(f32, f32)> = lance.samples()[0]
            .nodes
            .iter()
            .map(|n| (n.x, n.y))
            .collect();
        lance.params_mut().jitter = 1.2;
        let after: Vec<(f32, f32)> = lance.samples()[0]
            .nodes
            .iter()
            .map(|n| (n.x, n.y))
            .collect();
        assert_eq!(before.len(), after.len());
        assert!(before.iter().zip(after.iter()).any(|(b, a)| b != a));
    }

    #[test]
    fn playable_trait_drives_progress() {
        use animato_core::Playable as _;
        let mut lance = cast();
        assert_eq!(Playable::duration(&lance), lance.total_duration());
        Playable::seek_to(&mut lance, 0.5);
        assert!(lance.age() > 0.0 && !lance.is_complete());
        Playable::seek_to(&mut lance, 1.0);
        assert!(lance.is_complete());
        Playable::reset(&mut lance);
        assert_eq!(lance.age(), 0.0);
        assert!(Playable::as_any(&lance).is::<StormLance>());
        assert!(Playable::as_any_mut(&mut lance).is::<StormLance>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let lance = cast();
        let total = lance.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "storm-lance", lance, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.25);
        let inner = comp.get::<StormLance>("storm-lance").unwrap();
        assert!(inner.age() > 0.0);
        assert!(!inner.samples().is_empty());
        comp.seek_abs(total + 1.0);
        assert!(comp.get::<StormLance>("storm-lance").unwrap().is_complete());
    }
}
