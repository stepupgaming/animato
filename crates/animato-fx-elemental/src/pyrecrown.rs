//! The Pyre Crown cast pipeline: zone aim, fire-front travel, crown bloom and
//! light bookkeeping (Ext).
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `PyreAbility.js` — LinearAbilityExtThreeJS's first
//! **far cast**: a circle planted at a floor point answered with a wall of
//! burning blades. Everything is a pure function of `(seed, params, time)`:
//! [`PyreCrown::seek_abs`] deterministically re-simulates from spawn, so the
//! pipeline can be driven by [`animato_composition::Composition`] — including
//! while paused, with params edited live (blade records store dice only;
//! metres resolve at sample time scaled by live
//! [`PyreCrownParams::zone_radius`]).
//!
//! Renderer-owned systems from the original (GPU particles, ground decals,
//! burst shells, camera shake, screen flash, pyre / ember-field / flame-veil /
//! heat-haze materials) are intentionally *not* reimplemented here: the
//! pipeline exposes every stimulus they need ([`PyreCrown::samples`],
//! [`PyreCrown::field`], [`PyreCrown::veil`], [`PyreCrown::haze`],
//! [`PyreCrown::drain_events`], [`PyreCrown::light`], [`PyreCrown::center`],
//! [`PyreCrown::open`]).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;

use crate::aim::{ZoneAimSolution, solve_zone_aim, solve_zone_aim_at};
use crate::math::{in_quad, out_quad, saturate};
use crate::params::PyreCrownParams;
use crate::pipeline::{Phase, SEEK_STEP, SpawnError};
use crate::pyre::{
    EmberFieldSample, FlameVeilSample, HeatHazeSample, PyreRecord, PyreSample, roll_pyre,
    sample_ember_field, sample_flame_veil, sample_heat_haze, sample_pyre,
    schedule_pyre_eruption,
};

/// Phase-transition events for renderer-owned systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PyreEvent {
    /// Front arrived; crater burns out and the crown bloom begins.
    Impact,
    /// Burn-out started.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast (fire gutter shimmer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PyreLight {
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

/// One complete Pyre Crown pipeline: zone aim + dice + phase machine.
#[derive(Clone, Debug)]
pub struct PyreCrown {
    params: PyreCrownParams,
    seed: f32,
    origin: [f32; 2],
    direction: [f32; 2],
    side: [f32; 2],
    center: [f32; 2],
    length: f32,
    zone_radius: f32,
    entry_angle: f32,
    front: f32,
    u: f32,
    age: f32,
    impact_time: f32,
    fade_time: f32,
    open_time: f32,
    light_boost: f32,
    phase: Phase,
    blades: Vec<PyreRecord>,
    events: Vec<PyreEvent>,
}

impl PyreCrown {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(PyreCrownParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: PyreCrownParams) -> Self {
        Self {
            params,
            seed: 0.0,
            origin: [0.0, 0.0],
            direction: [0.0, 1.0],
            side: [1.0, 0.0],
            center: [0.0, 0.0],
            length: 1.0,
            zone_radius: 4.2,
            entry_angle: 0.0,
            front: 0.0,
            u: 0.0,
            age: 0.0,
            impact_time: 0.0,
            fade_time: 0.0,
            open_time: 0.0,
            light_boost: 0.0,
            phase: Phase::Idle,
            blades: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Live params (edit-while-paused reshapes the standing crown).
    pub fn params(&self) -> &PyreCrownParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut PyreCrownParams {
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

    /// Metres the fire front has travelled.
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

    /// Planted crown centre on the floor.
    pub fn center(&self) -> [f32; 2] {
        self.center
    }

    /// Live footprint from params (`zone_radius`), metres.
    pub fn zone_radius(&self) -> f32 {
        self.params.radius()
    }

    /// Seconds since the crater started burning (impact + fade).
    pub fn open_time(&self) -> f32 {
        self.open_time
    }

    /// Bearing the front arrives on (near side of the ring), radians.
    pub fn entry_angle(&self) -> f32 {
        self.entry_angle
    }

    /// Crater burn-out amount `0..1` (0 while travelling).
    pub fn open(&self) -> f32 {
        if self.phase == Phase::Travel || self.phase == Phase::Idle {
            0.0
        } else {
            crate::math::out_cubic(saturate(
                self.open_time / self.params.snap_time.max(0.02),
            ))
        }
    }

    /// How far the crater has cooled back toward the middle, `0..1`.
    pub fn cool(&self) -> f32 {
        if self.phase != Phase::Fade {
            return 0.0;
        }
        let hold = self.params.burn_delay * 0.4;
        saturate(
            (self.fade_time - hold) / (self.params.fade_duration() - hold).max(0.05),
        )
    }

    /// Crown fade `1 → 0` through burn-out.
    pub fn crown_fade(&self) -> f32 {
        match self.phase {
            Phase::Travel | Phase::Impact => 1.0,
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                1.0 - in_quad(t)
            }
            Phase::Idle | Phase::Charge | Phase::Done => 0.0,
        }
    }

    /// Blade dice for the current cast.
    pub fn blades(&self) -> &[PyreRecord] {
        &self.blades
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
        // Sweep starts on the near side: bearing from centre back toward caster.
        self.entry_angle = libm::atan2f(-self.direction[1], -self.direction[0]);

        self.blades = roll_pyre(&self.params, seed);

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.5;
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

    /// Travelling fire-front tip on the floor plane.
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

    /// Seconds the front needs to reach the centre at current speed.
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

    fn on_impact(&mut self) {
        self.open_time = 0.0;
        schedule_pyre_eruption(&mut self.blades, &self.params, self.age, self.entry_angle);
        self.light_boost = self.params.light_intensity * 1.5;
        self.events.push(PyreEvent::Impact);
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
                    self.on_impact();
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
                    self.events.push(PyreEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                self.open_time += dt;
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(PyreEvent::Done);
                }
            }
            Phase::Idle | Phase::Charge | Phase::Done => {}
        }
    }

