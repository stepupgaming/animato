//! Procedural parameters for the Frost Lance line-cast.
//!
//! Mirrors the `ice` block of `src/config/settings.js` in the original
//! sandbox (defaults copied verbatim, `snake_case`d). Only the dimensions the
//! CPU-side pipeline resolves are carried here — shader-only material tuning
//! (facet shading, glints, gradients) stays in GLSL and is documented in the
//! crate README mapping table.
//!
//! The governing rule from the original applies unchanged: spike records store
//! **only what the dice decided** (fractions and unitless jitters). Every
//! metre, radian and second is resolved against these params at sample time,
//! so editing params re-shapes a standing field — including while paused.

/// Hard ceiling on crystals per cast (matches `MAX_SPIKES` in `IceAbility.js`).
pub const MAX_SPIKES: usize = 288;

/// Fraction of the budget held back for the terminal impact cluster.
pub const IMPACT_FRACTION: f32 = 0.22;

/// Procedural parameters for one Frost Lance cast.
///
/// Field defaults reproduce the shipped `settings.ice` look.
#[derive(Clone, Debug, PartialEq)]
pub struct FrostLanceParams {
    // ── the cast itself ──
    /// Maximum cast distance, metres.
    pub range: f32,
    /// Casts nearer than this are refused (mirrors the red arrow state).
    pub min_range: f32,
    /// Fracture-front speed, metres/second.
    pub speed: f32,
    /// Seconds the field stands before it withdraws.
    pub lifetime: f32,
    /// Seconds before the ability can be armed again.
    pub cooldown: f32,
    /// Global speed multiplier (mirrors `settings.global.speed`; `1.0` = off).
    pub speed_scale: f32,
    /// Global randomness multiplier (mirrors `settings.global.randomness`).
    pub randomness: f32,

    // ── the footprint the spikes fill ──
    /// Half-width of the band at the caster, metres.
    pub width_near: f32,
    /// Half-width at the far end, metres.
    pub width: f32,
    /// `<1` flares early, `>1` stays narrow then opens out.
    pub width_curve: f32,
    /// Instances spent on one cast (capped at [`MAX_SPIKES`]).
    pub spike_count: f32,
    /// Multiplier on that count.
    pub density: f32,
    /// `>1` pulls spikes toward the centre line.
    pub clumping: f32,
    /// Extra lateral jitter, fraction of the local half-width.
    pub scatter: f32,
    /// `<1` crowds spikes toward the impact point.
    pub front_bias: f32,

    // ── silhouette of the field ──
    /// Spike height at the caster, metres.
    pub height_near: f32,
    /// Spike height at the far end, metres.
    pub height: f32,
    /// How late the ramp climbs.
    pub height_curve: f32,
    /// Unitless height jitter gain.
    pub height_jitter: f32,
    /// How much shorter the flank blades are than the spine, `0..1`.
    pub crown: f32,
    /// Extra height multiplier at the impact point.
    pub peak: f32,
    /// How much of the line that swell covers, `0..1`.
    pub peak_width: f32,
    /// Fraction of spikes demoted to ankle-height shards.
    pub rubble: f32,
    /// Height multiplier for those shards.
    pub rubble_scale: f32,

    // ── an individual crystal ──
    /// Base radius, metres.
    pub radius: f32,
    /// Unitless radius jitter gain.
    pub radius_jitter: f32,
    /// Tip radius as a fraction of the base (geometry-baked upstream).
    pub taper: f32,
    /// Sides of the prism (geometry-baked upstream).
    pub facets: f32,
    /// Facet displacement (geometry-baked upstream).
    pub roughness: f32,
    /// Sideways base-to-tip curve (geometry-baked upstream).
    pub bend: f32,
    /// Radians the spikes lean away from the caster.
    pub lean: f32,
    /// Unitless lean jitter gain.
    pub lean_jitter: f32,
    /// Random yaw, `0..1` of a full turn.
    pub twist: f32,

    // ── the eruption ──
    /// Seconds from buried to full height.
    pub rise_time: f32,
    /// How far past full height the punch carries.
    pub rise_overshoot: f32,
    /// Seconds of random delay between neighbours.
    pub rise_stagger: f32,
    /// Seconds the overshoot takes to damp out.
    pub settle: f32,
    /// Seconds after `lifetime` before withdrawal starts.
    pub shatter_delay: f32,
    /// Seconds to withdraw into the floor.
    pub sink_time: f32,
    /// Seconds the birth flash lasts.
    pub birth_fade: f32,

    // ── dynamic light (resolved by the pipeline, consumed by the renderer) ──
    /// Base intensity of the cast light.
    pub light_intensity: f32,
    /// Radius of the cast light, metres.
    pub light_radius: f32,
}

impl Default for FrostLanceParams {
    fn default() -> Self {
        Self {
            range: 15.0,
            min_range: 2.5,
            speed: 26.0,
            lifetime: 3.6,
            cooldown: 0.4,
            speed_scale: 1.0,
            randomness: 1.0,

            width_near: 0.55,
            width: 2.5,
            width_curve: 0.75,
            spike_count: 190.0,
            density: 1.0,
            clumping: 1.35,
            scatter: 0.55,
            front_bias: 0.85,

            height_near: 0.5,
            height: 3.1,
            height_curve: 1.7,
            height_jitter: 0.55,
            crown: 0.55,
            peak: 1.45,
            peak_width: 0.28,
            rubble: 0.42,
            rubble_scale: 0.3,

            radius: 0.41,
            radius_jitter: 0.93,
            taper: 0.69,
            facets: 7.0,
            roughness: 0.09,
            bend: 0.66,
            lean: 0.42,
            lean_jitter: 1.5,
            twist: 1.0,

            rise_time: 0.17,
            rise_overshoot: 0.26,
            rise_stagger: 0.09,
            settle: 0.55,
            shatter_delay: 0.6,
            sink_time: 1.0,
            birth_fade: 0.45,

            light_intensity: 9.0,
            light_radius: 13.0,
        }
    }
}

impl FrostLanceParams {
    /// How many spikes a cast spends: `clamp(round(spike_count * density), 1, 288)`.
    pub fn spike_budget(&self) -> usize {
        libm::roundf(self.spike_count * self.density).clamp(1.0, MAX_SPIKES as f32) as usize
    }

    /// How many of the budget are held back for the impact cluster.
    pub fn impact_count(&self, budget: usize) -> usize {
        libm::roundf((budget as f32) * IMPACT_FRACTION) as usize
    }

    /// Seconds the field stands once the front arrives (`max(0.2, lifetime)`).
    pub fn impact_duration(&self) -> f32 {
        self.lifetime.max(0.2)
    }

    /// Seconds the field takes to withdraw (`max(0.2, shatter_delay + sink_time)`).
    pub fn fade_duration(&self) -> f32 {
        (self.shatter_delay + self.sink_time).max(0.2)
    }
}

impl crate::aim::AimReach for FrostLanceParams {
    fn cast_range(&self) -> f32 {
        self.range
    }

    fn cast_min_range(&self) -> f32 {
        self.min_range
    }
}


/// Hard ceiling on filaments per bolt (matches `MAX_STRANDS` in `ThunderAbility.js`).
pub const MAX_STRANDS: usize = 24;

