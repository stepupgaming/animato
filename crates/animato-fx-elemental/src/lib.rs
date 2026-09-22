//! # animato-fx-elemental
//!
//! Optional elemental-VFX edge crate for Animato (pure Rust, no Three.js):
//! seekable, time-driven ports of line-cast ability pipelines from
//! achrefelouafi's `LinearAbiltyCastingThreeJS` sandbox (MIT).
//!
//! - [`FrostLance`] / [`FrostLanceParams`] — Frost Lance (Q) crystal field.
//! - [`StormLance`] / [`StormLanceParams`] — Storm Lance (E) bolt filament
//!   bundle with restrike (ThunderAbility port).
//! - [`CinderFall`] / [`CinderFallParams`] — Cinder Fall (R) arced meteor with
//!   impact debris and molten fissures (MeteorAbility port).
//! - [`NovaBeam`] / [`NovaBeamParams`] — Nova Beam (F) charge + sustained
//!   parametric tube with shock discs (BeamAbility port).
//! - [`VoltaicSnare`] / [`VoltaicSnareParams`] — Voltaic Snare (V) far-cast
//!   zone trap with leash travel + lightning cage (SnareAbility port).
//! - [`GlacialCrown`] / [`GlacialCrownParams`] — Glacial Crown (X) far-cast
//!   ice ring / skirt crown (GlacierAbility ZONE port).
//! - [`PyreCrown`] / [`PyreCrownParams`] — Pyre Crown (Q) far-cast fire ring /
//!   skirt crown (Ext `PyreAbility` ZONE port).
//! - Time-driven: `seek_abs` re-simulates deterministically, so every pipeline
//!   is drivable by `animato_composition::Composition` and
//!   `animato_timeline::Timeline::seek_abs` — including while paused, with
//!   params edited live (records store dice only; metres resolve at sample
//!   time).
//! - All seven implement [`animato_core::Playable`] (`Send + 'static`).
//!
//! ## Quick Start
//!
//! ```rust
//! use animato_fx_elemental::{
//!     CinderFall, CinderFallParams, FrostLance, FrostLanceParams, GlacialCrown,
//!     GlacialCrownParams, NovaBeam, NovaBeamParams, PyreCrown, PyreCrownParams,
//!     StormLance, StormLanceParams, VoltaicSnare, VoltaicSnareParams,
//! };
//!
//! let mut frost = FrostLance::with_params(FrostLanceParams::default());
//! frost.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! frost.seek_abs(1.0);
//! assert!(frost.erupted_count() > 0);
//!
//! let mut storm = StormLance::with_params(StormLanceParams::default());
//! storm.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! storm.seek_abs(0.3);
//! assert!(!storm.samples().is_empty());
//!
//! let mut cinder = CinderFall::with_params(CinderFallParams::default());
//! cinder.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! cinder.seek_abs(0.5);
//! assert!(cinder.rock().visible);
//!
//! let mut nova = NovaBeam::with_params(NovaBeamParams::default());
//! nova.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! nova.seek_abs(0.2);
//! assert_eq!(nova.phase(), animato_fx_elemental::Phase::Charge);
//!
//! let mut snare = VoltaicSnare::with_params(VoltaicSnareParams::default());
//! snare.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! snare.seek_abs(0.5);
//! assert!(!snare.samples().is_empty());
//!
//! let mut crown = GlacialCrown::with_params(GlacialCrownParams::default());
//! crown.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! crown.seek_abs(0.6);
//! assert!(!crown.samples().is_empty());
//!
//! let mut pyre = PyreCrown::with_params(PyreCrownParams::default());
//! pyre.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//! pyre.seek_abs(0.6);
//! assert!(!pyre.samples().is_empty());
//! ```
//!
//! ## Attribution
//!
//! Ported from [LinearAbiltyCastingThreeJS](https://github.com/achrefelouafi/LinearAbiltyCastingThreeJS)
//! and [LinearAbilityExtThreeJS](https://github.com/achrefelouafi/LinearAbilityExtThreeJS)
//! by achrefelouafi (MIT). See `ATTRIBUTION.md` and the crate README for the
//! module mapping. No textures, meshes or HDR assets were taken — only the
//! pipeline structure, timing curves and parameter defaults, re-expressed in
//! renderer-agnostic Rust.
//!
//! ## Scope
//!
//! Frost Lance, Storm Lance, Cinder Fall, Nova Beam, Voltaic Snare and
//! Glacial Crown are in-crate (ZONE casts via [`solve_zone_aim`]) — completing
//! the LinearAbilityCastingThreeJS ability set. Ext ports (Pyre Crown and
//! later) stay additive in this same optional crate (ADR 0004); see README.
//!
//! The optional `wgpu` feature (`gpu.rs`) is an instance-layout stub only,
//! not a renderer: the default build stays GPU-free. See the crate README
//! ("GPU / `wgpu` scope") and `docs/adr/0003-optional-fx-elemental-crate.md`.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

