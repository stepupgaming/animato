//! Dump deterministic Frost Lance front + spike samples per frame as JSON.
//!
//! The cinematic demo (`docs/examples/fx-elemental/render_fx_elemental.py`)
//! consumes this dump, so the pictures are proven from the real Rust
//! pipeline — not a Python reimplementation.
//!
//! ```sh
//! cargo run -p animato-fx-elemental --example dump_frost_lance -- \
//!   docs/examples/fx-elemental/frost_lance_frames.json
//! ```
//!
//! Output schema (floats rounded to 4 decimals):
//! ```text
//! {
//!   "seed": 7, "origin": [x, z], "length": m, "total_duration": s,
//!   "fps": 30, "frame_count": N,
//!   "frames": [
//!     {"t": s, "phase": "Travel", "front": m, "u": 0..1,
//!      "erupted": n, "breaching": n,
//!      "light": {"x": x, "z": z, "intensity": i, "radius": r},
//!      "front_pos": [x, z], "impact_pos": [x, z],
//!      "spikes": [{"x": x, "z": z, "h": h, "r": r,
//!                  "e": emergence, "b": birth, "y": y_base}]}
//!   ]
//! }
//! ```

use animato_fx_elemental::{FrostLance, FrostLanceParams};

const SEED: u64 = 7;
const ORIGIN: [f32; 2] = [0.0, 0.0];
const DIRECTION: [f32; 2] = [0.0, 1.0];
const RAW_DISTANCE: f32 = 12.0;
const FPS: usize = 30;

fn f4(v: f32) -> String {
    if !v.is_finite() {
        return String::from("0.0");
    }
    // Round to 4 decimals to keep the dump compact; the viz only needs
    // frame-accurate placement, not f32-exact fidelity.
    format!("{:.4}", v)
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_frost_lance <output.json>");
        std::process::exit(2);
    });

    let mut lance = FrostLance::with_params(FrostLanceParams::default());
    lance
        .cast(ORIGIN, DIRECTION, RAW_DISTANCE, SEED)
        .expect("default aim must be valid");
    let total = lance.total_duration();
    let frame_count = ((total * FPS as f32).ceil() as usize).max(2);

    // Determinism proof: same time twice ⇒ same front + same samples.
    let probe_t = total * 0.4;
    let mut a = lance.clone();
    let mut b = lance.clone();
    a.seek_abs(probe_t);
    b.seek_abs(probe_t);
    assert_eq!(a.front(), b.front(), "seek_abs must be deterministic");
    assert_eq!(a.erupted_count(), b.erupted_count());
    assert_eq!(a.samples(), b.samples());
    eprintln!(
        "determinism ok: t={:.3}s front={:.3}m erupted={}",
        probe_t,
        a.front(),
        a.erupted_count()
    );

    let mut out = String::with_capacity(1 << 20);
    out.push_str(&format!(
        "{{\"seed\":{SEED},\"origin\":[{:.1},{:.1}],\"length\":{},\"total_duration\":{},\"fps\":{FPS},\"frame_count\":{frame_count},\"frames\":[",
        ORIGIN[0],
        ORIGIN[1],
        f4(lance.length()),
        f4(total),
    ));

    for frame in 0..frame_count {
        let t = total * frame as f32 / (frame_count - 1) as f32;
        lance.seek_abs(t);
        let light = lance.light();
        let fp = lance.front_position();
        let ip = lance.impact_position();
        let phase = format!("{:?}", lance.phase());
        out.push_str(&format!(
            "{{\"t\":{},\"phase\":\"{phase}\",\"front\":{},\"u\":{},\"erupted\":{},\"breaching\":{},\"light\":{{\"x\":{},\"z\":{},\"intensity\":{},\"radius\":{}}},\"front_pos\":[{},{}],\"impact_pos\":[{},{}],\"spikes\":[",
            f4(t),
            f4(lance.front()),
            f4(lance.u()),
            lance.erupted_count(),
            lance.breaching_count(),
            f4(light.x),
            f4(light.z),
            f4(light.intensity),
            f4(light.radius),
            f4(fp[0]),
            f4(fp[1]),
            f4(ip[0]),
            f4(ip[1]),
        ));
        let samples = lance.samples();
        for (i, s) in samples.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"x\":{},\"z\":{},\"h\":{},\"r\":{},\"e\":{},\"b\":{},\"y\":{}}}",
                f4(s.x),
                f4(s.z),
                f4(s.height),
                f4(s.radius),
                f4(s.emergence),
                f4(s.birth),
                f4(s.y_base),
            ));
        }
        out.push_str("]}");
        if frame + 1 < frame_count {
            out.push(',');
        }
    }
    out.push_str("]}");
    std::fs::write(&out_path, &out).unwrap_or_else(|e| {
        eprintln!("failed to write {out_path}: {e}");
        std::process::exit(1);
    });
    eprintln!(
        "wrote {} frames (total {:.3}s, length {:.2}m, {} spikes) to {out_path}",
        frame_count,
        total,
        lance.length(),
        lance.spikes().len()
    );
}