/// Samples along one filament polyline when resolving CPU-side strand samples.
///
/// Matches the ribbon tessellation ceiling in `ThunderAbility.js` (`NODES = 72`):
/// higher-frequency kinks than one per two nodes just alias.
pub const STRAND_NODES: usize = 72;

/// Procedural parameters for one Storm Lance cast.
///
/// Field defaults reproduce the shipped `settings.thunder` look. Only
/// CPU-resolved dimensions are carried — shader-only colour / particle /
/// decal tuning stays in GLSL and is documented in the crate README mapping.
#[derive(Clone, Debug, PartialEq)]
pub struct StormLanceParams {
    // ── the cast itself ──
    /// Maximum cast distance, metres.
    pub range: f32,
    /// Casts nearer than this are refused (mirrors the red arrow state).
    pub min_range: f32,
    /// Strike-front speed, metres/second.
    pub speed: f32,
    /// Seconds the bolt holds after it lands.
    pub lifetime: f32,
    /// Seconds it takes to blow out.
    pub fade_time: f32,
    /// Seconds before the ability can be armed again.
    pub cooldown: f32,
    /// Global speed multiplier (mirrors `settings.global.speed`; `1.0` = off).
    pub speed_scale: f32,
    /// Global randomness multiplier (mirrors `settings.global.randomness`).
    pub randomness: f32,

    // ── where the bolt leaves the caster ──
    /// Metres above the floor at the hand.
    pub hand_height: f32,
    /// Metres in front of the caster.
    pub hand_forward: f32,
    /// Metres to the side (+ follows the cast `side`).
    pub hand_side: f32,
    /// Height of the bolt where it lands, metres.
    pub end_height: f32,
    /// Metres the mid-span bows upward (negative droops).
    pub sag: f32,

    // ── the bundle of filaments ──
    /// Separate filaments (capped at [`MAX_STRANDS`]).
    pub strands: f32,
    /// Metres the bundle fans out at the far end.
    pub spread: f32,
    /// Metres the bundle fans out at the hand.
    pub spread_near: f32,
    /// `>1` keeps the bundle tight then opens it late.
    pub spread_curve: f32,
    /// Turns the bundle makes around the axis over its length.
    pub twist: f32,
    /// Turns/second it rolls on top of that.
    pub twist_speed: f32,
    /// How much dimmer an outer filament is than the spine, `0..1`.
    pub branch_dim: f32,

    // ── the shape of one filament ──
    /// Metres of kink at the coarsest octave.
    pub jitter: f32,
    /// Kinks per metre.
    pub jitter_scale: f32,
    /// Octave count, `1..5`.
    pub octaves: f32,
    /// Amplitude kept per octave.
    pub jitter_falloff: f32,
    /// How fast the kinks slide along the bolt.
    pub crawl: f32,
    /// Fraction of the span the ends are pulled straight over.
    pub pinch: f32,
    /// How hard the far end is pulled onto the target, `0..1`.
    pub converge: f32,

    // ── the ribbon (width hints for CPU samples / renderers) ──
    /// Half-width of a filament at the hand, metres.
    pub width: f32,
    /// That width at the impact point, as a fraction.
    pub width_tip: f32,
    /// How early the taper happens.
    pub width_curve: f32,
    /// Multiplier on the central spine.
    pub core_width: f32,

    // ── flicker & restrike ──
    /// Times/second the filaments re-roll their shape.
    pub restrike: f32,
    /// Depth of the whole-bolt brightness stutter.
    pub flicker: f32,
    /// Stutters/second.
    pub flicker_speed: f32,
    /// How much individual filaments blink out.
    pub strand_flash: f32,
    /// Extra heat on the leading edge while it travels.
    pub tip_glow: f32,
    /// Length of that leading edge, fraction of the span.
    pub tip_length: f32,

    // ── dynamic light ──
    /// Base intensity of the cast light.
    pub light_intensity: f32,
    /// Radius of the cast light, metres.
    pub light_radius: f32,
    /// Depth of the light's gutter, `0` = steady.
    pub light_flicker: f32,
    /// Light gutter steps/second.
    pub light_flicker_speed: f32,
}

impl Default for StormLanceParams {
    fn default() -> Self {
        Self {
            range: 24.0,
            min_range: 2.0,
            speed: 105.0,
            lifetime: 0.45,
            fade_time: 0.5,
            cooldown: 0.5,
            speed_scale: 1.0,
            randomness: 1.0,

            hand_height: 1.28,
            hand_forward: 0.55,
            hand_side: 0.16,
            end_height: 0.35,
            sag: 0.22,

            strands: 9.0,
            spread: 0.75,
            spread_near: 0.05,
            spread_curve: 1.6,
            twist: 0.45,
            twist_speed: 0.8,
            branch_dim: 0.72,

            jitter: 0.34,
            jitter_scale: 0.85,
            octaves: 4.0,
            jitter_falloff: 0.55,
            crawl: 3.2,
            pinch: 0.14,
            converge: 0.8,

            width: 0.025,
            width_tip: 0.43,
            width_curve: 1.09,
            core_width: 1.31,

            restrike: 24.0,
            flicker: 0.3,
            flicker_speed: 34.0,
            strand_flash: 0.5,
            tip_glow: 2.0,
            tip_length: 0.08,

            light_intensity: 26.0,
            light_radius: 17.0,
            light_flicker: 0.4,
            light_flicker_speed: 26.0,
        }
    }
}

impl StormLanceParams {
    /// How many filaments a cast spends: `clamp(round(strands), 1, 24)`.
    pub fn strand_budget(&self) -> usize {
        libm::roundf(self.strands).clamp(1.0, MAX_STRANDS as f32) as usize
    }

    /// Seconds the bolt holds once the front arrives (`max(0.05, lifetime)`).
    pub fn impact_duration(&self) -> f32 {
        self.lifetime.max(0.05)
    }

    /// Seconds the bolt takes to blow out (`max(0.05, fade_time)`).
    pub fn fade_duration(&self) -> f32 {
        self.fade_time.max(0.05)
    }

    /// Octave count clamped into the shader's fixed trip `1..5`.
    pub fn octave_count(&self) -> u32 {
        libm::roundf(self.octaves).clamp(1.0, 5.0) as u32
    }
}

impl crate::aim::AimReach for StormLanceParams {
    fn cast_range(&self) -> f32 {
        self.range
    }

    fn cast_min_range(&self) -> f32 {
        self.min_range
    }
}
/// Hard ceiling on debris chunks (matches `MAX_CHUNKS` in `MeteorAbility.js`).
pub const MAX_CHUNKS: usize = 28;

/// Hard ceiling on main fissure arms.
pub const MAX_FISSURE_ARMS: usize = 12;

/// Branches generated per fissure network (density slider culls them).
pub const MAX_FISSURE_BRANCHES: usize = 8;

/// Centreline resample step in unit space (matches `GroundFissures.js`).
pub const FISSURE_STEP: f32 = 0.045;

