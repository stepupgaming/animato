//! # animato-fx-elemental
//!
//! Optional elemental-VFX edge crate for Animato (pure Rust, no Three.js):
//! a seekable, time-driven port of the **Frost Lance (Q)** line-cast pipeline
//! from achrefelouafi's `LinearAbiltyCastingThreeJS` sandbox (MIT).
//!
//! - [`FrostLanceParams`] — procedural params (defaults = shipped `settings.ice`).
//! - [`FrostLance`] — the complete ability pipeline end-to-end: aim → spawn →
//!   travelling fracture front → spike eruption → impact cluster → withdrawal,
//!   with dynamic-light bookkeeping and phase events for renderer-owned
//!   systems (particles, decals, shake, flash).
//! - Time-driven: [`FrostLance::seek_abs`] re-simulates deterministically, so
//!   the pipeline is drivable by `animato_composition::Composition` and
//!   `animato_timeline::Timeline::seek_abs` — including while paused, with
//!   params edited live (the original's edit-while-paused rule: records store
//!   dice only, every metre/radian/second resolves at sample time).
//! - [`FrostLance`] implements [`animato_core::Playable`] (`Send + 'static`),
//!   so `Composition::add("fx", "frost-lance", lance, start)` just works.
//!
//! ## Quick Start
//!
//! ```rust
//! use animato_fx_elemental::{FrostLance, FrostLanceParams};
//!
//! let mut lance = FrostLance::with_params(FrostLanceParams::default());
//! lance.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();
//!
//! // Scrub like a timeline.
//! lance.seek_abs(1.0);
//! assert!(lance.erupted_count() > 0);
//!
//! // …or drive it through a composition.
//! let mut comp = animato_composition::Composition::new();
//! comp.add("fx", "frost-lance", lance, 0.25);
//! comp.seek_abs(1.0);
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
//! Phase 2 covers **one** line-cast ability (Frost Lance). Storm Lance,
//! Cinder Fall, Nova Beam and Voltaic Snare — plus any Ext/Extended sandboxes
//! — are follow-ups (see README).
//!
//! The optional `wgpu` feature (`gpu.rs`) is an instance-layout stub only,
//! not a renderer: the default build stays GPU-free. See the crate README
//! ("GPU / `wgpu` scope") and `docs/adr/0003-optional-fx-elemental-crate.md`.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

extern crate alloc;

pub mod aim;
pub mod math;
pub mod params;
pub mod pipeline;
pub mod rng;
pub mod spike;

#[cfg(feature = "wgpu")]
pub mod gpu;

pub use aim::{AimSolution, solve_aim};
pub use params::{FrostLanceParams, IMPACT_FRACTION, MAX_SPIKES};
pub use pipeline::{FrostEvent, FrostLance, FrostLight, Phase, SpawnError, SEEK_STEP};
pub use spike::{SpikeRecord, SpikeSample, roll_spikes};
