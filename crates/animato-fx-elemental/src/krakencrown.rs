//! The Kraken Crown cast pipeline: zone aim, wet-surge travel, rift + tentacle
//! hammering and light bookkeeping (Ext).
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `KrakenAbility.js` — LinearAbilityExtThreeJS's
//! **alive** far cast: a circle planted at a floor point answered with a ring
//! of cephalopod arms that haul themselves out of a rift and hammer the
//! middle. Everything is a pure function of `(seed, params, time)`:
//! [`KrakenCrown::seek_abs`] deterministically re-simulates from spawn, so the
//! pipeline can be driven by [`animato_composition::Composition`] — including
//! while paused, with params edited live (tentacle records store dice only;
//! metres resolve at sample time scaled by live
//! [`KrakenCrownParams::zone_radius`]).
//!
//! Renderer-owned systems from the original (GPU particles, ground decals,
//! camera shake, screen flash, kraken / abyss-field / brine-veil materials)
//! are intentionally *not* reimplemented here: the pipeline exposes every
//! stimulus they need ([`KrakenCrown::samples`], [`KrakenCrown::field`],
//! [`KrakenCrown::veil`], [`KrakenCrown::drain_events`], [`KrakenCrown::light`],
//! [`KrakenCrown::center`], [`KrakenCrown::open`]).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;

use crate::aim::{ZoneAimSolution, solve_zone_aim, solve_zone_aim_at};
use crate::kraken::{
    AbyssFieldSample, BrineVeilSample, KrakenRecord, KrakenSample, roll_kraken,
    sample_abyss_field, sample_brine_veil, sample_kraken, tick_arm_events,
};
use crate::math::{in_quad, out_cubic, out_quad, saturate};
use crate::params::KrakenCrownParams;
use crate::pipeline::{Phase, SEEK_STEP, SpawnError};

/// Phase-transition and hammer events for renderer-owned systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KrakenEvent {
    /// Front arrived; rift tears open and arms begin to rise.
    Impact,
    /// One or more arms just landed a smash this step.
    Smash,
    /// The synchronised finale just landed (once per cast).
    Finale,
    /// Withdrawal started.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast (rift swell shimmer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KrakenLight {
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

/// One complete Kraken Crown pipeline: zone aim + dice + phase machine.
#[derive(Clone, Debug)]
pub struct KrakenCrown {
    params: KrakenCrownParams,
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
    arms: Vec<KrakenRecord>,
    events: Vec<KrakenEvent>,
    finale_fired: bool,
}

impl KrakenCrown {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(KrakenCrownParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: KrakenCrownParams) -> Self {
        Self {
            params,
            seed: 0.0,
            origin: [0.0, 0.0],
            direction: [0.0, 1.0],
            side: [1.0, 0.0],
            center: [0.0, 0.0],
            length: 1.0,
            zone_radius: 4.6,
            entry_angle: 0.0,
            front: 0.0,
            u: 0.0,
            age: 0.0,
            impact_time: 0.0,
            fade_time: 0.0,
            open_time: 0.0,
            light_boost: 0.0,
            phase: Phase::Idle,
            arms: Vec::new(),
            events: Vec::new(),
            finale_fired: false,
        }
    }