/// Procedural parameters for one Cinder Fall cast.
///
/// Field defaults reproduce the shipped `settings.meteor` look. Only
/// CPU-resolved dimensions are carried — shader-only rock shading, volumetric
/// trail, particle gradients and decal colours stay in GLSL and are documented
/// in the crate README mapping.
#[derive(Clone, Debug, PartialEq)]
pub struct CinderFallParams {
    // ── the cast itself ──
    /// Maximum cast distance, metres.
    pub range: f32,
    /// Casts nearer than this are refused (mirrors the red arrow state).
    pub min_range: f32,
    /// Rock travel speed along the floor line, metres/second.
    pub speed: f32,
    /// Seconds the crater burns after impact.
    pub lifetime: f32,
    /// Seconds everything takes to clear.
    pub fade_time: f32,
    /// Seconds before the ability can be armed again.
    pub cooldown: f32,
    /// Global speed multiplier (mirrors `settings.global.speed`; `1.0` = off).
    pub speed_scale: f32,
    /// Global randomness multiplier (mirrors `settings.global.randomness`).
    pub randomness: f32,

    // ── the flight path ──
    /// Metres above the floor at the hand.
    pub hand_height: f32,
    /// Metres in front of the caster.
    pub hand_forward: f32,
    /// Metres to the side (+ follows the cast `side`).
    pub hand_side: f32,
    /// Height of the rock where it lands, metres.
    pub end_height: f32,
    /// Metres the mid-span lobs upward.
    pub arc: f32,
    /// `<1` flattens the top of the arc, `>1` peaks it.
    pub arc_curve: f32,

    // ── the rock (CPU-relevant) ──
    /// Rock radius, metres.
    pub radius: f32,
    /// Tumble rate, radians/second.
    pub spin: f32,
    /// How late the rock heats up on its way in (`pow(u, charge_curve)`).
    pub charge_curve: f32,

    // ── impact debris ──
    /// Chunks thrown at impact (capped at [`MAX_CHUNKS`]).
    pub chunk_count: f32,
    /// Chunk radius as a fraction of the meteor's.
    pub chunk_scale: f32,
    /// Metres/second chunks leave the crater at.
    pub chunk_speed: f32,
    /// How far the spray is biased downrange.
    pub chunk_forward: f32,
    /// How steeply they are thrown.
    pub chunk_loft: f32,
    /// Gravity on chunk ballistics (negative).
    pub chunk_gravity: f32,
    /// Chunk tumble rate, radians/second.
    pub chunk_spin: f32,
    /// Seconds a chunk's seams take to go out.
    pub chunk_cool: f32,
    /// Seconds they lie there before sinking.
    pub chunk_linger: f32,
    /// Seconds to withdraw into the floor.
    pub chunk_sink: f32,

    // ── molten fissures ──
    /// How far the cracks reach, metres.
    pub fissure_radius: f32,
    /// Seconds before they close up.
    pub fissure_life: f32,
    /// Main cracks radiating from the impact.
    pub fissure_arms: f32,
    /// How hard an arm veers, radians per unit walked.
    pub fissure_wander: f32,
    /// Fraction of generated branches kept, `0..1`.
    pub fissure_branches: f32,
    /// How far along a branch runs before its point, `0..1`.
    pub fissure_branch_length: f32,
    /// Width of the open seam, metres.
    pub fissure_width: f32,
    /// Core temperature / heat gain.
    pub fissure_heat: f32,
    /// Speed of heat waves travelling along them.
    pub fissure_pulse: f32,
    /// How fast the cracks race outward, metres/second.
    pub fissure_growth: f32,
    /// Basalt heaved up along the lips, metres.
    pub fissure_rock_size: f32,

    // ── dynamic light ──
    /// Base intensity of the cast light.
    pub light_intensity: f32,
    /// Radius of the cast light, metres.
    pub light_radius: f32,
    /// Depth of the light's gutter, `0` = steady.
    pub light_flicker: f32,
    /// Light gutter steps/second.
    pub light_flicker_speed: f32,
}

impl Default for CinderFallParams {
    fn default() -> Self {
        Self {
            range: 20.0,
            min_range: 3.0,
            speed: 21.0,
            lifetime: 2.2,
            fade_time: 1.6,
            cooldown: 0.9,
            speed_scale: 1.0,
            randomness: 1.0,

            hand_height: 1.35,
            hand_forward: 0.6,
            hand_side: 0.2,
            end_height: 0.75,
            arc: 2.6,
            arc_curve: 0.85,

            radius: 0.8,
            spin: 3.4,
            charge_curve: 1.6,

            chunk_count: 18.0,
            chunk_scale: 0.28,
            chunk_speed: 7.5,
            chunk_forward: 0.55,
            chunk_loft: 1.0,
            chunk_gravity: -17.0,
            chunk_spin: 6.0,
            chunk_cool: 2.6,
            chunk_linger: 0.5,
            chunk_sink: 1.0,

            fissure_radius: 5.2,
            fissure_life: 6.5,
            fissure_arms: 6.0,
            fissure_wander: 1.6,
            fissure_branches: 0.75,
            fissure_branch_length: 0.85,
            fissure_width: 0.14,
            fissure_heat: 1.5,
            fissure_pulse: 1.0,
            fissure_growth: 9.0,
            fissure_rock_size: 0.3,

            light_intensity: 16.0,
            light_radius: 14.0,
            light_flicker: 0.25,
            light_flicker_speed: 13.0,
        }
    }
}

impl CinderFallParams {
    /// How many chunks a cast spends: `clamp(round(chunk_count), 0, 28)`.
    pub fn chunk_budget(&self) -> usize {
        libm::roundf(self.chunk_count).clamp(0.0, MAX_CHUNKS as f32) as usize
    }

    /// How many main fissure arms: `clamp(round(fissure_arms), 2, 12)`.
    pub fn fissure_arm_budget(&self) -> usize {
        libm::roundf(self.fissure_arms).clamp(2.0, MAX_FISSURE_ARMS as f32) as usize
    }

    /// Seconds the crater burns once the rock lands (`max(0.2, lifetime)`).
    pub fn impact_duration(&self) -> f32 {
        self.lifetime.max(0.2)
    }

    /// Seconds everything takes to clear (`max(0.2, fade_time)`).
    pub fn fade_duration(&self) -> f32 {
        self.fade_time.max(0.2)
    }

    /// Rock radius floored so samples never go degenerate.
    pub fn rock_radius(&self) -> f32 {
        self.radius.max(0.02)
    }
}

impl crate::aim::AimReach for CinderFallParams {
    fn cast_range(&self) -> f32 {
        self.range
    }

    fn cast_min_range(&self) -> f32 {
        self.min_range
    }
}


/// Hard ceiling on shock discs (matches `MAX_RINGS` in `BeamAbility.js`).
pub const MAX_RINGS: usize = 12;

/// Hard ceiling on coil ribbons (matches `MAX_COILS` in `BeamAbility.js`).
pub const MAX_COILS: usize = 8;

/// Centreline resample count for the parametric beam tube (CPU sample only).
pub const TUBE_SEGMENTS: usize = 32;

