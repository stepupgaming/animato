# animato-fx-elemental

Optional elemental-VFX edge crate for Animato (pure Rust, no browser /
Three.js required): a seekable, time-driven port of **one complete ability
pipeline end-to-end** — the **Frost Lance (Q)** line-cast — from
achrefelouafi's `LinearAbiltyCastingThreeJS` sandbox (MIT).

It contains no renderer: the pipeline resolves every spike transform, the
fracture-front position, phase transitions and light state as plain data.
GPU particles, ground decals, burst shells, camera shake and screen flash stay
renderer-owned downstream (the pipeline exposes every stimulus they need).

## Why Frost Lance

Of the five sandbox abilities, Frost Lance (Q) is the most representative
*line-cast*: press to arm, a LoL-style arrow swings with the pointer, click to
fire, and a fracture front races down the line while a procedural crystal
field erupts behind it — dense and ankle-high at the caster, opening into a
wall of blades plus an impact cluster at the far end. Its CPU-side structure
(front advance, dice-only records, live-param resolution) is exactly the
seekable/time-driven shape `animato-composition` / `Timeline::seek_abs` can
drive, and it ports without raymarching, ribbon strips or parametric tubes.

## Three.js / GLSL → Rust module mapping

| Original (`LinearAbiltyCastingThreeJS`) | This crate | Notes |
|---|---|---|
| `src/abilities/Ability.js` — phase machine, `spawn`, `advance`, `pointAt`, `_updateLight` | `src/pipeline.rs` — `FrostLance`, `Phase`, `FrostLight` | `advance` ease-off-standstill, light boost decay and shimmer ported verbatim |
| `src/abilities/IceAbility.js` — dice records, `_triggerUpTo`, `_emergence`, `_spikeHeight/Radius/Position`, `onSpawn/onTravel/onImpact/onFade`, impact/fade durations | `src/pipeline.rs` + `src/spike.rs` | Particle/decal/burst/shake/flash *calls* replaced by `FrostEvent` + query methods (`erupted_count`, `breaching_count`, `front_position`, `impact_position`) |
| `src/abilities/IceAbility.js` — `MAX_SPIKES = 288`, `VARIANTS = 3`, 22% impact slice | `src/params.rs` (`MAX_SPIKES`, `IMPACT_FRACTION`), `src/spike.rs::roll_spikes` | Seeded `FxRng` replaces `Math.random()` so casts are reproducible |
| `src/config/settings.js` — `ice` block | `src/params.rs::FrostLanceParams` (`Default` = shipped values) | CPU-resolved dims only; shader-only material tuning stays in GLSL downstream |
| `src/input/AimController.js` — `[minRange, range]` clamp, `minRange` refusal | `src/aim.rs::solve_aim` | Raycast/DOM omitted; floor-plane math only |
| `src/utils/math.js` — `saturate/lerp/smoothstep`, `Easing.outQuad/outQuint/inQuad/inCubic` | `src/math.rs` | `libm`-backed for `no_std` |
| `src/materials/IceMaterial.js` (patched `MeshStandardMaterial`: thickness tint, fracture, frost, glint, birth flash) | *Downstream renderer* (wgpu feature hook: `src/gpu.rs`) | `birth` attribute values are computed here (`SpikeSample::birth`); shading stays GLSL/WGSL |
| `src/particles/*` — mist / shards / glitter systems + `RateEmitter` | *Downstream renderer* | Pipeline exposes rates' stimuli (front pos, breach counts); emission itself is renderer-owned |
| `src/effects/GroundDecals.js` (frost patches, shockwave), `BurstSphere.js`, `CameraShake.js`, `ScreenFlash.js`, `LightPool.js` | *Downstream renderer* | Fired off `FrostEvent::{Impact,Fade,Done}` + `FrostLight` |
| `src/assets/ProceduralGeometry.js` — crystal geometry (`facets/taper/roughness/bend`) | *Downstream renderer* | Params carried (`taper/facets/roughness/bend`) as the geometry-bake key; pipeline is geometry-agnostic |
| Future: `wgpu` backend for the three instanced crystal draws | `src/gpu.rs` (feature `wgpu`, off by default) | Instance-buffer layout stub only |

## Time model

- `FrostLance::update(dt)` — live playback (frame-rate independent, like `Ability#update`).
- `FrostLance::seek_abs(t)` — deterministic re-simulation from spawn at a
  fixed 1/480 s step (`SEEK_STEP`): same `(seed, params, time)` ⇒ same state.
- `FrostLance` implements `animato_core::{Playable, Update}` (`Send +
  'static`), so it composes directly:

```rust
use animato_fx_elemental::FrostLance;

let mut lance = FrostLance::new();
lance.cast([0.0, 0.0], [0.0, 1.0], 12.0, 7).unwrap();

let mut comp = animato_composition::Composition::new();
comp.add("fx", "frost-lance", lance, 0.25);
comp.seek_abs(1.0); // synchronizes the pipeline like any other clip
```

The original's edit-while-paused rule is preserved: records store dice only,
so mutating `FrostLance::params_mut()` re-shapes a standing field with the
clock stopped.

## Attribution / license

Derived from [LinearAbiltyCastingThreeJS](https://github.com/achrefelouafi/LinearAbiltyCastingThreeJS)
by achrefelouafi, MIT-licensed — see `ATTRIBUTION.md`. No binary assets
(FBX, PNG, HDR) were taken; only pipeline structure, timing curves and
parameter defaults, re-expressed in renderer-agnostic Rust.

## Follow-ups (not started)

- Remaining sandbox abilities: **Storm Lance** (bolt ribbon + restrike),
  **Cinder Fall** (arced meteor + fissures), **Nova Beam** (parametric tube +
  charge phase), **Voltaic Snare** (far-cast circle + ribbon cage).
- Ext / Extended sandboxes.
- Full `wgpu` renderer backend (particles, decals, ice shading) behind the
  `wgpu` feature; default build stays GPU-free so `cargo test` needs no GPU.