    /// Live params (edit-while-paused reshapes the standing crown).
    pub fn params(&self) -> &KrakenCrownParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut KrakenCrownParams {
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

    /// Metres the wet surge has travelled.
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

    /// Seconds since the rift started tearing (impact + fade).
    pub fn open_time(&self) -> f32 {
        self.open_time
    }

    /// Bearing the surge arrives on (near side of the ring), radians.
    pub fn entry_angle(&self) -> f32 {
        self.entry_angle
    }

    /// Rift tear-open amount `0..1` (0 while travelling).
    pub fn open(&self) -> f32 {
        if self.phase == Phase::Travel || self.phase == Phase::Idle {
            0.0
        } else {
            out_cubic(saturate(
                self.open_time / self.params.open_duration(),
            ))
        }
    }

    /// How far the rift has closed back over the arms, `0..1`.
    pub fn close(&self) -> f32 {
        if self.phase != Phase::Fade {
            return 0.0;
        }
        let hold = self.params.withdraw_delay * 0.5;
        saturate(
            (self.fade_time - hold) / (self.params.fade_duration() - hold).max(0.05),
        )
    }

    /// Crown fade `1 → 0` through withdrawal.
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

    /// Tentacle dice for the current cast.
    pub fn arms(&self) -> &[KrakenRecord] {
        &self.arms
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
        self.entry_angle = libm::atan2f(-self.direction[1], -self.direction[0]);

        self.arms = roll_kraken(&self.params, seed);

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.4;
        self.phase = Phase::Travel;
        self.events.clear();
        self.finale_fired = false;
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

    /// Travelling surge tip on the floor plane.
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

    /// Seconds the surge needs to reach the centre at current speed.
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
        self.light_boost = self.params.light_intensity * 1.2;
        self.events.push(KrakenEvent::Impact);
    }

    fn tick_arms(&mut self) {
        if matches!(self.phase, Phase::Travel | Phase::Idle | Phase::Done | Phase::Charge) {
            return;
        }
        let impact_dur = self.params.impact_duration();
        let (smash, finale, _breach) = tick_arm_events(
            &mut self.arms,
            &self.params,
            self.open_time,
            self.entry_angle,
            impact_dur,
        );
        if smash > 0 {
            self.events.push(KrakenEvent::Smash);
            let power = if finale {
                self.params.finale_power
            } else {
                1.0
            };
            self.light_boost = self
                .light_boost
                .max(self.params.light_intensity * 0.5 * power);
        }
        if finale && !self.finale_fired {
            self.finale_fired = true;
            self.events.push(KrakenEvent::Finale);
        }
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
                self.tick_arms();
                let t = saturate(self.impact_time / self.params.impact_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Fade;
                    self.fade_time = 0.0;
                    self.events.push(KrakenEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                self.open_time += dt;
                self.tick_arms();
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(KrakenEvent::Done);
                }
            }
            Phase::Idle | Phase::Charge | Phase::Done => {}
        }
    }

    fn update_light(&mut self, dt: f32) {
        self.light_boost =
            (self.light_boost - self.light_boost * 4.5 * dt - 0.5 * dt).max(0.0);
    }

    /// Rift swell shimmer (`KrakenAbility#lightShimmer`).
    fn shimmer(&self) -> f32 {
        let t = self.age;
        0.82 + 0.18 * libm::sinf(t * 1.7) * libm::sinf(t * 0.9)
            + 0.04 * libm::sinf(t * 11.3)
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
                (1.0 - t) * 0.45
            }
            Phase::Idle | Phase::Charge | Phase::Done => 0.0,
        }
    }

    /// Current dynamic-light state.
    pub fn light(&self) -> KrakenLight {
        let (x, y, z) = if self.phase == Phase::Travel {
            let tip = self.front_position();
            (tip[0], 0.3, tip[1])
        } else {
            (
                self.center[0],
                self.params.light_height,
                self.center[1],
            )
        };
        KrakenLight {
            x,
            y,
            z,
            intensity: self.params.light_intensity * self.light_scale() * self.shimmer()
                + self.light_boost,
            radius: self.params.light_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Drain phase / smash events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<KrakenEvent> {
        core::mem::take(&mut self.events)
    }

    /// How many arms have emerged so far.
    pub fn emerged_count(&self) -> usize {
        self.samples().len()
    }

    /// Seek to an absolute cast time in seconds.
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle && self.arms.is_empty() {
            return;
        }
        if self.arms.is_empty() {
            return;
        }
        let target = time.max(0.0);
        for r in &mut self.arms {
            r.last_strike = -1.0;
            r.breached = false;
        }
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.4;
        self.phase = Phase::Travel;
        self.events.clear();
        self.finale_fired = false;
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    /// Resolve every emerged tentacle against live params.
    pub fn samples(&self) -> Vec<KrakenSample> {
        if matches!(self.phase, Phase::Idle | Phase::Done | Phase::Travel) {
            return Vec::new();
        }
        sample_kraken(
            &self.arms,
            &self.params,
            self.center,
            self.open_time,
            self.age,
            self.fade_time,
            self.phase == Phase::Fade,
            self.entry_angle,
            self.params.impact_duration(),
        )
    }

    /// Abyss rift sample (hidden while travelling).
    pub fn field(&self) -> Option<AbyssFieldSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel | Phase::Done) {
            return None;
        }
        let f = sample_abyss_field(
            &self.params,
            self.center,
            self.open(),
            self.close(),
            self.crown_fade(),
            self.seed,
        );
        if f.fade < 0.002 || f.open < 0.002 {
            None
        } else {
            Some(f)
        }
    }

    /// Brine-veil sample (hidden while travelling / fully closed).
    pub fn veil(&self) -> Option<BrineVeilSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel | Phase::Done) {
            return None;
        }
        sample_brine_veil(
            &self.params,
            self.center,
            self.open(),
            self.close(),
            self.crown_fade(),
            self.age,
            self.seed,
        )
    }

    /// Advance by `dt` seconds. Returns `true` while still active.
    pub fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }

    /// Return to the pool.
    pub fn destroy(&mut self) {
        self.arms.clear();
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.open_time = 0.0;
        self.light_boost = 0.0;
        self.phase = Phase::Idle;
        self.events.clear();
        self.finale_fired = false;
    }
}