/// Procedural parameters for one Nova Beam cast.
///
/// Field defaults reproduce the shipped `settings.beam` look. Only CPU-resolved
/// dimensions are carried — shader-only tube/coil/orb shading, particle
/// gradients and decal colours stay in GLSL and are documented in the crate
/// README mapping.
#[derive(Clone, Debug, PartialEq)]
pub struct NovaBeamParams {
    // ── the cast itself ──
    /// Maximum cast distance, metres.
    pub range: f32,
    /// Casts nearer than this are refused (mirrors the red arrow state).
    pub min_range: f32,
    /// Seconds the charge orb winds up before the beam is released.
    pub charge: f32,
    /// Leading-edge speed once released, metres/second.
    pub speed: f32,
    /// Seconds the beam burns once it lands (Impact / sustain).
    pub lifetime: f32,
    /// Seconds it takes to collapse.
    pub fade_time: f32,
    /// Seconds before the ability can be armed again.
    pub cooldown: f32,
    /// Global speed multiplier (mirrors `settings.global.speed`; `1.0` = off).
    pub speed_scale: f32,
    /// Global randomness multiplier (mirrors `settings.global.randomness`).
    pub randomness: f32,

    // ── where it leaves the caster ──
    /// Metres above the floor at the hands.
    pub hand_height: f32,
    /// Metres in front of the caster.
    pub hand_forward: f32,
    /// Metres to the side (+ follows the cast `side`); beam sits on centreline.
    pub hand_side: f32,
    /// Height of the beam where it lands, metres.
    pub end_height: f32,

    // ── the column (CPU radius profile) ──
    /// Half-width at the muzzle, metres.
    pub radius_near: f32,
    /// Half-width at the target, metres.
    pub radius: f32,
    /// `<1` opens out early, `>1` stays tight then flares late.
    pub radius_curve: f32,
    /// Extra swell where it lands.
    pub flare: f32,
    /// How much of the span that swell covers, `0..1`.
    pub flare_width: f32,

    // ── shock discs (dice + sample) ──
    /// Discs in flight (capped at [`MAX_RINGS`]).
    pub rings: f32,
    /// Trips down the beam per second.
    pub ring_speed: f32,
    /// Inner lip, × the local column radius.
    pub ring_inner: f32,
    /// Outer lip, × the local column radius.
    pub ring_outer: f32,
    /// How much they open out as they travel.
    pub ring_swell: f32,
    /// How much is left of one by the time it lands.
    pub ring_fade: f32,

    // ── charge orb (CPU size) ──
    /// Orb radius once up to power, metres.
    pub orb_size: f32,
    /// How hard it pulses.
    pub orb_throb: f32,
    /// Orb throb cycles/second.
    pub orb_throb_speed: f32,

    // ── dynamic light ──
    /// Base intensity of the beam light.
    pub light_intensity: f32,
    /// Radius of the beam light, metres.
    pub light_radius: f32,
    /// Depth of the light's hum, `0` = steady.
    pub light_pulse: f32,
    /// Light hum pulses/second.
    pub light_pulse_speed: f32,
    /// Intensity of the muzzle light in the hands.
    pub muzzle_light_intensity: f32,
    /// Radius of the muzzle light, metres.
    pub muzzle_light_radius: f32,
}

impl Default for NovaBeamParams {
    fn default() -> Self {
        Self {
            range: 26.0,
            min_range: 3.0,
            charge: 0.42,
            speed: 150.0,
            lifetime: 1.15,
            fade_time: 0.4,
            cooldown: 1.6,
            speed_scale: 1.0,
            randomness: 1.0,

            hand_height: 1.3,
            hand_forward: 0.72,
            hand_side: 0.0,
            end_height: 1.0,

            radius_near: 0.16,
            radius: 0.77,
            radius_curve: 1.27,
            flare: 1.74,
            flare_width: 0.09,

            rings: 10.0,
            ring_speed: 1.31,
            ring_inner: 2.42,
            ring_outer: 2.73,
            ring_swell: 0.55,
            ring_fade: 0.18,

            orb_size: 0.39,
            orb_throb: 0.11,
            orb_throb_speed: 6.9,

            light_intensity: 30.0,
            light_radius: 20.0,
            light_pulse: 0.18,
            light_pulse_speed: 5.0,
            muzzle_light_intensity: 16.0,
            muzzle_light_radius: 9.0,
        }
    }
}

impl NovaBeamParams {
    /// How many shock discs a cast spends: `clamp(round(rings), 1, 12)`.
    pub fn ring_budget(&self) -> usize {
        libm::roundf(self.rings).clamp(1.0, MAX_RINGS as f32) as usize
    }

    /// Seconds the orb winds up (`max(0.01, charge)`).
    pub fn charge_duration(&self) -> f32 {
        self.charge.max(0.01)
    }

    /// Seconds the beam burns once the front arrives (`max(0.05, lifetime)`).
    pub fn impact_duration(&self) -> f32 {
        self.lifetime.max(0.05)
    }

    /// Seconds the beam takes to collapse (`max(0.05, fade_time)`).
    pub fn fade_duration(&self) -> f32 {
        self.fade_time.max(0.05)
    }
}

impl crate::aim::AimReach for NovaBeamParams {
    fn cast_range(&self) -> f32 {
        self.range
    }

    fn cast_min_range(&self) -> f32 {
        self.min_range
    }
}

/// Hard ceiling on leash filaments (matches `MAX_LEASH` in `SnareAbility.js`).
pub const MAX_LEASH: usize = 6;
/// Hard ceiling on column filaments (matches `MAX_COLUMN`).
pub const MAX_COLUMN: usize = 16;
/// Hard ceiling on tendril filaments (matches `MAX_TENDRIL`).
pub const MAX_TENDRIL: usize = 20;
/// Hard ceiling on rim-arc filaments (matches `MAX_RIM`).
pub const MAX_RIM: usize = 14;
/// Sample nodes along one cage filament.
pub const CAGE_NODES: usize = 48;

/// Procedural parameters for the Voltaic Snare far-cast.
///
/// Mirrors the `snare` block of `src/config/settings.js`. Only CPU-resolved
/// dimensions are carried — colours / particle rates / field-shader tuning
/// stay GLSL downstream. Metres that scale under `zone_radius` (throat,
/// column spread, tendril reach, rim jitter, …) are stored as **unit
/// fractions** and multiplied by the live footprint at sample time, so editing
/// `zone_radius` on a standing trap reshapes the cage with the clock stopped.
#[derive(Clone, Debug, PartialEq)]
pub struct VoltaicSnareParams {
    // ── the cast ──
    /// Maximum cast distance, metres.
    pub range: f32,
    /// Casts nearer than this are refused (`0` = plant underfoot is legal).
    pub min_range: f32,
    /// Footprint the circle indicator measures out, metres.
    pub zone_radius: f32,
    /// How fast the leash races to the point, metres/second.
    pub speed: f32,
    /// Seconds the ring takes to slam open once it lands.
    pub snap_time: f32,
    /// Seconds the snare stands (impact phase).
    pub lifetime: f32,
    /// Seconds it takes to collapse.
    pub fade_time: f32,
    /// Seconds before the ability can be armed again.
    pub cooldown: f32,
    /// Global speed multiplier (`settings.global.speed`).
    pub speed_scale: f32,
    /// Global randomness multiplier (`settings.global.randomness`).
    pub randomness: f32,

