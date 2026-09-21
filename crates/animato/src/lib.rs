//! # Animato
//!
//! > *Italian: animato — animated, lively, with life and movement.*
//!
//! A professional-grade, renderer-agnostic animation library for Rust.
//! Zero mandatory dependencies. `no_std`-ready.
//!
//! Works everywhere: TUIs, Web (WASM), Bevy games, embedded targets, and native apps.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use animato::{Tween, Easing, Update};
//!
//! let mut tween = Tween::new(0.0_f32, 100.0)
//!     .duration(1.0)
//!     .easing(Easing::EaseOutCubic)
//!     .build();
//!
//! tween.update(1.0);
//! assert_eq!(tween.value(), 100.0);
//! assert!(tween.is_complete());
//! ```
//!
//! ## Spring Physics
//!
//! ```rust,ignore
//! use animato::{Spring, SpringConfig, Update};
//!
//! let mut spring = Spring::new(SpringConfig::wobbly());
//! spring.set_target(200.0);
//!
//! while !spring.is_settled() {
//!     spring.update(1.0 / 60.0);
//! }
//! assert!((spring.position() - 200.0).abs() < 0.01);
//! ```
//!
//! ## Input Physics
//!
//! ```rust,ignore
//! use animato::{Inertia, InertiaConfig, Update};
//!
//! let mut inertia = Inertia::new(InertiaConfig::smooth());
//! inertia.kick(800.0);
//! while inertia.update(1.0 / 60.0) {}
//! ```
//!
//! ## AnimationDriver
//!
//! ```rust,ignore
//! use animato::{Tween, Easing, AnimationDriver, WallClock, Clock};
//!
//! let mut driver = AnimationDriver::new();
//! let id = driver.add(
//!     Tween::new(0.0_f32, 1.0).duration(2.0).easing(Easing::EaseInOutSine).build()
//! );
//!
//! let mut clock = WallClock::new();
//! // In your loop: driver.tick(clock.delta());
//! ```
//!
//! ## `no_std` Usage
//!
//! For `no_std` targets, depend on the sub-crates directly:
//!
//! ```toml
//! [dependencies]
//! animato-core   = { version = "1.7.2", default-features = false }
//! animato-tween  = { version = "1.7.2", default-features = false }
//! animato-spring = { version = "1.7.2", default-features = false }
//! animato-physics = { version = "1.7.2", default-features = false }
//! animato-color = { version = "1.7.2", default-features = false }
//! ```
//!
//! ## Feature Flags
//!
//! | Feature | What it adds |
//! |---------|-------------|
//! | `default` | `std` + `tween` + `timeline` + `spring` + `driver` |
//! | `std` | Wall clock, heap-backed types |
//! | `tween` | [`Tween<T>`], [`KeyframeTrack<T>`], [`Loop`] |
//! | `timeline` | [`Timeline`], [`Sequence`], [`stagger()`] |
//! | `spring` | [`Spring`], [`SpringConfig`], [`SpringN<T>`] |
//! | `path` | [`MotionPath`], [`MotionPathTween`], [`SvgPathParser`] |
//! | `physics` | [`Inertia`], [`DragState`], [`GestureRecognizer`] |
//! | `color` | [`InLab<T>`], [`InOklch<T>`], [`InLinear<T>`] |
//! | `driver` | [`AnimationDriver`], all [`Clock`] variants |
//! | `gpu` | [`GpuAnimationBatch`] for high-volume `Tween<f32>` batches |
//! | `bevy` | [`AnimatoPlugin`], Bevy tween/spring wrapper components |
//! | `wasm` | [`RafDriver`] for `requestAnimationFrame` loops |
//! | `leptos` | Signal-backed Leptos hooks and components |
//! | `dioxus` | Dioxus signal hooks, motion, presence, gestures, and native helpers |
//! | `yew` | Yew hooks, CSS helpers, scroll, presence, FLIP lists, gestures, and agents |
//! | `js` | WASM-to-NPM JavaScript bindings |
//! | `devtools` | Timeline inspector, easing editor, spring visualizer, recorder controls, perf monitor |
//! | `macro` | Declarative `animato!{}` Motion Macro DSL |
//! | `composition` | [`Composition`], [`Track`], [`Clip`] seekable track/clip composition |
//! | `procgen` | Procedural-geometry edge crate: Delaunay, Voronoi, Lloyd, Poisson-disk, Worley, starter caustics |
//! | `fx-elemental` (`elemental` alias) | Elemental-VFX edge crate: seekable Frost Lance + Storm Lance + Cinder Fall + Nova Beam pipelines |
//! | `tokio` | [`Timeline::wait()`] async completion waiting |
//! | `serde` | `Serialize`/`Deserialize` on all public types |