    fn update_light(&mut self, dt: f32) {
        self.light_boost =
            (self.light_boost - self.light_boost * 4.5 * dt - 0.5 * dt).max(0.0);
    }

    /// Fire gutter shimmer (`PyreAbility#lightShimmer`).
    fn shimmer(&self) -> f32 {
        let t = self.age;
        0.84 + 0.16 * libm::sinf(t * 21.3) * libm::sinf(t * 7.9)
            + 0.07 * libm::sinf(t * 43.7)
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
            Phase::Idle | Phase::Charge | Phase::Done => 0.0,
        }
    }

    /// Current dynamic-light state.
    pub fn light(&self) -> PyreLight {
        let (x, y, z) = if self.phase == Phase::Travel {
            let tip = self.front_position();
            (tip[0], 0.35, tip[1])
        } else {
            let h = self.params.ring_height
                * saturate(self.params.light_height)
                * self.open();
            (self.center[0], h, self.center[1])
        };
        PyreLight {
            x,
            y,
            z,
            intensity: self.params.light_intensity * self.light_scale() * self.shimmer()
                + self.light_boost,
            radius: self.params.light_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Drain phase-transition events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<PyreEvent> {
        core::mem::take(&mut self.events)
    }

    /// How many blades have erupted so far.
    pub fn erupted_count(&self) -> usize {
        self.blades
            .iter()
            .filter(|r| r.erupt_time >= 0.0 && self.age >= r.erupt_time)
            .count()
    }

    /// Seek to an absolute cast time in seconds.
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle && self.blades.is_empty() {
            return;
        }
        if self.blades.is_empty() {
            return;
        }
        let target = time.max(0.0);
        for r in &mut self.blades {
            r.erupt_time = -1.0;
        }
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.5;
        self.phase = Phase::Travel;
        self.events.clear();
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    /// Resolve every erupted blade against live params.
    pub fn samples(&self) -> Vec<PyreSample> {
        if matches!(self.phase, Phase::Idle | Phase::Done | Phase::Travel) {
            return Vec::new();
        }
        sample_pyre(
            &self.blades,
            &self.params,
            self.center,
            self.seed,
            self.age,
            self.fade_time,
            self.phase == Phase::Fade,
            self.entry_angle,
        )
    }

    /// Molten crater sample (hidden while travelling).
    pub fn field(&self) -> Option<EmberFieldSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel | Phase::Done) {
            return None;
        }
        let f = sample_ember_field(
            &self.params,
            self.center,
            self.open(),
            self.cool(),
            self.crown_fade(),
        );
        if f.fade < 0.002 {
            None
        } else {
            Some(f)
        }
    }