    // ── the leash that plants it ──
    /// Metres above the floor at the hand.
    pub hand_height: f32,
    /// Metres in front of the caster.
    pub hand_forward: f32,
    /// Metres to the side (+ follows the cast `side`).
    pub hand_side: f32,
    /// Filaments in the whip.
    pub leash_strands: f32,
    /// Metres the mid-span bows (negative drops it to the floor).
    pub leash_sag: f32,
    /// How far the filaments separate, metres.
    pub leash_spread: f32,
    /// Kink amplitude on the whip, metres.
    pub leash_kink: f32,
    /// × the shared filament width.
    pub leash_width: f32,
    /// How far above the floor the tip runs, metres.
    pub leash_cling: f32,

    // ── the column ──
    /// Filaments in the pillar.
    pub strands: f32,
    /// How high it reaches, metres.
    pub height: f32,
    /// `<1` gets it up fast, `>1` makes it climb late.
    pub height_curve: f32,
    /// Radius where it leaves the floor, × `zone_radius`.
    pub throat: f32,
    /// Radius at the top, × `zone_radius`.
    pub column_spread: f32,
    /// `>1` keeps the throat tight then opens it late.
    pub column_curve: f32,
    /// Extra opening over the last quarter, × `zone_radius`.
    pub column_flare: f32,
    /// Turns a filament makes over the climb.
    pub column_twist: f32,
    /// Turns/second the whole pillar rolls.
    pub column_spin: f32,
    /// Kink amplitude, metres.
    pub column_kink: f32,
    /// × the shared filament width.
    pub column_width: f32,
    /// How much thinner the top is than the base.
    pub column_taper: f32,

    // ── tendrils ──
    /// Separate ground filaments.
    pub tendrils: f32,
    /// Where they leave the column, × `zone_radius`.
    pub tendril_inner: f32,
    /// Where they end, × `zone_radius` (`1` = exactly on the band).
    pub tendril_reach: f32,
    /// `<1` throws them outward early.
    pub tendril_curve: f32,
    /// Radians a tendril veers over its run.
    pub tendril_wander: f32,
    /// Metres it hops off the floor mid-span.
    pub tendril_arch: f32,
    /// How far above the floor it runs, metres.
    pub tendril_hug: f32,
    /// Turns/second the whole fan rotates.
    pub tendril_spin: f32,
    /// Kink amplitude, metres.
    pub tendril_kink: f32,
    /// × the shared filament width.
    pub tendril_width: f32,
    /// How much dimmer than the column.
    pub tendril_dim: f32,

    // ── rim arcs ──
    /// Arcs on the boundary at once.
    pub rim_arcs: f32,
    /// Fraction of the circle one arc covers.
    pub rim_span: f32,
    /// Revolutions/second they travel.
    pub rim_speed: f32,
    /// Metres they hop at mid-span.
    pub rim_height: f32,
    /// Radial wobble, × `zone_radius`.
    pub rim_jitter: f32,
    /// Kink amplitude, metres.
    pub rim_kink: f32,
    /// × the shared filament width.
    pub rim_width: f32,
    /// How much dimmer than the column.
    pub rim_dim: f32,

    // ── shared filament shape ──
    /// Master multiplier on the four per-role kink amplitudes.
    pub jitter: f32,
    /// Kinks per metre.
    pub jitter_scale: f32,
    /// Octave count, `1..5`.
    pub octaves: f32,
    /// Amplitude kept per octave.
    pub jitter_falloff: f32,
    /// How fast the kinks slide along a filament.
    pub crawl: f32,
    /// Fraction of the span the ends are pulled straight over.
    pub pinch: f32,
    /// Times/second every filament re-rolls its shape.
    pub restrike: f32,
    /// Depth of the whole-cage brightness stutter.
    pub flicker: f32,
    /// Stutters/second.
    pub flicker_speed: f32,
    /// How much individual filaments blink out.
    pub strand_flash: f32,
    /// Half-width of a filament, metres.
    pub width: f32,

    // ── dynamic light ──
    /// Base intensity of the cast light.
    pub light_intensity: f32,
    /// Radius of the cast light, metres.
    pub light_radius: f32,
    /// How far up the column the light sits, `0..1`.
    pub light_height: f32,
    /// Depth of the light's gutter, `0` = steady.
    pub light_flicker: f32,
    /// Light gutter steps/second.
    pub light_flicker_speed: f32,
}

impl Default for VoltaicSnareParams {
    fn default() -> Self {
        Self {
            range: 20.0,
            min_range: 0.0,
            zone_radius: 4.4,
            speed: 62.0,
            snap_time: 0.16,
            lifetime: 2.6,
            fade_time: 0.75,
            cooldown: 1.4,
            speed_scale: 1.0,
            randomness: 1.0,

            hand_height: 1.24,
            hand_forward: 0.58,
            hand_side: 0.18,
            leash_strands: 3.0,
            leash_sag: -0.35,
            leash_spread: 0.22,
            leash_kink: 0.3,
            leash_width: 1.0,
            leash_cling: 0.12,

            strands: 15.0,
            height: 9.2,
            height_curve: 1.45,
            throat: 0.16,
            column_spread: 0.25,
            column_curve: 2.88,
            column_flare: 0.585,
            column_twist: 0.22,
            column_spin: 1.26,
            column_kink: 0.27,
            column_width: 1.86,
            column_taper: 1.09,

            tendrils: 20.0,
            tendril_inner: 0.0,
            tendril_reach: 1.07,
            tendril_curve: 1.18,
            tendril_wander: 1.41,
            tendril_arch: 1.16,
            tendril_hug: 0.005,
            tendril_spin: -0.225,
            tendril_kink: 0.72,
            tendril_width: 0.75,
            tendril_dim: 0.8,

            rim_arcs: 14.0,
            rim_span: 0.335,
            rim_speed: -1.84,
            rim_height: 0.98,
            rim_jitter: 0.23,
            rim_kink: 0.15,
            rim_width: 0.85,
            rim_dim: 1.0,

            jitter: 1.0,
            jitter_scale: 1.4,
            octaves: 4.0,
            jitter_falloff: 0.55,
            crawl: 2.4,
            pinch: 0.16,
            restrike: 21.0,
            flicker: 0.26,
            flicker_speed: 30.0,
            strand_flash: 0.45,
            width: 0.032,

            light_intensity: 24.0,
            light_radius: 18.0,
            light_height: 0.38,
            light_flicker: 0.38,
            light_flicker_speed: 24.0,
        }
    }
}

impl VoltaicSnareParams {
    /// Live footprint, metres (`max(0.05, zone_radius)`).
    pub fn radius(&self) -> f32 {
        self.zone_radius.max(0.05)
    }

    /// Leash filament budget: `clamp(round(leash_strands), 0, MAX_LEASH)`.
    pub fn leash_budget(&self) -> usize {
        libm::roundf(self.leash_strands).clamp(0.0, MAX_LEASH as f32) as usize
    }

    /// Column filament budget: `clamp(round(strands), 0, MAX_COLUMN)`.
    pub fn column_budget(&self) -> usize {
        libm::roundf(self.strands).clamp(0.0, MAX_COLUMN as f32) as usize
    }

    /// Tendril filament budget: `clamp(round(tendrils), 0, MAX_TENDRIL)`.
    pub fn tendril_budget(&self) -> usize {
        libm::roundf(self.tendrils).clamp(0.0, MAX_TENDRIL as f32) as usize
    }