extern crate alloc;

pub mod aim;
pub mod filament;
pub mod math;
pub mod params;
pub mod pipeline;
pub mod rng;
pub mod spike;
pub mod storm;
pub mod cinder;
pub mod fissure;
pub mod nova;
pub mod cage;
pub mod voltaic;
pub mod crown;
pub mod glacial;
pub mod pyre;
pub mod pyrecrown;

#[cfg(feature = "wgpu")]
pub mod gpu;

pub use aim::{
    AimReach, AimSolution, ZoneAimReach, ZoneAimSolution, solve_aim, solve_zone_aim,
    solve_zone_aim_at,
};
pub use filament::{StrandNode, StrandRecord, StrandSample, roll_strands};
pub use params::{
    CAGE_NODES, CinderFallParams, FISSURE_STEP, FrostLanceParams, GlacialCrownParams,
    IMPACT_FRACTION, MAX_CHUNKS, MAX_COILS, MAX_COLUMN, MAX_CROWN_SPIKES,
    MAX_FISSURE_ARMS, MAX_FISSURE_BRANCHES, MAX_LEASH, MAX_PYRE_SPIKES, MAX_RIM,
    MAX_RINGS, MAX_SPIKES, MAX_STRANDS, MAX_TENDRIL, NovaBeamParams, PyreCrownParams,
    STRAND_NODES, StormLanceParams, TUBE_SEGMENTS, VoltaicSnareParams,
};
pub use pipeline::{FrostEvent, FrostLance, FrostLight, Phase, SpawnError, SEEK_STEP};
pub use rng::FxRng;
pub use spike::{SpikeRecord, SpikeSample, roll_spikes};
pub use storm::{StormEvent, StormLance, StormLight};
pub use cinder::{CinderEvent, CinderFall, CinderLight, RockSample};
pub use fissure::{
    ChunkRecord, ChunkSample, FissureArmRecord, FissureBranchRecord, FissureNode,
    FissureSample, arc_point, heading_at, roll_chunks, roll_fissures, sample_chunk,
    sample_fissures,
};
pub use nova::{
    NovaBeam, NovaEvent, NovaLight, OrbSample, RingRecord, RingSample, TubeNode,
    axis_point as beam_axis_point, beam_radius, roll_rings, sample_ring,
};

pub use cage::{
    CageContext, CageNode, CageRecord, CageSample, FilamentRole, climb_amount, open_amount,
    roll_cage, sample_cage, sample_filament,
};
pub use voltaic::{VoltaicEvent, VoltaicLight, VoltaicSnare};
pub use crown::{
    CrownRecord, CrownSample, FieldSample, ShardRole, VeilSample, angle_delta,
    birth_flash, emergence, growth, roll_crown, sample_crown, sample_field,
    sample_shard, sample_veil, schedule_eruption, shatter_amount, shard_fan,
    shard_height, shard_lean, shard_position, shard_radius,
};
pub use glacial::{GlacialCrown, GlacialEvent, GlacialLight};
pub use pyre::{
    BladeRole, EmberFieldSample, FlameVeilSample, HeatHazeSample, PyreRecord,
    PyreSample, blade_fan, blade_height, blade_lean, blade_position, blade_radius,
    char_amount, ignition, pyre_angle_delta, pyre_birth_flash, pyre_emergence,
    roll_pyre, sample_blade, sample_ember_field, sample_flame_veil, sample_heat_haze,
    sample_pyre, schedule_pyre_eruption,
};
pub use pyrecrown::{PyreCrown, PyreEvent, PyreLight};
