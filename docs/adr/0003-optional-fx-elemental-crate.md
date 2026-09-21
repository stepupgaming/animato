# ADR 0003: Optional Elemental-FX (`animato-fx-elemental`) Crate

- Status: Accepted
- Date: 2026-09-21
- Branch: `feat/issue-1-composition-elemental-sandbox` (fork: `stepupgaming/animato`)
- Extends: ADR 0001 (optional composition and FX crates), ADR 0002 (optional procgen crate)

## Context

The product request (see <https://github.com/stepupgaming/animato/issues/1>)
names the Elemental Sandbox as the priority: a skillshot VFX sandbox where
abilities are aimed like LoL skillshots and erupt as procedural fields. The
concrete upstream reference is achrefelouafi's
[LinearAbiltyCastingThreeJS](https://github.com/achrefelouafi/LinearAbiltyCastingThreeJS)
(MIT) — a Three.js / Vite / hand-written-GLSL sandbox with five abilities.

Of the five, **Frost Lance (Q)** is the most representative *line-cast* for a
first port: press to arm, a LoL-style arrow swings with the pointer, click to
fire, and a fracture front races down the line while a procedural crystal
field erupts behind it — dense and ankle-high at the caster, opening into a
wall of blades plus an impact cluster at the far end. Its CPU-side structure
(front advance, dice-only records, live-param resolution) is exactly the
seekable / time-driven shape `animato-composition` / `Timeline::seek_abs` can
drive, and it ports without raymarching, ribbon strips or parametric tubes
(the blockers for the other four abilities).

As with ADR 0001 / 0002, none of this belongs in the tween engine or core
traits: it must land as an optional edge crate with unchanged defaults.

## Decision

1. **New workspace crate `animato-fx-elemental`** (pure Rust, no Three.js, no
   browser, no new non-MIT/Apache dependencies — only `animato-core` +
   `libm`, plus optional `wgpu`):
   - `aim::solve_aim` — floor-plane line-cast solve with `[min_range, range]`
     clamping and `min_range` refusal (ports `AimController.js`; raycast/DOM
     omitted).
   - `FrostLance` pipeline (`pipeline.rs`) — one ability end-to-end: aim →
     spawn → travelling fracture front → spike eruption → impact cluster →
     withdrawal, with `Phase::{Idle, Travel, Impact, Fade, Done}` and
     `FrostEvent::{Impact, Fade, Done}` for renderer-owned systems.
   - Dice-only records (`spike.rs::roll_spikes`, seeded `FxRng` replacing
     `Math.random()`): `MAX_SPIKES = 288`, `VARIANTS = 3` draw split,
     22% impact slice (`IMPACT_FRACTION`), front-biased placement, domed /
     crowned heights, rubble demotion, springy overshoot emergence and birth
     flash — all resolved against live `FrostLanceParams` (`Default` =
     shipped `settings.ice`) at sample time.
   - **Seekable / `Playable`.** `FrostLance::seek_abs(t)` deterministically
     re-simulates from spawn at fixed 1/480 s step (`SEEK_STEP`): same
     `(seed, params, time)` ⇒ same state. `FrostLance` implements
     `animato_core::{Update, Playable}` (`Send + 'static`), so
     `Composition::add("fx", "frost-lance", lance, start)` and
     `Timeline::seek_abs` drive it like any other clip — including while
     paused, with params edited live (records store dice only).
   - **No GPU in the default build.** The pipeline resolves every spike
     transform, front position, phase transition and light state as plain
     data (`SpikeSample`, `front_position`, `impact_position`,
     `erupted_count`, `breaching_count`, `light`). `cargo test` needs no GPU.
2. **Optional facade features `fx-elemental` / `elemental`** on the `animato`
   umbrella crate re-export the public API (`FrostLance`, `FrostLanceParams`,
   `solve_aim`, …). `elemental` is a plain alias of `fx-elemental`. Facade
   `default` features are unchanged.
3. **Attribution.** Upstream is MIT; obligations are honored by
   `ATTRIBUTION.md` plus the Three.js / GLSL → Rust module mapping in the
   crate README. No binary assets (FBX, PNG, HDR) were taken — only pipeline
   structure, timing curves and parameter defaults, re-expressed in
   renderer-agnostic Rust.
4. **Follow-ups.** **Storm Lance (E)** is now in-crate (`StormLance` /
   `StormLanceParams` / `filament.rs` — ThunderAbility port with restrike
   polylines). **Cinder Fall (R)** is now in-crate (`CinderFall` /
   `CinderFallParams` / `fissure.rs` — MeteorAbility port with ballistic arc,
   chunk ballistics and molten fissures). Remaining sandbox abilities (Nova
   Beam, Voltaic Snare), Ext / Extended sandboxes, and a full `wgpu` renderer
   backend (particles, decals, ice/lightning/meteor shading) are not started
   here.

## Consequences

- `cargo test -p animato-fx-elemental` covers the contract: aim clamp /
  refusal, front monotonicity + saturation, full lifecycle to `Done`,
  `seek_abs` determinism + terminal seek, seek-vs-fine-stepped-playback
  parity, params-edit-reshapes-standing-field-while-paused, sane samples
  through retract, light-tracks-front-then-punches-at-impact, `Playable`
  progress, `Composition` ownership + seeking, spike-dice split /
  determinism / doming / emergence / birth bounds.
- Particles, ground decals, burst shells, camera shake and screen flash stay
  **renderer-owned downstream**: the pipeline exposes every stimulus they
  need (front position, breach counts, `FrostEvent`s, `FrostLight` with boost
  decay + shimmer) and never touches GPU or DOM types.
- The thin optional `wgpu` hook (`src/gpu.rs`: instance-buffer layout for the
  three crystal-variant instanced draws, feature `wgpu`, off by default) is
  **intentionally an instance-layout stub, not a renderer** — it only
  describes how resolved `SpikeSample` data reaches a renderer. The crate
  README says so explicitly so nobody mistakes it for a rendering backend.
