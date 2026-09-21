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
}
