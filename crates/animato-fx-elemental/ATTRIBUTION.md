# Attribution — elemental VFX source

`animato-fx-elemental` ports the **Frost Lance (Q)**, **Storm Lance (E)**, **Cinder Fall (R)**, **Nova Beam (F)**, **Voltaic Snare (V)** and **Glacial Crown (X)** ability pipelines from:

- **Project:** [LinearAbiltyCastingThreeJS](https://github.com/achrefelouafi/LinearAbiltyCastingThreeJS)
  ("Elemental Sandbox" — a skillshot VFX sandbox built with Three.js, Vite
  and hand-written GLSL)
- **Author:** [achrefelouafi](https://github.com/achrefelouafi)
- **License:** MIT (see the `LICENSE` file at the repository root above)

## What was taken

Only the intangible structure, re-expressed in renderer-agnostic Rust:

- the ability phase machine (`Ability.js`), Frost Lance eruption logic
  (`IceAbility.js`), Storm Lance filament / restrike logic
  (`ThunderAbility.js` + `LightningMaterial.js`), Cinder Fall ballistic
  arc / debris / fissure logic (`MeteorAbility.js` + `GroundFissures.js`),
  Nova Beam charge / tube / ring logic (`BeamAbility.js`), Voltaic Snare
  zone / leash / cage logic (`SnareAbility.js` + `SnareMaterial.js`), Glacial
  Crown zone / ring / skirt / veil logic (`GlacierAbility.js` +
  `GlacierMaterial.js` / `FrostFieldMaterial.js`), timing curves and the
  dice-only record rule;
- the `ice`, `thunder`, `meteor`, `beam`, `snare` and `glacier` parameter-block defaults (`src/config/settings.js`);
- the aim-clamp / `minRange`-refusal rule and zone-cast circle solve (`src/input/AimController.js`);
- the maths helpers (`src/utils/math.js`).

## What was NOT taken

No binary or authored assets: no FBX character/rig, no `diffuse.png`, no HDR
probe, no textures or sprite sheets (the original generates all VFX
procedurally anyway). No JavaScript/TypeScript or GLSL source was copied
verbatim into this repository.

## License of this crate

This crate is distributed under the workspace license (`MIT OR Apache-2.0`),
with the MIT obligations toward the upstream project honored by this notice
and the mapping in `README.md`.
