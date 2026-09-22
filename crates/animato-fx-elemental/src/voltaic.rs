//! The Voltaic Snare cast pipeline: zone aim, leash travel, cage snap and light
//! bookkeeping.
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `SnareAbility.js` — the sandbox's first **far cast**:
//! a circle planted at a floor point. Everything is a pure function of
//! `(seed, params, time)`: [`VoltaicSnare::seek_abs`] deterministically
//! re-simulates from spawn, so the pipeline can be driven by
//! [`animato_composition::Composition`] — including while paused, with params
//! edited live (cage records store dice only; polylines resolve at sample time
//! scaled by live [`VoltaicSnareParams::zone_radius`]).
//!
//! Renderer-owned systems from the original (GPU particles, ground decals,
//! burst shells, camera shake, screen flash, ribbon / field materials) are
//! intentionally *not* reimplemented here: the pipeline exposes every stimulus
//! they need ([`VoltaicSnare::samples`], [`VoltaicSnare::drain_events`],
//! [`VoltaicSnare::light`], [`VoltaicSnare::center`], [`VoltaicSnare::open`]).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;

use crate::aim::{ZoneAimSolution, solve_zone_aim, solve_zone_aim_at};
use crate::cage::{
    CageContext, CageRecord, CageSample, climb_amount, open_amount, roll_cage, sample_cage,
};
use crate::math::{in_cubic, in_quad, out_quad, saturate};
use crate::params::VoltaicSnareParams;
use crate::pipeline::{Phase, SEEK_STEP, SpawnError};

/// Phase-transition events for renderer-owned systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoltaicEvent {
    /// Leash arrived; cage snaps open at [`VoltaicSnare::center`].
    Impact,
    /// Collapse started.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast (quantised gutter like Storm Lance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoltaicLight {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// World z.
    pub z: f32,
    /// Effective intensity.
    pub intensity: f32,
    /// Effective radius, metres.
    pub radius: f32,
}

/// One complete Voltaic Snare pipeline: zone aim + dice + phase machine.
#[derive(Clone, Debug)]
pub struct VoltaicSnare {
    params: VoltaicSnareParams,
    seed: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    center: [f32; 2],
    length: f32,
    zone_radius: f32,
    front: f32,
    u: f32,
    age: f32,
    impact_time: f32,
    fade_time: f32,
    open_time: f32,
    light_boost: f32,
    phase: Phase,
    cage: Vec<CageRecord>,
    events: Vec<VoltaicEvent>,
}