// ── Core — always present ────────────────────────────────────────────────────
pub use animato_core::{
    Angle, Animatable, AnimationIntrospection, AnimationKind, Color, Easing, Inspectable,
    Interpolate, Mat4, Playable, PlaybackState, Quaternion, Update,
};

// ── Serde convenience re-export ─────────────────────────────────────────────
#[cfg(feature = "serde")]
pub use serde::{Deserialize, Serialize};

/// All free easing functions (`ease_out_cubic`, `cubic_bezier`, etc.) re-exported at crate root.
///
/// These are `#[inline]` free functions — use them when you want zero-overhead
/// easing without the `Easing` enum indirection.
pub mod easing {
    pub use animato_core::easing::*;
}

// ── Tween ────────────────────────────────────────────────────────────────────
#[cfg(feature = "tween")]
pub use animato_tween::{
    GridOrigin, Keyframe, KeyframeTrack, Loop, StaggerPattern, Tween, TweenBuilder, TweenSnapshot,
    TweenState, Waveform, round_to, snap_to,
};

// ── Timeline ────────────────────────────────────────────────────────────────
#[cfg(feature = "timeline")]
pub use animato_timeline::{AnimationGroup, At, Sequence, Timeline, TimelineState, stagger};

// ── Spring ───────────────────────────────────────────────────────────────────
#[cfg(feature = "spring")]
pub use animato_spring::{Integrator, Spring, SpringConfig};

#[cfg(feature = "spring")]
pub use animato_spring::SpringN;

// ── Path ─────────────────────────────────────────────────────────────────────
#[cfg(feature = "path")]
pub use animato_path::{
    CatmullRomSpline, CompoundPath, CubicBezierCurve, DrawSvg, DrawValues, EllipticalArc,
    LineSegment, MorphPath, MotionPath, MotionPathTween, MotionPathTweenBuilder, PathCommand,
    PathEvaluate, PathSegment, PolyPath, QuadBezier, SvgPathError, SvgPathParser, resample,
};

// ── Physics ─────────────────────────────────────────────────────────────────
#[cfg(feature = "physics")]
pub use animato_physics::{
    DragAxis, DragConstraints, DragState, Gesture, GestureConfig, GestureRecognizer, Inertia,
    InertiaBounds, InertiaConfig, InertiaN, PointerData, SwipeDirection,
};

// ── Color ───────────────────────────────────────────────────────────────────
#[cfg(feature = "color")]
pub use animato_color::{InLab, InLinear, InOklch};
#[cfg(feature = "color")]
pub use palette;

// ── Driver ───────────────────────────────────────────────────────────────────
#[cfg(feature = "driver")]
pub use animato_driver::{
    AnimationDriver, AnimationId, AnimationUpdateCost, Clock, DriverFrameProfile, DriverSnapshot,
    ManualClock, MockClock, ScrollClock, ScrollDriver, WallClock,
};

#[cfg(all(feature = "driver", feature = "std"))]
pub use animato_driver::{AnimationRecorder, RecordedSample, RecordedTrack, RecorderError};

// ── GPU ──────────────────────────────────────────────────────────────────────
#[cfg(feature = "gpu")]
pub use animato_gpu::{GpuAnimationBatch, GpuBackend, GpuBatchError};

// ── Bevy ─────────────────────────────────────────────────────────────────────
#[cfg(feature = "bevy")]
pub use animato_bevy::{
    AnimationChannel, AnimationLabel, AnimatoPlugin, AnimatoSet, AnimatoSpring,
    AnimatoSpringPlugin, AnimatoTween, AnimatoTweenPlugin, SpringSettled, TweenCompleted,
};

