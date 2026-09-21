//! The Frost Lance cast pipeline: phase machine, fracture front, spike
//! eruption, withdrawal and light bookkeeping.
//!
//! Ports `Ability.js` (travel → impact → fade → done, front advance, dynamic
//! light) specialised with `IceAbility.js` (spike triggering, impact cluster,
//! field withdrawal). Everything is a pure function of `(seed, params, time)`:
//! [`FrostLance::seek_abs`] deterministically re-simulates from spawn, so the
//! pipeline can be driven by [`animato_composition::Composition`] or
//! [`animato_timeline::Timeline::seek_abs`](https://docs.rs/animato-timeline)
//! — including while paused, with params edited live.
//!
//! Renderer-owned systems from the original (GPU particles, ground decals,
//! burst shells, camera shake, screen flash) are intentionally *not*
//! reimplemented here: the pipeline exposes every stimulus they need
//! ([`FrostLance::front_position`], [`FrostLance::erupted_count`],
//! [`FrostLance::drain_events`], [`FrostLance::light`]) and they stay in the
//! wgpu feature / downstream renderer (see `gpu.rs` and the README mapping).

extern crate alloc;

use alloc::vec::Vec;
use animato_core::{Playable, Update};
use core::any::Any;

use crate::aim::{AimSolution, solve_aim};
use crate::math::{in_quad, out_quad, saturate};
use crate::params::FrostLanceParams;
use crate::spike::{SpikeRecord, SpikeSample, emergence, roll_spikes, sample_spike};

/// Fixed step used by [`FrostLance::seek_abs`] re-simulation. Fine enough that
/// trigger times quantize below a visible frame; coarse enough that a full
/// seek stays under a millisecond.
pub const SEEK_STEP: f32 = 1.0 / 480.0;

/// Phase machine from `Ability.js`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// No cast spawned yet, or returned to the pool.
    #[default]
    Idle,
    /// Nova Beam wind-up: charge orb building in the hands (front held).
    Charge,
    /// The fracture front is racing down the line.
    Travel,
    /// The front has arrived; the field stands.
    Impact,
    /// The field is withdrawing into the floor.
    Fade,
    /// Finished; ready for `destroy` (pooling) or a new `spawn`.
    Done,
}

/// Why a cast refused to start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnError {
    /// Aimed nearer than `min_range` (the red-arrow state upstream).
    TooClose {
        /// Requested distance in metres.
        raw: f32,
        /// Minimum allowed distance in metres.
        min: f32,
    },
    /// Aimed farther than `range` (zone casts refuse rather than clamp).
    TooFar {
        /// Requested distance in metres.
        raw: f32,
        /// Maximum allowed distance in metres.
        max: f32,
    },
}

impl core::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            SpawnError::TooClose { raw, min } => {
                write!(f, "cast refused: {raw:.2} m < min_range {min:.2} m")
            }
            SpawnError::TooFar { raw, max } => {
                write!(f, "cast refused: {raw:.2} m > range {max:.2} m")
            }
        }
    }
}

/// Phase-transition events for renderer-owned systems (bursts, decals, shake,
/// flash). Drained via [`FrostLance::drain_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrostEvent {
    /// The fracture front reached the far end; fire impact FX at
    /// [`FrostLance::impact_position`].
    Impact,
    /// Withdrawal started; stop ambient emission.
    Fade,
    /// Pipeline complete.
    Done,
}

/// Dynamic-light state for the cast, mirroring `Ability#_updateLight`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrostLight {
    /// World x of the light (tracks the front, then the impact point).
    pub x: f32,
    /// World z of the light.
    pub z: f32,
    /// Effective intensity (base × phase scale × shimmer + boost).
    pub intensity: f32,
    /// Effective radius in metres.
    pub radius: f32,
}

/// One complete Frost Lance pipeline: dice + phase machine + front.
///
/// `Playable + Send + 'static`, so it slots directly into
/// `animato_composition::Composition::add` / `animato_timeline::Timeline`.
#[derive(Clone, Debug)]
pub struct FrostLance {
    params: FrostLanceParams,
    seed: u64,
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
    spikes: Vec<SpikeRecord>,
    active_count: usize,
    impact_start: usize,
    events: Vec<FrostEvent>,
}