impl VoltaicSnare {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(VoltaicSnareParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: VoltaicSnareParams) -> Self {
        Self {
            params,
            seed: 0.0,
            origin: [0.0, 0.0],
            direction: [0.0, 1.0],
            side: [1.0, 0.0],
            center: [0.0, 0.0],
            length: 1.0,
            zone_radius: 4.4,
            front: 0.0,
            u: 0.0,
            age: 0.0,
            impact_time: 0.0,
            fade_time: 0.0,
            open_time: 0.0,
            light_boost: 0.0,
            phase: Phase::Idle,
            cage: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Live params (edit-while-paused reshapes the standing cage).
    pub fn params(&self) -> &VoltaicSnareParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut VoltaicSnareParams {
        &mut self.params
    }

    /// Current phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// `true` while travel / impact / fade.
    pub fn is_active(&self) -> bool {
        matches!(self.phase, Phase::Travel | Phase::Impact | Phase::Fade)
    }

    /// Metres the leash tip has travelled.
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

    /// Cast distance (origin → centre), metres.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// Dice seed as `f32`.
    pub fn seed(&self) -> f32 {
        self.seed
    }

    /// Planted trap centre on the floor.
    pub fn center(&self) -> [f32; 2] {
        self.center
    }

    /// Live footprint from params (`zone_radius`), metres.
    pub fn zone_radius(&self) -> f32 {
        self.params.radius()
    }

    /// Seconds since the ring started opening (impact + fade).
    pub fn open_time(&self) -> f32 {
        self.open_time
    }

    /// Snap-open amount `0..~1.16` (0 while travelling).
    pub fn open(&self) -> f32 {
        if self.phase == Phase::Travel || self.phase == Phase::Idle {
            0.0
        } else {
            open_amount(self.open_time, &self.params)
        }
    }

    /// Column climb amount `0..1` (0 while travelling).
    pub fn climb(&self) -> f32 {
        if self.phase == Phase::Travel || self.phase == Phase::Idle {
            0.0
        } else {
            climb_amount(self.open_time, &self.params)
        }
    }

    /// Cage dice for the current cast.
    pub fn cage(&self) -> &[CageRecord] {
        &self.cage
    }

    /// Begin a cast from a solved zone aim. Refuses too-close and too-far.
    pub fn spawn(&mut self, aim: &ZoneAimSolution, seed: u64) -> Result<(), SpawnError> {
        if aim.too_close {
            return Err(SpawnError::TooClose {
                raw: aim.raw_distance,
                min: self.params.min_range,
            });
        }
        if aim.too_far || !aim.valid {
            return Err(SpawnError::TooFar {
                raw: aim.raw_distance,
                max: self.params.range,
            });
        }
        self.seed = seed as f32;
        self.origin = aim.origin;
        self.direction = aim.direction;
        self.side = [aim.direction[1], -aim.direction[0]];
        self.center = aim.center;
        self.length = aim.distance.max(0.1);
        self.zone_radius = aim.zone_radius;

        self.cage = roll_cage(&self.params, seed);

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        // Muzzle punch as the leash leaves the hand.
        self.light_boost = self.params.light_intensity * 0.7;
        self.phase = Phase::Travel;
        self.events.clear();
        Ok(())
    }

    /// Convenience: solve zone aim from heading + raw distance, then spawn.
    pub fn cast(
        &mut self,
        origin: [f32; 2],
        direction: [f32; 2],
        raw_distance: f32,
        seed: u64,
    ) -> Result<(), SpawnError> {
        let aim = solve_zone_aim(&self.params, origin, direction, raw_distance);
        self.spawn(&aim, seed)
    }

    /// Convenience: solve zone aim from an absolute floor aim-point, then spawn.
    pub fn cast_at(
        &mut self,
        origin: [f32; 2],
        aim_point: [f32; 2],
        seed: u64,
    ) -> Result<(), SpawnError> {
        let aim = solve_zone_aim_at(&self.params, origin, aim_point);
        self.spawn(&aim, seed)
    }

    /// A point on the floor cast line; `s` is `0..1` along it.
    pub fn point_at(&self, s: f32) -> [f32; 2] {
        [
            self.origin[0] + self.direction[0] * s * self.length,
            self.origin[1] + self.direction[1] * s * self.length,
        ]
    }

    /// Travelling leash tip on the floor plane.
    pub fn front_position(&self) -> [f32; 2] {
        self.point_at(self.u)
    }

    /// Hand origin in world space.
    pub fn hand_point(&self) -> [f32; 3] {
        let p = &self.params;
        [
            self.origin[0] + self.direction[0] * p.hand_forward + self.side[0] * p.hand_side,
            p.hand_height,
            self.origin[1] + self.direction[1] * p.hand_forward + self.side[1] * p.hand_side,
        ]
    }

    /// Leash tip in world space (pinned to centre once arrived).
    pub fn front_point_3d(&self) -> [f32; 3] {
        let u = if self.phase == Phase::Travel {
            self.u
        } else {
            1.0
        };
        let p = self.point_at(u);
        [p[0], self.params.leash_cling, p[1]]
    }

    /// Cage fade `1 → 0` through collapse (cubic; hangs on then goes).
    pub fn cage_fade(&self) -> f32 {
        match self.phase {
            Phase::Travel | Phase::Impact => 1.0,
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                1.0 - in_cubic(t)
            }
            Phase::Idle | Phase::Charge | Phase::Done => 0.0,
        }
    }