// ── WASM ─────────────────────────────────────────────────────────────────────
#[cfg(feature = "wasm")]
pub use animato_wasm::{RafDriver, ScrollSmoother};

#[cfg(all(feature = "wasm-dom", target_arch = "wasm32"))]
pub use animato_wasm::{
    Draggable, FlipAnimation, FlipState, LayoutAnimator, Observer, ObserverEvent,
    SharedElementTransition, SplitMode, SplitText,
};

// ── Leptos ──────────────────────────────────────────────────────────────────
#[cfg(feature = "leptos")]
pub mod leptos {
    //! Leptos integration namespace.
    pub use animato_leptos::*;
}

#[cfg(all(feature = "leptos", not(feature = "dioxus")))]
pub use animato_leptos::*;

#[cfg(feature = "dioxus")]
pub mod dioxus {
    //! Dioxus integration namespace.
    pub use animato_dioxus::*;
}

#[cfg(all(feature = "dioxus", not(feature = "leptos")))]
pub use animato_dioxus::*;

// ── Yew ─────────────────────────────────────────────────────────────────────
#[cfg(feature = "yew")]
pub mod yew {
    //! Yew integration namespace.
    pub use animato_yew::*;
}

#[cfg(all(feature = "yew", not(feature = "leptos"), not(feature = "dioxus")))]
pub use animato_yew::*;

// ── JavaScript / NPM ────────────────────────────────────────────────────────
#[cfg(feature = "js")]
pub mod js {
    //! JavaScript/WASM integration namespace.
    pub use animato_js::*;
}

// ── DevTools ────────────────────────────────────────────────────────────────
#[cfg(feature = "devtools")]
pub mod devtools {
    //! DevTools integration namespace.
    pub use animato_devtools::*;
}

#[cfg(feature = "devtools")]
pub use animato_devtools::{
    DevToolsState, EasingCurveEditor, PerformanceMonitor, RecorderControls, SpringVisualizer,
    TimelineInspector,
};

#[cfg(feature = "devtools-web-panel")]
pub use animato_devtools::DevToolsWebPanel;

#[cfg(feature = "devtools-egui-panel")]
pub use animato_devtools::DevToolsEguiPanel;

#[cfg(feature = "devtools-tui-panel")]
pub use animato_devtools::DevToolsTuiPanel;

// ── Motion Macro ──────────────────────────────────────────────────────────────
#[cfg(feature = "macro")]
pub use animato_macro::{animato, keyframes, motion, preset, spring, timeline, tween};

// ── Composition (optional edge crate; defaults unchanged) ────────────────────
#[cfg(feature = "composition")]
pub use animato_composition::{Clip, Composition, Track};

// ── Procgen (optional edge crate; defaults unchanged) ────────────────────────
#[cfg(feature = "procgen")]
pub use animato_procgen::{
    Bounds, CausticField, Point as ProcgenPoint, SmallRng, VoronoiCell, WorleyField,
    lloyd_relax, poisson_disk, polygon_area, polygon_centroid, signed_area2, triangulate,
    voronoi,
};

// ── Elemental FX (optional edge crate; defaults unchanged) ───────────────────
#[cfg(any(feature = "fx-elemental", feature = "elemental"))]
pub use animato_fx_elemental::{
    AimReach, AimSolution, CinderEvent, CinderFall, CinderFallParams, CinderLight,
    ChunkRecord, ChunkSample, FISSURE_STEP, FissureArmRecord, FissureBranchRecord,
    FissureNode, FissureSample, FrostEvent, FrostLance, FrostLanceParams, FrostLight,
    FxRng, IMPACT_FRACTION, MAX_CHUNKS, MAX_COILS, MAX_FISSURE_ARMS, MAX_FISSURE_BRANCHES,
    MAX_RINGS, MAX_SPIKES, MAX_STRANDS, NovaBeam, NovaBeamParams, NovaEvent, NovaLight,
    OrbSample, Phase as FrostPhase, RingRecord, RingSample, RockSample, SEEK_STEP,
    STRAND_NODES, SpawnError as FrostSpawnError, SpikeRecord, SpikeSample, StormEvent,
    StormLance, StormLanceParams, StormLight, StrandNode, StrandRecord, StrandSample,
    TUBE_SEGMENTS, TubeNode, arc_point, beam_axis_point, beam_radius, heading_at,
    roll_chunks, roll_fissures, roll_rings, roll_spikes, roll_strands, sample_chunk,
    sample_fissures, sample_ring, solve_aim,
};