impl FrostLance {
    /// Create an unspawned pipeline with default params.
    pub fn new() -> Self {
        Self::with_params(FrostLanceParams::default())
    }

    /// Create an unspawned pipeline with explicit params.
    pub fn with_params(params: FrostLanceParams) -> Self {
        Self {
            params,
            seed: 0,
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
            spikes: Vec::new(),
            active_count: 0,
            impact_start: 0,
            events: Vec::new(),
        }
    }

    /// Live params. Edits apply to the *standing* cast on the next sample —
    /// the edit-while-paused rule from the original.
    pub fn params(&self) -> &FrostLanceParams {
        &self.params
    }

    /// Mutable live params.
    pub fn params_mut(&mut self) -> &mut FrostLanceParams {
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

    /// Metres the fracture front has travelled.
    pub fn front(&self) -> f32 {
        self.front
    }

    /// Front as a fraction of the cast length.
    pub fn u(&self) -> f32 {
        self.u
    }

    /// Cast age in seconds (the pipeline clock; driven by `update`/`seek_abs`).
    pub fn age(&self) -> f32 {
        self.age
    }

    /// Cast distance in metres.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// Dice seed for the current cast.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Spike dice for the current cast (fractions and jitters only).
    pub fn spikes(&self) -> &[SpikeRecord] {
        &self.spikes[..self.active_count.min(self.spikes.len())]
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
        self.seed = seed;
        self.origin = aim.origin;
        self.direction = aim.direction;
        // `side = direction × up` on the floor plane = (dz, -dx).
        self.side = [aim.direction[1], -aim.direction[0]];
        self.length = aim.distance.max(0.1);

        let budget = self.params.spike_budget();
        let impact = self.params.impact_count(budget);
        self.spikes = roll_spikes(&self.params, budget, impact, seed);
        self.active_count = budget;
        self.impact_start = budget.saturating_sub(impact);

        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = 0.0;
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

    /// A point on the cast line; `s` is `0..1` along it.
    pub fn point_at(&self, s: f32) -> [f32; 2] {
        [
            self.origin[0] + self.direction[0] * s * self.length,
            self.origin[1] + self.direction[1] * s * self.length,
        ]
    }

    /// World position of the travelling front (what the camera frames).
    pub fn front_position(&self) -> [f32; 2] {
        self.point_at(self.u)
    }

    /// Far-end impact point.
    pub fn impact_position(&self) -> [f32; 2] {
        self.point_at(1.0)
    }

    /// How many spikes have erupted so far.
    pub fn erupted_count(&self) -> usize {
        self.spikes().iter().filter(|r| r.erupted()).count()
    }

    /// How many spikes are currently breaching (`emergence > 0.25`) — the
    /// stimulus the original uses to fire per-spike chips and puffs.
    pub fn breaching_count(&self) -> usize {
        self.spikes()
            .iter()
            .filter(|r| emergence(r, &self.params, self.age) > 0.25)
            .count()
    }

    /// Whole-field withdrawal `0..1` (fade phase only, else `0`).
    pub fn retract(&self) -> f32 {
        if self.phase == Phase::Fade {
            saturate(
                (self.fade_time - self.params.shatter_delay)
                    / self.params.sink_time.max(0.05),
            )
        } else if self.phase == Phase::Done {
            1.0
        } else {
            0.0
        }
    }

    /// Seconds the front needs to cross the current length at current speed.
    ///
    /// The ease off a standstill has no closed form here, so this steps the
    /// same advance rule at [`SEEK_STEP`] — the single source of truth for
    /// travel timing, shared with `seek_abs`.
    pub fn travel_duration(&self) -> f32 {
        let speed = (self.params.speed * self.params.speed_scale).max(1e-6);
        let mut front = 0.0f32;
        let mut age = 0.0f32;
        loop {
            let ease_in = out_quad(saturate(age / 0.08));
            front += speed * ease_in * SEEK_STEP;
            age += SEEK_STEP;
            if front >= self.length || age > 3600.0 {
                break;
            }
        }
        age
    }

    /// Total finite duration: travel + impact + fade.
    pub fn total_duration(&self) -> f32 {
        self.travel_duration() + self.params.impact_duration() + self.params.fade_duration()
    }

    /// Advance the front. Returns `true` on the frame it reaches the end.
    /// Mirrors `Ability#advance` (constant m/s, eased off a standstill; the
    /// ease is keyed off elapsed age so the first step is never zero).
    fn advance(&mut self, dt: f32) -> bool {
        let speed = self.params.speed * self.params.speed_scale;
        let ease_in = out_quad(saturate(self.age / 0.08));
        self.front += speed * ease_in * dt;
        let previous = self.u;
        self.u = saturate(self.front / self.length);
        previous < 1.0 && self.u >= 1.0
    }

    /// Trigger every spike the front has reached. Mirrors
    /// `IceAbility#_triggerUpTo`.
    fn trigger_up_to(&mut self, limit: f32, include_impact: bool) {
        for i in 0..self.active_count {
            let erupt = {
                let r = &self.spikes[i];
                if r.erupt_time >= 0.0 {
                    false
                } else if r.impact && !include_impact {
                    false
                } else {
                    r.impact || r.along <= limit
                }
            };
            if erupt {
                let stagger = self.spikes[i].stagger;
                self.spikes[i].erupt_time = self.age + stagger * self.params.rise_stagger;
            }
        }
    }

    /// Shared step body for live `update` and `seek_abs` re-simulation.
    fn step(&mut self, dt: f32) {
        if !self.is_active() {
            return;
        }
        self.age += dt;
        match self.phase {
            Phase::Travel => {
                let reached = self.advance(dt);
                self.trigger_up_to(self.u, false);
                self.update_light(dt, 1.0);
                if reached {
                    self.phase = Phase::Impact;
                    self.impact_time = 0.0;
                    self.trigger_up_to(1.0, true);
                    // Impact punch for the dynamic light.
                    self.light_boost = self.params.light_intensity * 1.6;
                    self.events.push(FrostEvent::Impact);
                }
            }
            Phase::Impact => {
                self.impact_time += dt;
                let t = saturate(self.impact_time / self.params.impact_duration());
                self.update_light(dt, 1.0 - in_quad(t) * 0.45);
                if t >= 1.0 {
                    self.phase = Phase::Fade;
                    self.fade_time = 0.0;
                    self.events.push(FrostEvent::Fade);
                }
            }
            Phase::Fade => {
                self.fade_time += dt;
                let t = saturate(self.fade_time / self.params.fade_duration());
                self.update_light(dt, (1.0 - t) * 0.35);
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    self.events.push(FrostEvent::Done);
                }
            }
            Phase::Idle | Phase::Charge | Phase::Done => {}
        }
    }

    /// Mirrors `Ability#_updateLight` (without a scene: tracks the boost
    /// decay and reports intensity/radius through [`FrostLance::light`]).
    fn update_light(&mut self, dt: f32, _scale: f32) {
        self.light_boost = (self.light_boost - self.light_boost * 4.5 * dt - 0.5 * dt).max(0.0);
    }

    /// Slow shimmer rather than flicker: ice glints, it does not gutter.
    /// Mirrors `Ability#lightShimmer`.
    fn shimmer(&self) -> f32 {
        0.9 + 0.1 * libm::sinf(self.age * 9.3) * libm::sinf(self.age * 3.7)
    }

    /// Current dynamic-light state (position tracks the front, then holds at
    /// the impact point since `u` saturates at `1`).
    pub fn light(&self) -> FrostLight {
        let base_intensity = self.params.light_intensity;
        let base_radius = self.params.light_radius;
        let scale = match self.phase {
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
        };
        let p = self.front_position();
        FrostLight {
            x: p[0],
            z: p[1],
            intensity: base_intensity * scale * self.shimmer() + self.light_boost,
            radius: base_radius * (1.0 + self.light_boost * 0.02),
        }
    }

    /// Drain phase-transition events pushed since the last drain.
    pub fn drain_events(&mut self) -> Vec<FrostEvent> {
        core::mem::take(&mut self.events)
    }

    /// Seek to an absolute cast time in seconds — the hook that
    /// `Composition`/`Timeline::seek_abs` drives.
    ///
    /// Deterministic: dynamic state resets to spawn (dice are kept) and the
    /// shared [`FrostLance::step`] body re-simulates at [`SEEK_STEP`], so the
    /// same time always yields the same state, and param edits apply at
    /// sample time (edit-while-paused).
    ///
    /// ```rust
    /// use animato_fx_elemental::FrostLance;
    ///
    /// let mut a = FrostLance::new();
    /// a.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
    /// let mut b = FrostLance::new();
    /// b.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
    ///
    /// a.seek_abs(1.25);
    /// b.seek_abs(1.25);
    /// assert_eq!(a.front(), b.front());
    /// assert_eq!(a.erupted_count(), b.erupted_count());
    /// assert_eq!(a.samples(), b.samples());
    /// ```
    pub fn seek_abs(&mut self, time: f32) {
        if self.phase == Phase::Idle {
            return;
        }
        let target = time.max(0.0);
        for r in self.spikes.iter_mut() {
            r.erupt_time = -1.0;
        }
        self.front = 0.0;
        self.u = 0.0;
        self.age = 0.0;
        self.impact_time = 0.0;
        self.fade_time = 0.0;
        self.light_boost = 0.0;
        self.phase = Phase::Travel;
        self.events.clear();
        let mut remaining = target;
        while remaining > 0.0 && self.is_active() {
            let dt = remaining.min(SEEK_STEP);
            self.step(dt);
            remaining -= dt;
        }
    }

    /// Resolve every active spike against the live params at the current age.
    pub fn samples(&self) -> Vec<SpikeSample> {
        let retract = self.retract();
        self.spikes()
            .iter()
            .map(|r| {
                sample_spike(
                    r,
                    &self.params,
                    self.origin,
                    self.direction,
                    self.side,
                    self.length,
                    self.age,
                    retract,
                )
            })
            .collect()
    }

    /// Return to the pool. Mirrors `Ability#destroy` (state reset; dice kept
    /// allocated so the next cast allocates nothing).
    pub fn destroy(&mut self) {
        self.active_count = 0;
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

impl Default for FrostLance {
    fn default() -> Self {
        Self::new()
    }
}

impl Update for FrostLance {
    fn update(&mut self, dt: f32) -> bool {
        self.step(dt.max(0.0));
        self.is_active()
    }
}

impl Playable for FrostLance {
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
    use crate::spike::birth;

    fn cast() -> FrostLance {
        let mut lance = FrostLance::new();
        lance.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
        lance
    }

    #[test]
    fn too_close_is_refused() {
        let mut lance = FrostLance::new();
        let err = lance.cast([0.0, 0.0], [0.0, 1.0], 1.0, 0).unwrap_err();
        assert_eq!(
            err,
            SpawnError::TooClose {
                raw: 1.0,
                min: 2.5
            }
        );
        assert_eq!(lance.phase(), Phase::Idle);
    }

    #[test]
    fn front_travels_and_saturates() {
        let mut lance = cast();
        assert_eq!(lance.phase(), Phase::Travel);
        let mut last_u = 0.0;
        lance.update(0.0);
        while lance.phase() == Phase::Travel {
            assert!(lance.u() >= last_u);
            last_u = lance.u();
            assert!(lance.update(1.0 / 60.0));
        }
        assert_eq!(lance.u(), 1.0);
        assert_eq!(lance.phase(), Phase::Impact);
        assert_eq!(lance.drain_events(), alloc::vec![FrostEvent::Impact]);
    }

    #[test]
    fn full_lifecycle_reaches_done() {
        let mut lance = cast();
        let total = lance.total_duration();
        assert!(total > 1.0 && total < 30.0);
        let mut guard = 0;
        while lance.update(1.0 / 60.0) {
            guard += 1;
            assert!(guard < 60 * 60, "pipeline did not finish");
        }
        assert_eq!(lance.phase(), Phase::Done);
        assert!(lance.is_complete());
        assert!(!lance.is_active());
        assert_eq!(lance.retract(), 1.0);
        let events = lance.drain_events();
        assert!(events.contains(&FrostEvent::Impact));
        assert!(events.contains(&FrostEvent::Fade));
        assert!(events.contains(&FrostEvent::Done));
    }

    #[test]
    fn seek_is_deterministic_and_terminal() {
        let mut a = cast();
        let mut b = cast();
        a.seek_abs(1.25);
        b.seek_abs(1.25);
        assert_eq!(a.front(), b.front());
        assert_eq!(a.u(), b.u());
        assert_eq!(a.erupted_count(), b.erupted_count());
        assert_eq!(a.samples(), b.samples());

        a.seek_abs(0.0);
        assert_eq!(a.phase(), Phase::Travel);
        assert_eq!(a.front(), 0.0);
        assert_eq!(a.erupted_count(), 0);

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
        assert_eq!(live.erupted_count(), sought.erupted_count());
        assert_eq!(live.phase(), sought.phase());
    }

    #[test]
    fn params_edit_reshapes_standing_field_while_paused() {
        let mut lance = cast();
        lance.seek_abs(2.0);
        let before: Vec<f32> = lance.samples().iter().map(|s| s.height).collect();
        lance.params_mut().height = 9.0;
        let after: Vec<f32> = lance.samples().iter().map(|s| s.height).collect();
        assert_eq!(before.len(), after.len());
        assert!(after.iter().zip(before.iter()).any(|(a, b)| a > b));
        // Positions re-resolve too.
        lance.params_mut().width = 6.0;
        let wide: Vec<(f32, f32)> = lance.samples().iter().map(|s| (s.x, s.z)).collect();
        assert_ne!(wide.len(), 0);
    }

    #[test]
    fn samples_stay_sane_through_retract() {
        let mut lance = cast();
        lance.seek_abs(lance.total_duration() - 0.05);
        assert_eq!(lance.phase(), Phase::Fade);
        for s in lance.samples() {
            assert!(s.height >= 0.02);
            assert!(s.radius >= 0.01);
            assert!((0.0..=1.0).contains(&s.birth));
            assert!(s.y_base.is_finite());
        }
        // Birth flash is spent long before the fade.
        assert!(lance.samples().iter().all(|s| s.birth == 0.0));
    }

    #[test]
    fn light_tracks_front_then_punches_at_impact() {
        let mut lance = cast();
        lance.seek_abs(0.2);
        let travel = lance.light();
        assert!(travel.intensity > 0.0);
        let fp = lance.front_position();
        assert!((travel.x - fp[0]).abs() < 1e-5);
        // Impact punch raises the boost above the travel shimmer band.
        lance.seek_abs(lance.travel_duration() + 0.05);
        assert_eq!(lance.phase(), Phase::Impact);
        let punch = lance.light();
        assert!(punch.intensity > travel.intensity);
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
        assert!(Playable::as_any(&lance).is::<FrostLance>());
        assert!(Playable::as_any_mut(&mut lance).is::<FrostLance>());
    }

    #[test]
    fn composition_can_own_and_seek_the_pipeline() {
        let lance = cast();
        let total = lance.total_duration();
        let mut comp = animato_composition::Composition::new();
        comp.add("fx", "frost-lance", lance, 0.0);
        assert!((comp.duration() - total).abs() < 1e-4);
        comp.seek_abs(total * 0.25);
        let inner = comp.get::<FrostLance>("frost-lance").unwrap();
        assert!(inner.age() > 0.0);
        assert!(inner.erupted_count() > 0);
        comp.seek_abs(total + 1.0);
        assert!(comp.get::<FrostLance>("frost-lance").unwrap().is_complete());
    }

    #[test]
    fn birth_helper_bounds() {
        let r = SpikeRecord {
            along: 0.3,
            lateral: 0.0,
            scatter: 0.0,
            angle: 0.0,
            radial: 0.0,
            impact: false,
            rubble: false,
            height_jitter: 0.0,
            radius_jitter: 0.0,
            lean_jitter: 0.0,
            yaw: 0.0,
            stagger: 0.0,
            erupt_time: 2.0,
        };
        let p = FrostLanceParams::default();
        assert_eq!(birth(&r, &p, 2.0), 1.0);
        assert_eq!(birth(&r, &p, 2.0 + p.birth_fade), 0.0);
    }
}