    /// Seconds the leash needs to reach the centre at current speed.
    pub fn travel_duration(&self) -> f32 {
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

    /// Total finite duration: travel + impact + fade (+ seek slack).
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
                    self.open_time = 0.0;
                    self.light_boost = self.params.light_intensity * 1.4;
                    self.events.push(VoltaicEvent::Impact);
                }
            }
            Phase::Impact => {
                self.impact_time += dt;
                self.open_time += dt;
                let t = saturate(self.impact_time / self.params.impact_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Fade;
                    self.fade_time = 0.0;
                    self.events.push(VoltaicEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                self.open_time += dt;
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(VoltaicEvent::Done);
                }
            }
            Phase::Idle | Phase::Charge | Phase::Done => {}
        }
    }

    fn update_light(&mut self, dt: f32) {
        self.light_boost =
            (self.light_boost - self.light_boost * 4.5 * dt - 0.5 * dt).max(0.0);
    }

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
                1.0 - in_quad(t) * 0.35
            }
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                (1.0 - t) * 0.4
            }
            Phase::Idle | Phase::Charge | Phase::Done => 0.0,
        }
    }

    /// Current dynamic-light state.
    pub fn light(&self) -> VoltaicLight {
        let (x, y, z) = if self.phase == Phase::Travel {
            let tip = self.front_point_3d();
            (tip[0], tip[1] + 0.3, tip[2])
        } else {
            let h = self.params.height * self.climb() * saturate(self.params.light_height);
            (self.center[0], h, self.center[1])
        };
        VoltaicLight {
            x,
            y,
            z,
            intensity: self.params.light_intensity * self.light_scale() * self.shimmer()
                + self.light_boost,
            radius: self.params.light_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Drain phase-transition events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<VoltaicEvent> {
        core::mem::take(&mut self.events)
    }

    /// Seek to an absolute cast time in seconds.
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle && self.cage.is_empty() {
            return;
        }
        if self.cage.is_empty() {
            return;
        }
        let target = time.max(0.0);
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.7;
        self.phase = Phase::Travel;
        self.events.clear();
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    fn sample_context(&self) -> CageContext {
        let open = self.open();
        let climb = self.climb();
        CageContext {
            center: self.center,
            hand: self.hand_point(),
            front: self.front_point_3d(),
            radius: (self.params.radius() * open).max(0.05),
            height: (self.params.height * climb).max(0.05),
            age: self.age,
            seed: self.seed,
            fade: self.cage_fade(),
        }
    }

    /// Resolve every active cage filament against live params.
    pub fn samples(&self) -> Vec<CageSample> {
        if matches!(self.phase, Phase::Idle) && self.cage.is_empty() {
            return Vec::new();
        }
        if matches!(self.phase, Phase::Done) {
            return Vec::new();
        }
        let traveling = self.phase == Phase::Travel;
        sample_cage(&self.cage, &self.params, &self.sample_context(), traveling)
    }

    /// Advance by `dt` seconds. Returns `true` while still active.
    pub fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }

    /// Return to the pool.
    pub fn destroy(&mut self) {
        self.cage.clear();
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = 0.0;
        self.phase = Phase::Idle;
        self.events.clear();
    }
}

impl Default for VoltaicSnare {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for VoltaicSnare {
    fn update(&mut self, dt: f32) -> bool {
        self.update(dt)
    }
}

impl Playable for VoltaicSnare {
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

    fn cast() -> VoltaicSnare {
        let mut snare = VoltaicSnare::new();
        snare.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        snare
    }

    #[test]
    fn too_close_is_refused_when_min_range_set() {
        let mut snare = VoltaicSnare::with_params(VoltaicSnareParams {
            min_range: 3.0,
            ..VoltaicSnareParams::default()
        });
        let err = snare.cast([0.0, 0.0], [0.0, 1.0], 1.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooClose {
                raw: 1.0,
                min: 3.0
            }
        );
        assert_eq!(snare.phase(), Phase::Idle);
    }

