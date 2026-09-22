//! # animato-composition
//!
//! Seekable track/clip composition over [`Timeline`](animato_timeline::Timeline).
//!
//! This crate is an **optional edge crate** (see
//! `docs/adr/0001-optional-composition-and-fx-crates.md`). It adds no new
//! playback engine: [`Composition`] owns a `Timeline` and delegates all
//! playback and seeking to the existing public API (`play`, `update`,
//! [`Timeline::seek`], [`Timeline::seek_abs`], `progress`, `duration`).
//!
//! ## Quick Start
//!
//! ```rust
//! use animato_composition::Composition;
//! use animato_core::{Easing, Update};
//! use animato_tween::Tween;
//!
//! let fade = Tween::new(0.0_f32, 1.0)
//!     .duration(1.0)
//!     .easing(Easing::EaseOutCubic)
//!     .build();
//!
//! let mut comp = Composition::new();
//! comp.add("video", "fade", fade, 0.25);
//!
//! comp.seek_abs(0.75);
//! assert!(comp.progress() > 0.0);
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use animato_core::{Playable, Update};
use animato_timeline::{At, Timeline, TimelineState};

/// A named segment on a [`Track`].
///
/// `start` is the absolute composition time in seconds at which the clip
/// begins; `duration` is cached from the source animation at insert time.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    label: String,
    start: f32,
    duration: f32,
}

impl Clip {
    /// Create a clip descriptor.
    ///
    /// Negative starts clamp to `0.0`; negative durations clamp to `0.0`.
    pub fn new(label: impl Into<String>, start: f32, duration: f32) -> Self {
        Self {
            label: label.into(),
            start: start.max(0.0),
            duration: duration.max(0.0),
        }
    }

    /// Clip label (matches the underlying [`Timeline`] entry label).
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Absolute start time in seconds.
    pub fn start(&self) -> f32 {
        self.start
    }

    /// Clip duration in seconds.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Absolute end time in seconds (`start + duration`).
    pub fn end(&self) -> f32 {
        self.start + self.duration
    }

    /// `true` when `time` falls inside `[start, end)`.
    pub fn contains(&self, time: f32) -> bool {
        time >= self.start && time < self.end()
    }
}

/// A named lane of non-overlapping-or-not [`Clip`] descriptors.
///
/// Tracks are organizational only; playback is driven by the parent
/// [`Composition`]'s [`Timeline`]. Overlapping clips on one track are allowed
/// at the model level (mirroring timeline entries) and resolve by insertion
/// order in [`Track::clip_at`].
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    name: String,
    clips: Vec<Clip>,
}

impl Track {
    /// Create an empty track.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
        }
    }

    /// Track name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// All clips on this track, in insertion order.
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// Push a clip descriptor, returning a reference to it.
    pub fn push_clip(&mut self, clip: Clip) -> &Clip {
        self.clips.push(clip);
        // Clip lookup by index avoids a second label search.
        &self.clips[self.clips.len() - 1]
    }

    /// Base duration: the latest clip end, or `0.0` when empty.
    pub fn duration(&self) -> f32 {
        self.clips.iter().map(Clip::end).fold(0.0, f32::max)
    }

    /// First clip containing `time`, or `None`.
    pub fn clip_at(&self, time: f32) -> Option<&Clip> {
        self.clips.iter().find(|clip| clip.contains(time))
    }
}

/// Seekable multi-track composition backed by a [`Timeline`].
///
/// Clips added via [`Composition::add`] are inserted into the inner timeline
/// at their absolute start (`At::Absolute`) and recorded as [`Clip`]
/// descriptors on the named [`Track`]. All transport — `play`, `pause`,
/// `resume`, `reset`, `update`, `seek`, `seek_abs` — delegates to the inner
/// timeline, so looping/seeking semantics live in exactly one place.
#[derive(Debug)]
pub struct Composition {
    tracks: Vec<Track>,
    timeline: Timeline,
}

impl Default for Composition {
    fn default() -> Self {
        Self::new()
    }
}

