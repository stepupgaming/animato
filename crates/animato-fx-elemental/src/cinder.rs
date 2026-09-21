//! The Cinder Fall cast pipeline: phase machine, ballistic arc, impact debris
//! and molten fissures.
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `MeteorAbility.js` (hand → impact arc, charge heat,
//! chunk ballistics) and `GroundFissures.js` (dice-only crack network).
//! Everything is a pure function of `(seed, params, time)`:
//! [`CinderFall::seek_abs`] deterministically re-simulates from spawn, so the
//! pipeline can be driven by [`animato_composition::Composition`] — including
//! while paused, with params edited live (records store dice only; metres
//! resolve at sample time).
//!
//! Renderer-owned systems from the original (volumetric fire trail, GPU
//! particles, ground decals, burst shells, camera shake, screen flash, rock
//! materials) are intentionally *not* reimplemented here: the pipeline exposes
//! every stimulus they need ([`CinderFall::arc_at`], [`CinderFall::chunks`],
//! [`CinderFall::fissures`], [`CinderFall::drain_events`],
//! [`CinderFall::light`]).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;

use crate::aim::{AimSolution, solve_aim};
use crate::fissure::{
    ChunkRecord, ChunkSample, FissureArmRecord, FissureBranchRecord, FissureSample, arc_point,
    heading_at, roll_chunks, roll_fissures, sample_chunk, sample_fissures,
};
use crate::math::{in_quad, out_quad, saturate};
use crate::params::CinderFallParams;
use crate::pipeline::{Phase, SEEK_STEP, SpawnError};

/// Phase-transition events for renderer-owned systems (bursts, decals, shake,
/// flash, fissure spawn). Drained via [`CinderFall::drain_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CinderEvent {
    /// The rock reached the far end; fire impact FX at
    /// [`CinderFall::impact_position`].
    Impact,
    /// Blow-out / withdrawal started.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast, mirroring `Ability#_updateLight` with
/// Meteor's uneven fire gutter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CinderLight {
    /// World x of the light (tracks the rock, then the impact point).
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

/// Resolved rock pose at the current age/params.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RockSample {
    /// World x.
    pub x: f32,
    /// World y (height).
    pub y: f32,
    /// World z.
    pub z: f32,
    /// Radius, metres.
    pub radius: f32,
    /// Charge heat `0..1` (`pow(u, charge_curve)` while travelling).
    pub charge: f32,
    /// Tumble angle, radians.
    pub angle: f32,
    /// Whether the intact meteor is drawn (`false` after impact).
    pub visible: bool,
}

/// One complete Cinder Fall pipeline: dice + phase machine + ballistic arc.
///
/// `Playable + Send + 'static`, so it slots directly into
/// `animato_composition::Composition::add` / `animato_timeline::Timeline`.
#[derive(Clone, Debug)]
pub struct CinderFall {
    params: CinderFallParams,
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
    tumble_axis: [f32; 3],
    chunks: Vec<ChunkRecord>,
    fissure_arms: Vec<FissureArmRecord>,
    fissure_branches: Vec<FissureBranchRecord>,
    events: Vec<CinderEvent>,
}

