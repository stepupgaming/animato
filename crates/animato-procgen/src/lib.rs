//! # animato-procgen
//!
//! Optional procedural-geometry edge crate for Animato (pure Rust, no
//! Three.js): the Delaunay / Voronoi / Poisson / Worley vocabulary from
//! Wawa Sensei's generative-3D card-deck video, as drivers and fields that
//! **feed** Animato clocks — never stuffed into `tween`/`core`.
//!
//! - [`triangulate`] — Delaunay triangulation (`2D points → triangles`).
//! - [`voronoi`] — Voronoi diagram (exact half-plane-intersection dual).
//! - [`lloyd_relax`] — iterative centroidal-Voronoi smoothing.
//! - [`poisson_disk`] — Bridson Poisson-disk site sampling.
//! - [`WorleyField`] — cellular-noise (`F1`/`F2`) sampler.
//! - [`CausticField`] — **starter** analytic caustics-style intensity field
//!   (seekable function of time + site set, not light transport).
//!
//! ## Quick Start
//!
//! ```rust
//! use animato_procgen::{Bounds, CausticField, Point, lloyd_relax, poisson_disk, triangulate, voronoi};
//!
//! let bounds = Bounds::unit();
//! let sites = poisson_disk(bounds, 0.2, 42);
//! let smooth = lloyd_relax(&sites, bounds, 3);
//! let tris = triangulate(&smooth);
//! let cells = voronoi(&smooth, bounds);
//! assert_eq!(cells.len(), smooth.len());
//! assert!(!tris.is_empty());
//!
//! let mut caustics = CausticField::new(smooth);
//! caustics.seek_time(1.0);
//! let glow: f32 = caustics.sample(Point::new(0.5, 0.5));
//! assert!((0.0..=1.0).contains(&glow));
//! ```

//! ## Demo clips: quality rejected, cinematic redo pending
//!
//! The MP4/GIF loops under `docs/examples/procgen/` are **placeholder
//! quality only** — the user reviewed them and rejected them as cheap
//! toy sketches (see `docs/examples/procgen/QUALITY_REJECTED.md`). Do NOT
//! treat them as shippable examples and do NOT link them as done.
//!
//! A cinematic redo is required before examples count as finished: 720p
//! product-grade MP4 loops that actually read as Voronoi/Lloyd, Delaunay,
//! Poisson/Worley, and caustics. A separate agent owns that redo and
//! coordinates by replacing files in `docs/examples/procgen/`; this crate
//! makes no claim about those clips until the redo lands.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

extern crate alloc;

pub mod caustics;
pub mod delaunay;
pub mod lloyd;
pub mod point;
pub mod poisson;
pub mod rng;
pub mod voronoi;
pub mod worley;

pub use caustics::CausticField;
pub use delaunay::{signed_area2, triangulate};
pub use lloyd::lloyd_relax;
pub use point::Point;
pub use poisson::poisson_disk;
pub use rng::SmallRng;
pub use voronoi::{Bounds, VoronoiCell, polygon_area, polygon_centroid, voronoi};
pub use worley::WorleyField;