impl Composition {
    /// Create an empty composition.
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            timeline: Timeline::new(),
        }
    }

    /// Add an animation as a clip on `track` starting at absolute `start`.
    ///
    /// Creates the track on first use. Returns the recorded [`Clip`].
    /// Negative starts clamp to `0.0` (matching [`At::Absolute`] handling).
    pub fn add<A>(
        &mut self,
        track: impl Into<String>,
        label: impl Into<String>,
        animation: A,
        start: f32,
    ) -> &Clip
    where
        A: Playable + Send + 'static,
    {
        let label: String = label.into();
        let start = start.max(0.0);
        let duration = animation.duration().max(0.0);
        // Timeline::add takes `self` by value, so swap the inner timeline out.
        let timeline = core::mem::replace(&mut self.timeline, Timeline::new());
        self.timeline = timeline.add(label.clone(), animation, At::Absolute(start));
        let track_name: String = track.into();
        let index = match self.tracks.iter().position(|t| t.name == track_name) {
            Some(index) => index,
            None => {
                self.tracks.push(Track::new(track_name));
                self.tracks.len() - 1
            }
        };
        self.tracks[index].push_clip(Clip::new(label, start, duration))
    }

    /// Begin playback.
    pub fn play(&mut self) {
        self.timeline.play();
    }

    /// Pause playback.
    pub fn pause(&mut self) {
        self.timeline.pause();
    }

    /// Resume playback after a pause.
    pub fn resume(&mut self) {
        self.timeline.resume();
    }

    /// Reset the composition and all children to the beginning.
    pub fn reset(&mut self) {
        self.timeline.reset();
    }

    /// Seek by normalized progress through the composition.
    pub fn seek(&mut self, progress: f32) {
        self.timeline.seek(progress);
    }

    /// Seek to an absolute time in seconds; synchronizes all children.
    pub fn seek_abs(&mut self, secs: f32) {
        self.timeline.seek_abs(secs);
    }

    /// Advance playback by `dt` seconds; returns `false` when complete.
    pub fn update(&mut self, dt: f32) -> bool {
        use animato_core::Update as _;
        self.timeline.update(dt)
    }

    /// Base duration in seconds (last finishing clip across all tracks).
    pub fn duration(&self) -> f32 {
        self.timeline.duration()
    }

    /// Current normalized progress through finite playback.
    pub fn progress(&self) -> f32 {
        self.timeline.progress()
    }

    /// Current total elapsed composition time in seconds.
    pub fn elapsed(&self) -> f32 {
        self.timeline.elapsed()
    }

    /// `true` when the composition has finished all finite playback.
    pub fn is_complete(&self) -> bool {
        self.timeline.is_complete()
    }

    /// Current timeline state of the inner timeline.
    pub fn state(&self) -> TimelineState {
        self.timeline.state()
    }

    /// Number of clips across all tracks.
    pub fn clip_count(&self) -> usize {
        self.tracks.iter().map(|t| t.clips().len()).sum()
    }

    /// All tracks, in creation order.
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Find a track by name.
    pub fn track(&self, name: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.name() == name)
    }

    /// First clip on `track` containing absolute `time`.
    pub fn clip_at(&self, track: &str, time: f32) -> Option<&Clip> {
        self.track(track).and_then(|t| t.clip_at(time))
    }

    /// Find a child animation by label and concrete type.
    pub fn get<T>(&self, label: &str) -> Option<&T>
    where
        T: Playable + 'static,
    {
        self.timeline.get::<T>(label)
    }

    /// Find a mutable child animation by label and concrete type.
    pub fn get_mut<T>(&mut self, label: &str) -> Option<&mut T>
    where
        T: Playable + 'static,
    {
        self.timeline.get_mut::<T>(label)
    }
}

impl Update for Composition {
    fn update(&mut self, dt: f32) -> bool {
        Composition::update(self, dt)
    }
}

impl Playable for Composition {
    fn duration(&self) -> f32 {
        Composition::duration(self)
    }

    fn reset(&mut self) {
        Composition::reset(self);
    }

    fn seek_to(&mut self, progress: f32) {
        Composition::seek(self, progress);
    }

    fn is_complete(&self) -> bool {
        Composition::is_complete(self)
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use animato_core::Easing;
    use animato_tween::Tween;

    fn tween(end: f32, duration: f32) -> Tween<f32> {
        Tween::new(0.0_f32, end)
            .duration(duration)
            .easing(Easing::Linear)
            .build()
    }

    #[test]
    fn add_drives_timeline_duration() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(1.0, 1.0), 0.0);
        comp.add("video", "b", tween(1.0, 0.5), 1.0);

        assert_eq!(comp.duration(), 1.5);
        assert_eq!(comp.clip_count(), 2);
        assert_eq!(comp.track("video").unwrap().duration(), 1.5);
    }

    #[test]
    fn seek_abs_synchronizes_children() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(100.0, 2.0), 0.0);

        comp.seek_abs(0.5);

        assert_eq!(comp.get::<Tween<f32>>("a").unwrap().value(), 25.0);
        assert_eq!(comp.elapsed(), 0.5);
    }

    #[test]
    fn seek_abs_respects_clip_offset() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(100.0, 1.0), 1.0);

        comp.seek_abs(1.5);

        assert_eq!(comp.get::<Tween<f32>>("a").unwrap().value(), 50.0);
    }

    #[test]
    fn clip_at_resolves_track_clip() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(1.0, 1.0), 0.0);
        comp.add("audio", "b", tween(1.0, 1.0), 0.5);

        assert_eq!(comp.clip_at("video", 0.5).unwrap().label(), "a");
        assert_eq!(comp.clip_at("audio", 0.75).unwrap().label(), "b");
        assert!(comp.clip_at("video", 1.5).is_none());
        assert!(comp.clip_at("missing", 0.0).is_none());
    }

    #[test]
    fn update_advances_and_reports_progress() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(100.0, 1.0), 0.0);

        comp.play();
        assert!(comp.update(0.5));
        assert_eq!(comp.get::<Tween<f32>>("a").unwrap().value(), 50.0);
        assert!((comp.progress() - 0.5).abs() < 0.001);

        assert!(!comp.update(0.5));
        assert!(comp.is_complete());
    }

    #[test]
    fn reset_returns_to_start() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(100.0, 1.0), 0.0);

        comp.play();
        comp.update(0.5);
        comp.reset();

        assert_eq!(comp.elapsed(), 0.0);
        assert_eq!(comp.get::<Tween<f32>>("a").unwrap().value(), 0.0);
    }

    #[test]
    fn missing_track_is_created_on_first_add() {
        let mut comp = Composition::new();
        assert!(comp.track("video").is_none());

        comp.add("video", "a", tween(1.0, 1.0), 0.0);

        assert_eq!(comp.tracks().len(), 1);
        assert_eq!(comp.track("video").unwrap().clips().len(), 1);
    }

    #[test]
    fn playable_trait_delegates_to_timeline() {
        let mut comp = Composition::new();
        comp.add("video", "a", tween(100.0, 1.0), 0.0);

        assert_eq!(Playable::duration(&comp), 1.0);
        Playable::seek_to(&mut comp, 0.5);
        assert_eq!(comp.get::<Tween<f32>>("a").unwrap().value(), 50.0);
        assert!(Playable::as_any(&comp).is::<Composition>());
        assert!(Playable::as_any_mut(&mut comp).is::<Composition>());
        Playable::reset(&mut comp);
        assert_eq!(comp.elapsed(), 0.0);
    }
}