impl CinderFall {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(CinderFallParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: CinderFallParams) -> Self {
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
            tumble_axis: [0.0, 1.0, 0.0],
            chunks: Vec::new(),
            fissure_arms: Vec::new(),
            fissure_branches: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Live params. Edits apply to the *standing* cast on the next sample —
    /// the edit-while-paused rule from the original.
    pub fn params(&self) -> &CinderFallParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut CinderFallParams {
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

    /// Metres the strike front has travelled along the floor line.
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

    /// Chunk dice for the current cast.
    pub fn chunk_records(&self) -> &[ChunkRecord] {
        let budget = self.params.chunk_budget().min(self.chunks.len());
        &self.chunks[..budget]
    }

    /// Fissure arm dice for the current cast.
    pub fn fissure_arm_records(&self) -> &[FissureArmRecord] {
        &self.fissure_arms
    }

    /// Begin a cast from a solved aim. Refuses aims nearer than `min_range`.
    ///
    /// Meteor is a **line** cast in `ELEMENT_META` (no `CastShape.ZONE`); aim
    /// still uses floor-plane [`solve_aim`] so the API stays consistent with
    /// Frost / Storm (`origin`, `direction`, raw distance).
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

        let budget = self.params.chunk_budget();
        let (tumble, chunks) = roll_chunks(budget, seed);
        self.tumble_axis = tumble;
        self.chunks = chunks;

        let (arms, branches) = roll_fissures(
            self.params.fissure_arm_budget(),
            self.params.fissure_wander,
            seed,
        );
        self.fissure_arms = arms;
        self.fissure_branches = branches;

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        // Muzzle punch — meteor ships muzzleSize=0; keep a modest launch boost.
        self.light_boost = self.params.light_intensity * 0.35;
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

    /// Hand origin in world space (3D), mirrors `MeteorAbility#_launchPoint`.
    pub fn hand_point(&self) -> [f32; 3] {
        let p = &self.params;
        [
            self.origin[0] + self.direction[0] * p.hand_forward + self.side[0] * p.hand_side,
            p.hand_height,
            self.origin[1] + self.direction[1] * p.hand_forward + self.side[1] * p.hand_side,
        ]
    }

    /// Point on the ballistic arc at `s` (`0` = hand, `1` = impact).
    pub fn arc_at(&self, s: f32) -> [f32; 3] {
        arc_point(
            s,
            self.origin,
            self.direction,
            self.side,
            self.length,
            &self.params,
        )
    }

    /// Unit heading along the arc at `s`.
    pub fn heading_at(&self, s: f32) -> [f32; 3] {
        heading_at(
            s,
            self.origin,
            self.direction,
            self.side,
            self.length,
            &self.params,
        )
    }

    /// How far along the arc the rock sits (`u` while travelling, else `1`).
    pub fn progress(&self) -> f32 {
        if self.phase == Phase::Travel {
            self.u
        } else if matches!(self.phase, Phase::Idle) {
            0.0
        } else {
            1.0
        }
    }

    /// Charge heat `0..1` (`pow(u, charge_curve)` while travelling).
    pub fn charge(&self) -> f32 {
        if self.phase != Phase::Travel {
            1.0
        } else {
            libm::powf(saturate(self.u), self.params.charge_curve.max(0.05))
        }
    }

    /// Seconds since the rock landed (`0` while still travelling).
    pub fn since_impact(&self) -> f32 {
        match self.phase {
            Phase::Impact => self.impact_time,
            Phase::Fade | Phase::Done => self.params.impact_duration() + self.fade_time,
            Phase::Travel | Phase::Idle | Phase::Charge => 0.0,
        }
    }

    /// Debris retract `0..1` through the fade (chunks sinking into the floor).
    pub fn chunk_retract(&self) -> f32 {
        match self.phase {
            Phase::Fade => {
                let linger = self.params.chunk_linger.max(0.0);
                let sink = self.params.chunk_sink.max(0.05);
                saturate((self.fade_time - linger) / sink)
            }
            Phase::Done => 1.0,
            _ => 0.0,
        }
    }

    /// Seconds the front needs to cross the current length at current speed.
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

    /// Total finite duration: travel + impact + fade.
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
                    self.light_boost = self.params.light_intensity * 1.8;
                    self.events.push(CinderEvent::Impact);
                }
            }
            Phase::Impact => {
                self.impact_time += dt;
                let t = saturate(self.impact_time / self.params.impact_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Fade;
                    self.fade_time = 0.0;
                    self.events.push(CinderEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(CinderEvent::Done);
                }
            }
            Phase::Idle | Phase::Charge | Phase::Done => {}
        }
    }

    fn update_light(&mut self, dt: f32) {
        self.light_boost = (self.light_boost - self.light_boost * 3.5 * dt - 0.4 * dt).max(0.0);
    }

    /// Fire gutters where ice glints — a fast, uneven guttering.
    /// Mirrors `MeteorAbility#lightShimmer`.
    fn shimmer(&self) -> f32 {
        let rate = self.params.light_flicker_speed.max(0.1);
        let wobble = libm::sinf(self.age * rate)
            * libm::sinf(self.age * rate * 0.37 + 1.7);
        1.0 - saturate(self.params.light_flicker) * (0.5 + 0.5 * wobble)
    }

    fn light_scale(&self) -> f32 {
        match self.phase {
            Phase::Travel => 0.55 + 0.45 * self.charge(),
            Phase::Impact => {
                let t = saturate(self.impact_time / self.params.impact_duration());
                1.0 - in_quad(t) * 0.4
            }
            Phase::Fade => {
                let t = saturate(self.fade_time / self.params.fade_duration());
                (1.0 - t) * 0.4
            }
            Phase::Idle | Phase::Charge | Phase::Done => 0.0,
        }
    }

    /// Current dynamic-light state (tracks the rock on the arc).
    pub fn light(&self) -> CinderLight {
        let tip = self.arc_at(self.progress());
        CinderLight {
            x: tip[0],
            y: tip[1],
            z: tip[2],
            intensity: self.params.light_intensity * self.light_scale() * self.shimmer()
                + self.light_boost,
            radius: self.params.light_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Drain phase-transition events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<CinderEvent> {
        core::mem::take(&mut self.events)
    }

    /// Seek to an absolute cast time in seconds.
    ///
    /// Deterministic: dynamic state resets to spawn (dice are kept) and the
    /// shared step body re-simulates at [`SEEK_STEP`]. Rock / chunk / fissure
    /// metres are not stored — they resolve at sample time from
    /// `(seed, params, time)`.
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle && self.chunks.is_empty() && self.fissure_arms.is_empty() {
            return;
        }
        if self.fissure_arms.is_empty() && self.chunks.is_empty() {
            return;
        }
        let target = time.max(0.0);
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = self.params.light_intensity * 0.35;
        self.phase = Phase::Travel;
        self.events.clear();
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    /// Resolve the intact meteor against live params.
    pub fn rock(&self) -> RockSample {
        let travelling = self.phase == Phase::Travel;
        let s = self.progress();
        let p = self.arc_at(s);
        RockSample {
            x: p[0],
            y: p[1],
            z: p[2],
            radius: self.params.rock_radius(),
            charge: self.charge(),
            angle: self.age * self.params.spin,
            visible: travelling && !matches!(self.phase, Phase::Idle),
        }
    }

    /// Resolve every debris chunk against live params.
    pub fn chunks(&self) -> Vec<ChunkSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel) {
            return Vec::new();
        }
        let since = self.since_impact();
        let retract = self.chunk_retract();
        let budget = self.params.chunk_budget();
        self.chunks
            .iter()
            .take(budget)
            .map(|r| {
                sample_chunk(
                    r,
                    &self.params,
                    self.origin,
                    self.direction,
                    self.side,
                    self.length,
                    since,
                    retract,
                )
            })
            .collect()
    }

    /// Resolve the fissure network against live params.
    pub fn fissures(&self) -> Vec<FissureSample> {
        if matches!(self.phase, Phase::Idle | Phase::Travel) {
            return Vec::new();
        }
        let since = self.since_impact();
        // Fissure age tracks wall-clock since impact (independent of the
        // ability's shorter fade, so cracks can outlive the cast).
        let mut samples = sample_fissures(
            &self.fissure_arms,
            &self.fissure_branches,
            &self.params,
            since,
            since,
        );
        // GroundFissures parks the network at the impact point; translate
        // unit-disc metres into world so consumers see floor coords.
        let impact = self.impact_position();
        for s in &mut samples {
            for n in &mut s.nodes {
                n.x += impact[0];
                n.z += impact[1];
            }
        }
        samples
    }

    /// Tumble axis rolled at spawn.
    pub fn tumble_axis(&self) -> [f32; 3] {
        self.tumble_axis
    }

    /// Return to the pool.
    pub fn destroy(&mut self) {
        self.chunks.clear();
        self.fissure_arms.clear();
        self.fissure_branches.clear();
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

impl Default for CinderFall {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for CinderFall {
    fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }
}

impl Playable for CinderFall {
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

    fn cast() -> CinderFall {
        let mut c = CinderFall::new();
        // 12 m is past min_range (3) and under range (20).
        c.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        c
    }

    #[test]
    fn too_close_is_refused() {
        let mut c = CinderFall::new();
        let err = c.cast([0.0, 0.0], [0.0, 1.0], 1.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooClose {
                raw: 1.0,
                min: 3.0
            }
        );
        assert_eq!(c.phase(), Phase::Idle);
    }

    #[test]
    fn full_lifecycle_reaches_done() {
        let mut c = cast();
        let total = c.total_duration();
        assert!(total > 1.0 && total < 30.0);
        let mut guard = 0;
        while c.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 120, "pipeline did not finish");
        }
        assert_eq!(c.phase(), Phase::Done);
        assert!(c.is_complete());
        assert!(!c.is_active());
        let events = c.drain_events();
        assert!(events.contains(&CinderEvent::Impact));
        assert!(events.contains(&CinderEvent::Fade));
        assert!(events.contains(&CinderEvent::Done));
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
        assert_eq!(a.rock(), b.rock());
        assert_eq!(a.chunks(), b.chunks());
        assert_eq!(a.fissures(), b.fissures());

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
    fn fissure_sample_determinism() {
        let mut c = cast();
        c.seek_abs(c.travel_duration() + 0.4);
        assert_eq!(c.phase(), Phase::Impact);
        let a = c.fissures();
        let b = c.fissures();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert!(a.iter().any(|s| s.rank == 0.0));
        // Param edit while paused re-scales the standing network.
        let before = a[0].nodes.last().map(|n| (n.x, n.z)).unwrap();
        c.params_mut().fissure_radius = 8.0;
        let after = c.fissures()[0].nodes.last().map(|n| (n.x, n.z)).unwrap();
        assert_ne!(before, after);
    }

    #[test]
    fn params_edit_reshapes_arc_while_paused() {
        let mut c = cast();
        c.seek_abs(c.travel_duration() * 0.5);
        let before = c.rock();
        c.params_mut().arc = 5.0;
        let after = c.rock();
        assert_eq!(before.x, after.x); // floor projection unchanged at same u
        assert_ne!(before.y, after.y); // lob height moves
    }

    #[test]
    fn playable_trait_drives_progress() {
        use animato_core::Playable as _;
        let mut c = cast();
        assert_eq!(Playable::duration(&c), c.total_duration());
        Playable::seek_to(&mut c, 0.5);
        assert!(c.age() > 0.0 && !c.is_complete());
        Playable::seek_to(&mut c, 1.0);
        assert!(c.is_complete());
        Playable::reset(&mut c);
        assert_eq!(c.age(), 0.0);
        assert!(Playable::as_any(&c).is::<CinderFall>());
        assert!(Playable::as_any_mut(&mut c).is::<CinderFall>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let c = cast();
        let total = c.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "cinder-fall", c, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.15);
        let inner = comp.get::<CinderFall>("cinder-fall").unwrap();
        assert!(inner.age() > 0.0);
        assert!(inner.rock().visible || !inner.fissures().is_empty());
        comp.seek_abs(total + 1.0);
        assert!(comp.get::<CinderFall>("cinder-fall").unwrap().is_complete());
    }
}
