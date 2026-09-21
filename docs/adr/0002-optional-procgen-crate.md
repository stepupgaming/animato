# ADR 0002: Optional Procedural-Geometry (`animato-procgen`) Crate

- Status: Accepted
- Date: 2026-09-21
- Branch: `feat/issue-1-composition-elemental-sandbox` (fork: `stepupgaming/animato`)
- Extends: ADR 0001 (optional composition and FX crates)

## Context

The product request (see <https://github.com/stepupgaming/animato/issues/1>)
asks for procedural geometry as an optional Animato edge — the vocabulary
from Wawa Sensei's video <https://www.youtube.com/watch?v=ywYOIk3rgHw>:
a Delaunay / Voronoi card deck for generative 3D. A future Elemental/FX
sandbox will consume such geometry, but nothing about it belongs in the
tween engine or core traits.

## Decision

1. **New workspace crate `animato-procgen`** (pure Rust, no Three.js, no new
   non-MIT/Apache dependencies — only `animato-core` + `libm`):
   - `triangulate`: Delaunay triangulation, 2D points → CCW index triangles
     (Bowyer–Watson).
   - `voronoi`: Voronoi diagram via exact half-plane clipping — the Delaunay
     dual computed directly, cells tile the caller's `Bounds`.
   - `lloyd_relax`: iterative centroidal-Voronoi (Lloyd) smoothing.
   - `poisson_disk`: Bridson Poisson-disk sampling with a deterministic
     embedded RNG (`SmallRng`; no `rand` dependency) for reproducible decks.
   - `WorleyField`: cellular-noise sampler (`F1` / `F2` / `F2-F1` edges).
   - `CausticField`: **starter** analytic caustics-style intensity field —
     Worley-edge glow over time-wobbled sites. A pure, seekable function of
     `(x, y, time)`; explicitly *not* light transport.
2. **Drivers/fields FEED Animato clocks; nothing is stuffed into
   tween/core.** `CausticField` implements `animato_core::{Update, Playable}`
   (endless ambient field: `duration() == INFINITY`, `seek_to` maps progress
   onto its wobble `period`), so existing `Clock`s / drivers can advance and
   seek it. No core-crate public API changes.
3. **Optional facade feature `procgen`** on the `animato` umbrella crate
   re-exports the public API (`Bounds`, `CausticField`, `WorleyField`,
   `triangulate`, `voronoi`, `lloyd_relax`, `poisson_disk`, …). Facade
   `default` features are unchanged.
4. **Elemental FX is still out of scope.** This phase only lands the geometry
   primitives + docs + tests. The future Elemental sandbox port consumes this
   crate through the same optional-feature pattern.

## Consequences

- `cargo test -p animato-procgen` covers every primitive (Delaunay area /
  orientation / empty-circle property, Voronoi tiling + containment, Lloyd
  spreading + centroidal fixed-point, Poisson separation + determinism,
  Worley brute-force parity, caustics bounds / seekability / driver
  contract).
- Downstream renderers (a future Elemental/FX crate, Bevy, WASM) map
  `Point` sets / triangles / intensity samples to their own buffers; procgen
  never touches GPU or DOM types.
- `CausticField` docs clearly state the starter status so nobody mistakes the
  analytic glow for a light-transport simulation.