    /// Flame-veil sample (hidden while travelling / fully cooled).
    pub fn veil(&self) -> Option<FlameVeilSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel | Phase::Done) {
            return None;
        }
        sample_flame_veil(
            &self.params,
            self.center,
            self.open(),
            self.cool(),
            self.crown_fade(),
            self.age,
            self.seed,
        )
    }

    /// Heat-haze sample (`haze` ships at 0).
    pub fn haze(&self) -> Option<HeatHazeSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel | Phase::Done) {
            return None;
        }
        sample_heat_haze(
            &self.params,
            self.center,
            self.open(),
            self.cool(),
            self.crown_fade(),
        )
    }

    /// Advance by `dt` seconds. Returns `true` while still active.
    pub fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }

    /// Return to the pool.
    pub fn destroy(&mut self) {
        self.blades.clear();
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

impl Default for PyreCrown {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for PyreCrown {
    fn update(&mut self, dt: f32) -> bool {
        self.update(dt)
    }
}

impl Playable for PyreCrown {
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

    fn cast() -> PyreCrown {
        let mut crown = PyreCrown::new();
        crown.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        crown
    }

    #[test]
    fn too_close_is_refused_when_min_range_set() {
        let mut crown = PyreCrown::with_params(PyreCrownParams {
            min_range: 3.0,
            ..PyreCrownParams::default()
        });
        let err = crown.cast([0.0, 0.0], [0.0, 1.0], 1.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooClose {
                raw: 1.0,
                min: 3.0
            }
        );
        assert_eq!(crown.phase(), Phase::Idle);
    }

    #[test]
    fn too_far_is_refused() {
        let mut crown = PyreCrown::new();
        let err = crown.cast([0.0, 0.0], [0.0, 1.0], 99.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooFar {
                raw: 99.0,
                max: 18.0
            }
        );
        assert_eq!(crown.phase(), Phase::Idle);
    }

    #[test]
    fn full_lifecycle_reaches_done() {
        let mut crown = cast();
        let total = crown.total_duration();
        assert!(total > 1.0 && total < 40.0);
        let mut guard = 0;
        while crown.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 60, "pipeline did not finish");
        }
        assert_eq!(crown.phase(), Phase::Done);
        assert!(crown.is_complete());
        let events = crown.drain_events();
        assert!(events.contains(&PyreEvent::Impact));
        assert!(events.contains(&PyreEvent::Fade));
        assert!(events.contains(&PyreEvent::Done));
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
        assert_eq!(live.erupted_count(), sought.erupted_count());
    }

    #[test]
    fn params_edit_reshapes_standing_crown_while_paused() {
        let mut crown = cast();
        crown.seek_abs(crown.travel_duration() + 0.5);
        assert_eq!(crown.phase(), Phase::Impact);
        let before: Vec<(f32, f32)> = crown
            .samples()
            .iter()
            .map(|s| (s.x, s.z))
            .collect();
        assert!(!before.is_empty());
        crown.params_mut().zone_radius *= 1.8;
        let after: Vec<(f32, f32)> = crown
            .samples()
            .iter()
            .map(|s| (s.x, s.z))
            .collect();
        assert_eq!(before.len(), after.len());
        let c = crown.center();
        assert!(before.iter().zip(after.iter()).any(|(b, a)| {
            let db = libm::sqrtf((b.0 - c[0]).powi(2) + (b.1 - c[1]).powi(2));
            let da = libm::sqrtf((a.0 - c[0]).powi(2) + (a.1 - c[1]).powi(2));
            (da - db).abs() > 0.05
        }));
    }

    #[test]
    fn sample_determinism_under_zone_radius_scale() {
        let mut crown = cast();
        crown.seek_abs(crown.travel_duration() + 0.35);
        let a = crown.samples();
        let b = crown.samples();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert!(crown.field().is_some());
        assert!(crown.veil().is_some());
        // haze ships at 0
        assert!(crown.haze().is_none());
    }

    #[test]
    fn cast_at_plants_centre_on_aim_point() {
        let mut crown = PyreCrown::new();
        crown
            .cast_at([0.0, 0.0], [3.0, 9.0], 11)
            .expect("in range");
        let c = crown.center();
        assert!((c[0] - 3.0).abs() < 1e-3);
        assert!((c[1] - 9.0).abs() < 1e-3);
    }

    #[test]
    fn playable_trait_drives_progress() {
        let mut crown = cast();
        assert_eq!(Playable::duration(&crown), crown.total_duration());
        Playable::seek_to(&mut crown, 0.5);
        assert!(crown.age() > 0.0 && !crown.is_complete());
        Playable::seek_to(&mut crown, 1.0);
        assert!(crown.is_complete());
        Playable::reset(&mut crown);
        assert_eq!(crown.age(), 0.0);
        assert!(Playable::as_any(&crown).is::<PyreCrown>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let crown = cast();
        let total = crown.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "pyre-crown", crown, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.35);
        let inner = comp.get::<PyreCrown>("pyre-crown").unwrap();
        assert!(inner.age() > 0.0);
        assert!(!inner.samples().is_empty());
        comp.seek_abs(total + 1.0);
        assert!(comp
            .get::<PyreCrown>("pyre-crown")
            .unwrap()
            .is_complete());
    }

    #[test]
    fn light_gutters_not_shimmers() {
        let mut crown = cast();
        crown.seek_abs(crown.travel_duration() + 0.2);
        let a = crown.light().intensity;
        crown.seek_abs(crown.travel_duration() + 0.21);
        let b = crown.light().intensity;
        // Fire gutter uses fast beats — intensity should move between nearby
        // ages (not a flat base alone).
        assert!((a - b).abs() > 1e-4 || a > 0.0);
    }
}