    /// Rim-arc filament budget: `clamp(round(rim_arcs), 0, MAX_RIM)`.
    pub fn rim_budget(&self) -> usize {
        libm::roundf(self.rim_arcs).clamp(0.0, MAX_RIM as f32) as usize
    }

    /// Octave count clamped to `1..5`.
    pub fn octave_count(&self) -> u32 {
        libm::roundf(self.octaves).clamp(1.0, 5.0) as u32
    }

    /// Seconds the ring takes to slam open (`max(0.01, snap_time)`).
    pub fn snap_duration(&self) -> f32 {
        self.snap_time.max(0.01)
    }

    /// Seconds the snare stands (`max(0.05, lifetime)`).
    pub fn impact_duration(&self) -> f32 {
        self.lifetime.max(0.05)
    }

    /// Seconds the snare takes to collapse (`max(0.05, fade_time)`).
    pub fn fade_duration(&self) -> f32 {
        self.fade_time.max(0.05)
    }
}

impl crate::aim::AimReach for VoltaicSnareParams {
    fn cast_range(&self) -> f32 {
        self.range
    }

    fn cast_min_range(&self) -> f32 {
        self.min_range
    }
}

impl crate::aim::ZoneAimReach for VoltaicSnareParams {
    fn zone_radius(&self) -> f32 {
        self.zone_radius
    }
}


/// Hard ceiling on crown shards (matches `MAX_SPIKES` in `GlacierAbility.js`).
pub const MAX_CROWN_SPIKES: usize = 320;

/// Procedural parameters for the Glacial Crown far-cast.
///
/// Mirrors the `glacier` block of `src/config/settings.js`. Only CPU-resolved
/// dimensions are carried — colours / particle rates / material-shader tuning
/// stay GLSL downstream. Metres that scale under `zone_radius` (ring seat,
/// skirt band, veil radius, …) are stored as **unit fractions** and multiplied
/// by the live footprint at sample time, so editing `zone_radius` on a standing
/// crown reshapes it with the clock stopped.
#[derive(Clone, Debug, PartialEq)]
pub struct GlacialCrownParams {
    // ── the cast ──
    /// Maximum cast distance, metres.
    pub range: f32,
    /// Casts nearer than this are refused (`0` = plant underfoot is legal).
    pub min_range: f32,
    /// Footprint the circle indicator measures out, metres.
    pub zone_radius: f32,
    /// How fast the freeze front races to the point, metres/second.
    pub speed: f32,
    /// Seconds the sheet takes to freeze out to the boundary.
    pub snap_time: f32,
    /// Seconds the crown stands (impact phase).
    pub lifetime: f32,
    /// Seconds after `lifetime` before the ice starts to break.
    pub shatter_delay: f32,
    /// Seconds of random delay between neighbours during shatter.
    pub shatter_stagger: f32,
    /// Seconds one shard takes to crumble and withdraw.
    pub sink_time: f32,
    /// Seconds before the ability can be armed again.
    pub cooldown: f32,
    /// Global speed multiplier (`settings.global.speed`).
    pub speed_scale: f32,
    /// Global randomness multiplier (`settings.global.randomness`).
    pub randomness: f32,

    // ── hand origin ──
    /// Metres above the floor at the hand.
    pub hand_height: f32,
    /// Metres in front of the caster.
    pub hand_forward: f32,
    /// Metres to the side (+ follows the cast `side`).
    pub hand_side: f32,

    // ── footprint fill ──
    /// Instances spent on one cast (capped at [`MAX_CROWN_SPIKES`]).
    pub spike_count: f32,
    /// Multiplier on that count.
    pub density: f32,
    /// Fraction spent on the wall at the boundary.
    pub ring_share: f32,
    /// Fraction on the spire in the middle (`0` = middle stays open).
    pub core_share: f32,
    /// Fraction of skirt held back for the hold.
    pub late_share: f32,
    /// Where the wall stands, × `zone_radius`.
    pub ring_seat: f32,
    /// Radial jitter of the wall, × `zone_radius`.
    pub ring_scatter: f32,
    /// Inner lip of the wreckage bank, × `zone_radius`.
    pub skirt_seat: f32,
    /// How wide that band is, × `zone_radius`.
    pub skirt_band: f32,
    /// `<1` pushes the skirt outward, `>1` crowds it inward.
    pub skirt_bias: f32,
    /// Radius of the cluster in the middle, × `zone_radius`.
    pub core_spread: f32,

    // ── silhouette ──
    /// Length of a blade on the wall, metres.
    pub ring_height: f32,
    /// How uneven the crest of that wall is, `0..1`.
    pub ring_wave: f32,
    /// Length of a shard in the skirt, metres.
    pub skirt_height: f32,
    /// Length of the spire, metres.
    pub core_height: f32,
    /// Height jitter strength.
    pub height_jitter: f32,
    /// Radians the wall is thrown outward.
    pub ring_lean: f32,
    /// Radians the skirt leans.
    pub skirt_lean: f32,
    /// Radians the spire leans.
    pub core_lean: f32,
    /// Lean jitter strength.
    pub lean_jitter: f32,
    /// Radians a blade is splayed off its own radius, ±.
    pub fan: f32,
    /// Random yaw, `0..1` of a full turn.
    pub twist: f32,
    /// Fraction of the skirt demoted to ankle-height wreckage.
    pub rubble: f32,
    /// Height scale for rubble shards.
    pub rubble_scale: f32,

    // ── crystal ──
    /// Base radius, metres.
    pub crystal_radius: f32,
    /// Radius jitter strength.
    pub radius_jitter: f32,
    /// Tip radius as a fraction of the base.
    pub taper: f32,
    /// Sides of the prism.
    pub facets: f32,
    /// How far the facets are pushed off a clean prism.
    pub roughness: f32,
    /// Sideways curve from base to tip.
    pub bend: f32,

    // ── bloom timing ──
    /// Seconds from buried to full height.
    pub rise_time: f32,
    /// How far past full height the punch carries.
    pub rise_overshoot: f32,
    /// Seconds the overshoot takes to damp out.
    pub settle: f32,
    /// Seconds the wave takes to run around the ring.
    pub sweep_time: f32,
    /// Seconds before the skirt starts.
    pub skirt_delay: f32,
    /// How long the skirt takes to cross the band.
    pub skirt_wave: f32,
    /// Seconds before the spire comes up.
    pub core_delay: f32,
    /// Seconds of random delay on top of all of it.
    pub stagger: f32,
    /// Fraction of the hold the late shards are scattered over.
    pub bloom_spread: f32,
    /// Seconds the birth flash lasts.
    pub birth_fade: f32,

    // ── sheet + veil ──
    /// Thickness of the band at the edge, metres.
    pub field_boundary: f32,
    /// Hover distance above the floor, metres.
    pub field_height: f32,
    /// Master opacity of the curtain, `0` hides it.
    pub veil: f32,
    /// How high the curtain stands, metres.
    pub veil_height: f32,
    /// Where it stands, × `zone_radius`.
    pub veil_radius: f32,
    /// Revolutions/second the whole curtain turns.
    pub veil_spin: f32,

    // ── dynamic light ──
    /// Base intensity of the cast light.
    pub light_intensity: f32,
    /// Radius of the cast light, metres.
    pub light_radius: f32,
    /// How far up the crown the light sits, `0..1`.
    pub light_height: f32,
}