    #[test]
    fn too_far_is_refused() {
        let mut snare = VoltaicSnare::new();
        let err = snare.cast([0.0, 0.0], [0.0, 1.0], 99.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooFar {
                raw: 99.0,
                max: 20.0
            }
        );
        assert_eq!(snare.phase(), Phase::Idle);
    }

    #[test]
    fn full_lifecycle_reaches_done() {
        let mut snare = cast();
        let total = snare.total_duration();
        assert!(total > 1.0 && total < 30.0);
        let mut guard = 0;
        while snare.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 60, "pipeline did not finish");
        }
        assert_eq!(snare.phase(), Phase::Done);
        assert!(snare.is_complete());
        let events = snare.drain_events();
        assert!(events.contains(&VoltaicEvent::Impact));
        assert!(events.contains(&VoltaicEvent::Fade));
        assert!(events.contains(&VoltaicEvent::Done));
    }

    #[test]
    fn seek_is_deterministic_and_terminal() {
        let mut a = cast();
        let mut b = cast();
        a.seek_abs(0.8);
        b.seek_abs(0.8);
        assert_eq!(a.front(), b.front());
        assert_eq!(a.u(), b.u());
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.open_time(), b.open_time());
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
        assert!((live.open_time() - sought.open_time()).abs() < 0.02);
    }

    #[test]
    fn params_edit_reshapes_standing_cage_while_paused() {
        let mut snare = cast();
        snare.seek_abs(snare.travel_duration() + 0.3);
        assert_eq!(snare.phase(), Phase::Impact);
        let before: Vec<(f32, f32)> = snare
            .samples()
            .iter()
            .flat_map(|s| s.nodes.iter().map(|n| (n.x, n.z)))
            .collect();
        snare.params_mut().zone_radius *= 1.8;
        let after: Vec<(f32, f32)> = snare
            .samples()
            .iter()
            .flat_map(|s| s.nodes.iter().map(|n| (n.x, n.z)))
            .collect();
        assert_eq!(before.len(), after.len());
        assert!(before.iter().zip(after.iter()).any(|(b, a)| {
            let db = libm::sqrtf(b.0 * b.0 + b.1 * b.1);
            let da = libm::sqrtf(a.0 * a.0 + a.1 * a.1);
            (da - db).abs() > 0.05
        }));
    }

    #[test]
    fn sample_determinism_under_zone_radius_scale() {
        let mut snare = cast();
        snare.seek_abs(snare.travel_duration() + 0.25);
        let a = snare.samples();
        let b = snare.samples();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        // Traveling samples are leash-only; standing samples exclude leash.
        assert!(a.iter().all(|s| s.role != crate::cage::FilamentRole::Leash));
    }

    #[test]
    fn cast_at_plants_centre_on_aim_point() {
        let mut snare = VoltaicSnare::new();
        snare
            .cast_at([0.0, 0.0], [3.0, 9.0], 11)
            .expect("in range");
        let c = snare.center();
        assert!((c[0] - 3.0).abs() < 1e-3);
        assert!((c[1] - 9.0).abs() < 1e-3);
    }

    #[test]
    fn playable_trait_drives_progress() {
        let mut snare = cast();
        assert_eq!(Playable::duration(&snare), snare.total_duration());
        Playable::seek_to(&mut snare, 0.5);
        assert!(snare.age() > 0.0 && !snare.is_complete());
        Playable::seek_to(&mut snare, 1.0);
        assert!(snare.is_complete());
        Playable::reset(&mut snare);
        assert_eq!(snare.age(), 0.0);
        assert!(Playable::as_any(&snare).is::<VoltaicSnare>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let snare = cast();
        let total = snare.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "voltaic-snare", snare, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.25);
        let inner = comp.get::<VoltaicSnare>("voltaic-snare").unwrap();
        assert!(inner.age() > 0.0);
        assert!(!inner.samples().is_empty());
        comp.seek_abs(total + 1.0);
        assert!(comp
            .get::<VoltaicSnare>("voltaic-snare")
            .unwrap()
            .is_complete());
    }
}
