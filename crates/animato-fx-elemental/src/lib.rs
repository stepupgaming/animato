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
//! - Time-driven: `seek_abs` re-simulates deterministically, so every pipeline
//!   is drivable by `animato_composition::Composition` and
//!   `animato_timeline::Timeline::seek_abs` — including while paused, with
//!   params edited live (records store dice only; metres resolve at sample
//!   time).
//! - All four implement [`animato_core::Playable`] (`Send + 'static`).
//!
//! ## Quick Start
//!
//! ```rust
//! use animato_fx_elemental::{
//!     CinderFall, CinderFallParams, FrostLance, FrostLanceParams, NovaBeam,
//!     NovaBeamParams, StormLance, StormLanceParams,
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
//! ```
//!
//! ## Attribution
//!
//! Ported from [LinearAbiltyCastingThreeJS](https://github.com/achrefelouafi/LinearAbiltyCastingThreeJS)
//! by achrefelouafi (MIT). See `ATTRIBUTION.md` and the crate README for the
//! module mapping. No textures, meshes or HDR assets were taken — only the
//! pipeline structure, timing curves and parameter defaults, re-expressed in
//! renderer-agnostic Rust.
//!
//! ## Scope
//!
//! Frost Lance, Storm Lance, Cinder Fall and Nova Beam are in-crate.
//! Voltaic Snare — plus any Ext/Extended sandboxes — are follow-ups (see
//! README).
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

#[cfg(feature = "wgpu")]
pub mod gpu;

pub use aim::{AimReach, AimSolution, solve_aim};
pub use filament::{StrandNode, StrandRecord, StrandSample, roll_strands};
pub use params::{
    CinderFallParams, FISSURE_STEP, FrostLanceParams, IMPACT_FRACTION, MAX_CHUNKS,
    MAX_COILS, MAX_FISSURE_ARMS, MAX_FISSURE_BRANCHES, MAX_RINGS, MAX_SPIKES,
    MAX_STRANDS, NovaBeamParams, STRAND_NODES, StormLanceParams, TUBE_SEGMENTS,
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