impl Default for GlacialCrownParams {
    fn default() -> Self {
        Self {
            range: 18.0,
            min_range: 0.0,
            zone_radius: 4.6,
            speed: 44.0,
            snap_time: 0.22,
            lifetime: 4.2,
            shatter_delay: 0.5,
            shatter_stagger: 0.45,
            sink_time: 1.15,
            cooldown: 1.6,
            speed_scale: 1.0,
            randomness: 1.0,

            hand_height: 1.22,
            hand_forward: 0.6,
            hand_side: 0.18,

            spike_count: 220.0,
            density: 1.0,
            ring_share: 0.6,
            core_share: 0.0,
            late_share: 0.12,
            ring_seat: 0.94,
            ring_scatter: 0.16,
            skirt_seat: 0.74,
            skirt_band: 0.42,
            skirt_bias: 0.9,
            core_spread: 0.16,

            ring_height: 1.4,
            ring_wave: 0.61,
            skirt_height: 1.7,
            core_height: 5.2,
            height_jitter: 0.65,
            ring_lean: 0.33,
            skirt_lean: 0.3,
            core_lean: 0.2,
            lean_jitter: 1.3,
            fan: 1.16,
            twist: 1.0,
            rubble: 0.53,
            rubble_scale: 0.34,

            crystal_radius: 0.375,
            radius_jitter: 0.94,
            taper: 0.36,
            facets: 7.0,
            roughness: 0.0,
            bend: 0.0,

            rise_time: 0.2,
            rise_overshoot: 0.3,
            settle: 0.5,
            sweep_time: 0.42,
            skirt_delay: 0.1,
            skirt_wave: 0.26,
            core_delay: 0.2,
            stagger: 0.07,
            bloom_spread: 0.7,
            birth_fade: 0.5,

            field_boundary: 0.4,
            field_height: 0.03,
            veil: 0.5,
            veil_height: 1.9,
            veil_radius: 1.02,
            veil_spin: 0.02,

            light_intensity: 14.0,
            light_radius: 16.0,
            light_height: 0.45,
        }
    }
}

impl GlacialCrownParams {
    /// Live footprint, metres (`max(0.05, zone_radius)`).
    pub fn radius(&self) -> f32 {
        self.zone_radius.max(0.05)
    }

    /// Shard budget: `clamp(round(spike_count * density), 1, MAX_CROWN_SPIKES)`.
    pub fn spike_budget(&self) -> usize {
        libm::roundf(self.spike_count * self.density)
            .clamp(1.0, MAX_CROWN_SPIKES as f32) as usize
    }

    /// Seconds the sheet takes to freeze out (`max(0.02, snap_time)`).
    pub fn snap_duration(&self) -> f32 {
        self.snap_time.max(0.02)
    }

    /// Seconds the crown stands (`max(0.2, lifetime)`).
    pub fn impact_duration(&self) -> f32 {
        self.lifetime.max(0.2)
    }

    /// Collapse duration: shatter delay + stagger + sink (`max(0.2, …)`).
    pub fn fade_duration(&self) -> f32 {
        (self.shatter_delay + self.shatter_stagger + self.sink_time).max(0.2)
    }
}

impl crate::aim::AimReach for GlacialCrownParams {
    fn cast_range(&self) -> f32 {
        self.range
    }

    fn cast_min_range(&self) -> f32 {
        self.min_range
    }
}