impl Default for KrakenCrown {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for KrakenCrown {
    fn update(&mut self, dt: f32) -> bool {
        self.update(dt)
    }
}

impl Playable for KrakenCrown {
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

    fn cast() -> KrakenCrown {
        let mut crown = KrakenCrown::new();
        crown.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        crown
    }

    #[test]
    fn too_close_is_refused_when_min_range_set() {
        let mut crown = KrakenCrown::with_params(KrakenCrownParams {
            min_range: 3.0,
            ..KrakenCrownParams::default()
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
        let mut crown = KrakenCrown::new();
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
        assert!(total > 1.0 && total < 60.0);
        let mut guard = 0;
        while crown.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 90, "pipeline did not finish");
        }
        assert_eq!(crown.phase(), Phase::Done);
        assert!(crown.is_complete());
        let events = crown.drain_events();
        assert!(events.contains(&KrakenEvent::Impact));
        assert!(events.contains(&KrakenEvent::Fade));
        assert!(events.contains(&KrakenEvent::Done));
    }

    #[test]
    fn seek_is_deterministic_and_terminal() {
        let mut a = cast();
        let mut b = cast();
        a.seek_abs(1.2);
        b.seek_abs(1.2);
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
        assert_eq!(live.emerged_count(), sought.emerged_count());
    }

    #[test]
    fn params_edit_reshapes_standing_crown_while_paused() {
        let mut crown = cast();
        crown.seek_abs(crown.travel_duration() + 1.0);
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
        crown.seek_abs(crown.travel_duration() + 0.8);
        let a = crown.samples();
        let b = crown.samples();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert!(crown.field().is_some());
        assert!(crown.veil().is_some());
    }

    #[test]
    fn cast_at_plants_centre_on_aim_point() {
        let mut crown = KrakenCrown::new();
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
        assert!(Playable::as_any(&crown).is::<KrakenCrown>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let crown = cast();
        let total = crown.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "kraken-crown", crown, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.35);
        let inner = comp.get::<KrakenCrown>("kraken-crown").unwrap();
        assert!(inner.age() > 0.0);
        assert!(!inner.samples().is_empty());
        comp.seek_abs(total + 1.0);
        assert!(comp
            .get::<KrakenCrown>("kraken-crown")
            .unwrap()
            .is_complete());
    }

    #[test]
    fn light_swells_not_gutters() {
        let mut crown = cast();
        crown.seek_abs(crown.travel_duration() + 0.2);
        let a = crown.light().intensity;
        crown.seek_abs(crown.travel_duration() + 0.35);
        let b = crown.light().intensity;
        assert!(a > 0.0 && b > 0.0);
        // Slow swell beats — nearby ages should still move a little.
        assert!((a - b).abs() > 1e-5 || a > 1.0);
    }

    #[test]
    fn hammering_emits_smash_and_finale() {
        let mut crown = cast();
        crown.seek_abs(crown.travel_duration() + crown.params.impact_duration() - 0.1);
        let events = crown.drain_events();
        assert!(events.contains(&KrakenEvent::Impact));
        assert!(
            events.contains(&KrakenEvent::Smash) || events.contains(&KrakenEvent::Finale),
            "expected smash/finale in hammering window: {events:?}"
        );
    }
}
