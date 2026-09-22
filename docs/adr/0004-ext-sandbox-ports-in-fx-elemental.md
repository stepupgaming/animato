# ADR 0004: Ext Sandbox Ports Stay in `animato-fx-elemental`

- Status: Accepted
- Date: 2026-09-21
- Branch: `feat/issue-1-composition-elemental-sandbox` (fork: `stepupgaming/animato`)
- Extends: ADR 0003 (optional fx-elemental crate)
- Amends: ADR 0003 §Decision follow-ups (Ext sandboxes)

## Context

ADR 0003 landed the six LinearAbilityCastingThreeJS abilities in the optional
`animato-fx-elemental` edge crate and left **Ext / Extended** sandboxes as
follow-ups. The first Ext ability is **Pyre Crown (Q)** from
[LinearAbilityExtThreeJS](https://github.com/achrefelouafi/LinearAbilityExtThreeJS)
(MIT) — the Glacial Crown answered in fire: same ZONE cast shape, dice-only
records, and seekable phase machine, with monotonic flame emergence and
burn-out instead of ice overshoot / shatter.

A separate crate (or git submodule) for Ext ports would split the public API,
duplicate aim helpers / `Phase` / `SEEK_STEP` / facade features, and force
consumers to opt into a second edge crate for what is still the same product
surface (skillshot VFX sandbox). Core (`animato-core`, tween, timeline) must
stay lean either way.

## Decision

1. **Ext sandbox ports live in the same optional crate**
   `animato-fx-elemental`. New abilities are additive modules
   (`pyre.rs` / `pyrecrown.rs`, …) + params + tests + cinematic demos —
   no new workspace crate unless a future Ext set clearly needs a different
   dependency / license / GPU surface.
2. **Keep core lean.** No Ext types in `animato-core`. Facade features
   `fx-elemental` / `elemental` re-export Ext abilities alongside the Casting
   set; facade `default` features stay unchanged.
3. **Attribution.** LinearAbilityExtThreeJS is MIT; honor it in
   `ATTRIBUTION.md` + the crate README module mapping (same rules as ADR 0003:
   pipeline structure / timing / defaults only — no binary assets).
4. **Cast shapes.** Follow upstream `ELEMENT_META` (`pyre` → `CastShape.ZONE`,
   label `Pyre Crown`, key `Q`); reuse `solve_zone_aim` / `solve_zone_aim_at`.

## Consequences

- Pyre Crown (and later Ext abilities) ship behind the existing optional crate
  boundary; Casting + Ext share aim, RNG, math, and seek contracts.
- Docs (ADR 0003 follow-ups, README) list Ext ports as in-crate additive work,
  not a new crate proposal.
- A dedicated Ext crate remains reserved for a future case where Ext needs a
  materially different stack — not invented preemptively.

## Follow-up (2026-09-21) — Kraken Crown in-crate

**Kraken Crown (E)** from the same Ext sandbox is now in-crate alongside Pyre
Crown: `kraken.rs` / `krakencrown.rs` + `KrakenCrownParams`, `ELEMENT_META`
`CastShape.ZONE` / label `Kraken Crown` / key `E`, reusing `solve_zone_aim`.
Same attribution and dice-only / seekable contract as Pyre; identity lives in
the *motion* (rolling strike cycles + synchronised finale) rather than a
static bloom material.