impl crate::aim::ZoneAimReach for GlacialCrownParams {
    fn zone_radius(&self) -> f32 {
        self.zone_radius
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_shipped_settings() {
        let p = FrostLanceParams::default();
        assert_eq!(p.range, 15.0);
        assert_eq!(p.speed, 26.0);
        assert_eq!(p.lifetime, 3.6);
        assert_eq!(p.width_near, 0.55);
        assert_eq!(p.width, 2.5);
        assert_eq!(p.height, 3.1);
        assert_eq!(p.facets, 7.0);
        assert_eq!(p.cooldown, 0.4);
        assert_eq!(p.light_intensity, 9.0);
        assert_eq!(p.light_radius, 13.0);
    }

    #[test]
    fn budget_clamps_to_ceiling() {
        let mut p = FrostLanceParams::default();
        assert_eq!(p.spike_budget(), 190);
        assert_eq!(p.impact_count(190), 42);
        p.spike_count = 10_000.0;
        assert_eq!(p.spike_budget(), MAX_SPIKES);
        p.spike_count = 0.0;
        assert_eq!(p.spike_budget(), 1);
    }

    #[test]
    fn phase_durations_have_floors() {
        let p = FrostLanceParams::default();
        assert!((p.impact_duration() - 3.6).abs() < f32::EPSILON);
        assert!((p.fade_duration() - 1.6).abs() < f32::EPSILON);
        let flat = FrostLanceParams {
            lifetime: 0.0,
            shatter_delay: 0.0,
            sink_time: 0.0,
            ..FrostLanceParams::default()
        };
        assert_eq!(flat.impact_duration(), 0.2);
        assert_eq!(flat.fade_duration(), 0.2);
    }

    #[test]
    fn thunder_defaults_match_shipped_settings() {
        let p = StormLanceParams::default();
        assert_eq!(p.range, 24.0);
        assert_eq!(p.min_range, 2.0);
        assert_eq!(p.speed, 105.0);
        assert_eq!(p.lifetime, 0.45);
        assert_eq!(p.fade_time, 0.5);
        assert_eq!(p.strands, 9.0);
        assert_eq!(p.restrike, 24.0);
        assert_eq!(p.jitter, 0.34);
        assert_eq!(p.light_intensity, 26.0);
        assert_eq!(p.strand_budget(), 9);
        assert_eq!(p.octave_count(), 4);
    }

    #[test]
    fn storm_budget_clamps_to_ceiling() {
        let mut p = StormLanceParams::default();
        p.strands = 100.0;
        assert_eq!(p.strand_budget(), MAX_STRANDS);
        p.strands = 0.0;
        assert_eq!(p.strand_budget(), 1);
    }

    #[test]
    fn storm_phase_durations_have_floors() {
        let p = StormLanceParams::default();
        assert!((p.impact_duration() - 0.45).abs() < f32::EPSILON);
        assert!((p.fade_duration() - 0.5).abs() < f32::EPSILON);
        let flat = StormLanceParams {
            lifetime: 0.0,
            fade_time: 0.0,
            ..StormLanceParams::default()
        };
        assert_eq!(flat.impact_duration(), 0.05);
        assert_eq!(flat.fade_duration(), 0.05);
    }

    #[test]
    fn meteor_defaults_match_shipped_settings() {
        let p = CinderFallParams::default();
        assert_eq!(p.range, 20.0);
        assert_eq!(p.min_range, 3.0);
        assert_eq!(p.speed, 21.0);
        assert_eq!(p.lifetime, 2.2);
        assert_eq!(p.fade_time, 1.6);
        assert_eq!(p.arc, 2.6);
        assert_eq!(p.arc_curve, 0.85);
        assert_eq!(p.radius, 0.8);
        assert_eq!(p.spin, 3.4);
        assert_eq!(p.charge_curve, 1.6);
        assert_eq!(p.fissure_arms, 6.0);
        assert_eq!(p.fissure_radius, 5.2);
        assert_eq!(p.chunk_count, 18.0);
        assert_eq!(p.light_intensity, 16.0);
        assert_eq!(p.chunk_budget(), 18);
        assert_eq!(p.fissure_arm_budget(), 6);
    }

    #[test]
    fn cinder_budgets_clamp() {
        let mut p = CinderFallParams::default();
        p.chunk_count = 100.0;
        assert_eq!(p.chunk_budget(), MAX_CHUNKS);
        p.chunk_count = -1.0;
        assert_eq!(p.chunk_budget(), 0);
        p.fissure_arms = 100.0;
        assert_eq!(p.fissure_arm_budget(), MAX_FISSURE_ARMS);
        p.fissure_arms = 1.0;
        assert_eq!(p.fissure_arm_budget(), 2);
    }

    #[test]
    fn cinder_phase_durations_have_floors() {
        let p = CinderFallParams::default();
        assert!((p.impact_duration() - 2.2).abs() < f32::EPSILON);
        assert!((p.fade_duration() - 1.6).abs() < f32::EPSILON);
        let flat = CinderFallParams {
            lifetime: 0.0,
            fade_time: 0.0,
            ..CinderFallParams::default()
        };
        assert_eq!(flat.impact_duration(), 0.2);
        assert_eq!(flat.fade_duration(), 0.2);
    }


    #[test]
    fn beam_defaults_match_shipped_settings() {
        let p = NovaBeamParams::default();
        assert_eq!(p.range, 26.0);
        assert_eq!(p.min_range, 3.0);
        assert_eq!(p.charge, 0.42);
        assert_eq!(p.speed, 150.0);
        assert_eq!(p.lifetime, 1.15);
        assert_eq!(p.fade_time, 0.4);
        assert_eq!(p.radius_near, 0.16);
        assert_eq!(p.radius, 0.77);
        assert_eq!(p.radius_curve, 1.27);
        assert_eq!(p.flare, 1.74);
        assert_eq!(p.rings, 10.0);
        assert_eq!(p.ring_speed, 1.31);
        assert_eq!(p.orb_size, 0.39);
        assert_eq!(p.light_intensity, 30.0);
        assert_eq!(p.muzzle_light_intensity, 16.0);
        assert_eq!(p.ring_budget(), 10);
    }

    #[test]
    fn nova_budgets_and_floors() {
        let mut p = NovaBeamParams::default();
        p.rings = 100.0;
        assert_eq!(p.ring_budget(), MAX_RINGS);
        p.rings = 0.0;
        assert_eq!(p.ring_budget(), 1);
        assert!((p.charge_duration() - 0.42).abs() < f32::EPSILON);
        assert!((p.impact_duration() - 1.15).abs() < f32::EPSILON);
        assert!((p.fade_duration() - 0.4).abs() < f32::EPSILON);
        let flat = NovaBeamParams {
            charge: 0.0,
            lifetime: 0.0,
            fade_time: 0.0,
            ..NovaBeamParams::default()
        };
        assert_eq!(flat.charge_duration(), 0.01);
        assert_eq!(flat.impact_duration(), 0.05);
        assert_eq!(flat.fade_duration(), 0.05);
    }

    #[test]
    fn snare_defaults_match_shipped_settings() {
        let p = VoltaicSnareParams::default();
        assert_eq!(p.range, 20.0);
        assert_eq!(p.min_range, 0.0);
        assert_eq!(p.zone_radius, 4.4);
        assert_eq!(p.speed, 62.0);
        assert_eq!(p.snap_time, 0.16);
        assert_eq!(p.lifetime, 2.6);
        assert_eq!(p.fade_time, 0.75);
        assert_eq!(p.height, 9.2);
        assert_eq!(p.leash_strands, 3.0);
        assert_eq!(p.strands, 15.0);
        assert_eq!(p.tendrils, 20.0);
        assert_eq!(p.rim_arcs, 14.0);
        assert_eq!(p.light_intensity, 24.0);
        assert_eq!(p.leash_budget(), 3);
        assert_eq!(p.column_budget(), 15);
        assert_eq!(p.tendril_budget(), 20);
        assert_eq!(p.rim_budget(), 14);
    }

    #[test]
    fn snare_budgets_and_floors() {
        let mut p = VoltaicSnareParams::default();
        p.strands = 100.0;
        assert_eq!(p.column_budget(), MAX_COLUMN);
        p.tendrils = 100.0;
        assert_eq!(p.tendril_budget(), MAX_TENDRIL);
        p.rim_arcs = 100.0;
        assert_eq!(p.rim_budget(), MAX_RIM);
        p.leash_strands = 100.0;
        assert_eq!(p.leash_budget(), MAX_LEASH);
        assert!((p.snap_duration() - 0.16).abs() < f32::EPSILON);
        assert!((p.impact_duration() - 2.6).abs() < f32::EPSILON);
        assert!((p.fade_duration() - 0.75).abs() < f32::EPSILON);
        let flat = VoltaicSnareParams {
            snap_time: 0.0,
            lifetime: 0.0,
            fade_time: 0.0,
            ..VoltaicSnareParams::default()
        };
        assert_eq!(flat.snap_duration(), 0.01);
        assert_eq!(flat.impact_duration(), 0.05);
        assert_eq!(flat.fade_duration(), 0.05);
    }

    #[test]
    fn glacier_defaults_match_shipped_settings() {
        let p = GlacialCrownParams::default();
        assert_eq!(p.range, 18.0);
        assert_eq!(p.min_range, 0.0);
        assert_eq!(p.zone_radius, 4.6);
        assert_eq!(p.speed, 44.0);
        assert_eq!(p.snap_time, 0.22);
        assert_eq!(p.lifetime, 4.2);
        assert_eq!(p.shatter_delay, 0.5);
        assert_eq!(p.shatter_stagger, 0.45);
        assert_eq!(p.sink_time, 1.15);
        assert_eq!(p.spike_count, 220.0);
        assert_eq!(p.ring_share, 0.6);
        assert_eq!(p.core_share, 0.0);
        assert_eq!(p.ring_height, 1.4);
        assert_eq!(p.veil, 0.5);
        assert_eq!(p.light_intensity, 14.0);
        assert_eq!(p.spike_budget(), 220);
    }

    #[test]
    fn glacier_budgets_and_floors() {
        let mut p = GlacialCrownParams::default();
        p.spike_count = 10_000.0;
        assert_eq!(p.spike_budget(), MAX_CROWN_SPIKES);
        p.spike_count = 0.0;
        assert_eq!(p.spike_budget(), 1);
        assert!((p.snap_duration() - 0.22).abs() < f32::EPSILON);
        assert!((p.impact_duration() - 4.2).abs() < f32::EPSILON);
        assert!((p.fade_duration() - 2.1).abs() < f32::EPSILON);
        let flat = GlacialCrownParams {
            snap_time: 0.0,
            lifetime: 0.0,
            shatter_delay: 0.0,
            shatter_stagger: 0.0,
            sink_time: 0.0,
            ..GlacialCrownParams::default()
        };
        assert_eq!(flat.snap_duration(), 0.02);
        assert_eq!(flat.impact_duration(), 0.2);
        assert_eq!(flat.fade_duration(), 0.2);
    }

}