/// Prelude module with macro-friendly re-exports.
///
/// Import everything for ergonomic macro usage:
///
/// ```ignore
/// use animato::prelude::*;
///
/// let intro = animato! {
///     sequence {
///         tween opacity: 0.0 => 1.0, duration: 0.35, easing: ease_out_cubic;
///         spring scale: 0.92 => 1.0, preset: snappy;
///     }
/// };
/// ```
pub mod prelude {
    #[cfg(feature = "tween")]
    pub use crate::{
        GridOrigin, Keyframe, KeyframeTrack, Loop, StaggerPattern, Tween, TweenBuilder, TweenState,
        Waveform, round_to, snap_to,
    };

    #[cfg(feature = "timeline")]
    pub use crate::{AnimationGroup, At, Sequence, Timeline, TimelineState, stagger};

    #[cfg(feature = "spring")]
    pub use crate::{Integrator, Spring, SpringConfig, SpringN};

    #[cfg(feature = "path")]
    pub use crate::{
        CatmullRomSpline, CompoundPath, CubicBezierCurve, DrawSvg, DrawValues, EllipticalArc,
        LineSegment, MorphPath, MotionPath, MotionPathTween, MotionPathTweenBuilder, PathCommand,
        PathEvaluate, PathSegment, PolyPath, QuadBezier, SvgPathError, SvgPathParser, resample,
    };

    #[cfg(feature = "physics")]
    pub use crate::{
        DragAxis, DragConstraints, DragState, Gesture, GestureConfig, GestureRecognizer, Inertia,
        InertiaBounds, InertiaConfig, InertiaN, PointerData, SwipeDirection,
    };

    #[cfg(feature = "color")]
    pub use crate::{InLab, InLinear, InOklch};

    #[cfg(feature = "driver")]
    pub use crate::{
        AnimationDriver, AnimationId, Clock, ManualClock, MockClock, ScrollClock, ScrollDriver,
        WallClock,
    };

    #[cfg(feature = "macro")]
    pub use crate::{animato, keyframes, motion, preset, spring, timeline, tween};

    #[cfg(feature = "composition")]
    pub use crate::{Clip, Composition, Track};

    #[cfg(feature = "procgen")]
    pub use crate::{
        Bounds, CausticField, ProcgenPoint, SmallRng, VoronoiCell, WorleyField, lloyd_relax,
        poisson_disk, polygon_area, polygon_centroid, signed_area2, triangulate, voronoi,
    };

    #[cfg(any(feature = "fx-elemental", feature = "elemental"))]
    pub use crate::{
        AimReach, AimSolution, CinderEvent, CinderFall, CinderFallParams, CinderLight,
        ChunkRecord, ChunkSample, FISSURE_STEP, FissureArmRecord, FissureBranchRecord,
        FissureNode, FissureSample, FrostEvent, FrostLance, FrostLanceParams, FrostLight,
        FrostPhase, FrostSpawnError, FxRng, IMPACT_FRACTION, MAX_CHUNKS, MAX_COILS,
        MAX_FISSURE_ARMS, MAX_FISSURE_BRANCHES, MAX_RINGS, MAX_SPIKES, MAX_STRANDS,
        NovaBeam, NovaBeamParams, NovaEvent, NovaLight, OrbSample, RingRecord, RingSample,
        RockSample, SEEK_STEP, STRAND_NODES, SpikeRecord, SpikeSample, StormEvent,
        StormLance, StormLanceParams, StormLight, StrandNode, StrandRecord, StrandSample,
        TUBE_SEGMENTS, TubeNode, arc_point, beam_axis_point, beam_radius, heading_at,
        roll_chunks, roll_fissures, roll_rings, roll_spikes, roll_strands, sample_chunk,
        sample_fissures, sample_ring, solve_aim,
    };
}
