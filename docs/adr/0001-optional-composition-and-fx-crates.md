# ADR 0001: Optional Composition and FX Crates

- Status: Accepted
- Date: 2026-09-21
- Branch: `feat/issue-1-composition-elemental-sandbox` (fork: `stepupgaming/animato`)

## Context

`animato` core crates (`animato-core`, `animato-tween`, `animato-timeline`,
`animato-spring`, `animato-path`, `animato-physics`, `animato-color`,
`animato-driver`) are lean, renderer-agnostic, and `no_std`-ready. There is a
proposal to add higher-level composition (seekable track/clip timelines for
video-style editing) and a future Elemental/FX sandbox port (Three.js-style
effects, GPU-heavy scenes).

These higher-level concerns must not bloat the core or destabilize its public
APIs.

## Decision

1. **Core crates stay lean.** No new required dependencies, no breaking public
   API changes to existing crates. Additive-only changes are allowed (optional
   facade feature flags / re-exports on the `animato` umbrella crate).
2. **Composition / video / FX are optional edge crates.** New functionality
   lives in new workspace crates behind opt-in cargo features:
   - `animato-composition` (this phase): seekable track/clip composition model
     that drives the existing `animato-timeline` `Timeline` / `seek_abs`
     public API. No reimplementation of timeline looping/seeking semantics.
   - Future `video` / `fx` crates (out of scope for Phase 1): Elemental
     sandbox port, Three.js-style effects. They must follow the same pattern —
     new crates, optional facade features, defaults unchanged.
3. **Defaults never change.** The `animato` facade `default` feature set is
   untouched by edge crates. Consumers opt in explicitly, e.g.
   `animato = { version = "...", features = ["composition"] }`.
4. **PRs stay on this fork.** Remote `origin` is `https://github.com/stepupgaming/animato`
   (our fork). Never push to or open PRs against upstream `AarambhDevHub`.
   All Phase 1+ work happens on `feat/issue-1-composition-elemental-sandbox`
   (or follow-up branches forked from it) until explicitly requested otherwise.

## Consequences

- `animato-composition` depends on `animato-core`, `animato-tween`, and
  `animato-timeline`; it delegates playback/seeking to `Timeline`
  (`play`, `update`, `seek`, `seek_abs`, `progress`, `duration`), so timeline
  semantics stay in one place.
- The `animato` facade gains an optional `composition` feature that re-exports
  `animato-composition`. No core crate public API changes besides this additive
  re-export.
- Future FX/video work adds new crates + new optional features using this ADR
  as the template. No Three.js / Elemental Sandbox port happens in Phase 1.
