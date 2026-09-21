//! Dump deterministic Voltaic Snare leash + cage polylines per frame as JSON.
//!
//! The cinematic demo (`docs/examples/fx-elemental/render_fx_elemental.py`)
//! consumes this dump, so the pictures are proven from the real Rust
//! pipeline — not a Python reimplementation.
//!
//! ```sh
//! cargo run -p animato-fx-elemental --example dump_voltaic_snare -- \
//!   docs/examples/fx-elemental/voltaic_snare_frames.json
//! ```

use animato_fx_elemental::{FilamentRole, VoltaicSnare, VoltaicSnareParams};

const SEED: u64 = 7;
const ORIGIN: [f32; 2] = [0.0, 0.0];
const DIRECTION: [f32; 2] = [0.0, 1.0];
const RAW_DISTANCE: f32 = 12.0;
const FPS: usize = 30;
const NODE_STRIDE: usize = 2;

fn f4(v: f32) -> String {
    if !v.is_finite() {
        return String::from("0.0");
    }
    format!("{:.4}", v)
}

fn role_name(r: FilamentRole) -> &'static str {
    match r {
        FilamentRole::Leash => "Leash",
        FilamentRole::Column => "Column",
        FilamentRole::Tendril => "Tendril",
        FilamentRole::Rim => "Rim",
    }
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_voltaic_snare <output.json>");
        std::process::exit(2);
    });

    let mut snare = VoltaicSnare::with_params(VoltaicSnareParams::default());
    snare
        .cast(ORIGIN, DIRECTION, RAW_DISTANCE, SEED)
        .expect("default aim must be valid");
    let total = snare.total_duration();
    let frame_count = ((total * FPS as f32).ceil() as usize).max(2);
    let center = snare.center();
    let zone_r = snare.zone_radius();

    let probe_t = total * 0.45;
    let mut a = snare.clone();
    let mut b = snare.clone();
    a.seek_abs(probe_t);
    b.seek_abs(probe_t);
    assert_eq!(a.front(), b.front(), "seek_abs must be deterministic");
    assert_eq!(a.samples(), b.samples());
    eprintln!(
        "determinism ok: t={:.3}s front={:.3}m phase={:?} filaments={}",
        probe_t,
        a.front(),
        a.phase(),
        a.samples().len()
    );

    let mut out = String::with_capacity(1 << 20);
    out.push_str(&format!(
        "{{\"seed\":{SEED},\"origin\":[{:.1},{:.1}],\"center\":[{},{}],\"zone_radius\":{},\"length\":{},\"total_duration\":{},\"fps\":{FPS},\"frame_count\":{frame_count},\"frames\":[",
        ORIGIN[0],
        ORIGIN[1],
        f4(center[0]),
        f4(center[1]),
        f4(zone_r),
        f4(snare.length()),
        f4(total),
    ));

    for frame in 0..frame_count {
        let t = total * frame as f32 / (frame_count - 1) as f32;
        snare.seek_abs(t);
        let light = snare.light();
        let fp = snare.front_position();
        let hand = snare.hand_point();
        let phase = format!("{:?}", snare.phase());
        out.push_str(&format!(
            "{{\"t\":{},\"phase\":\"{phase}\",\"front\":{},\"u\":{},\"open\":{},\"climb\":{},\"fade\":{},\"light\":{{\"x\":{},\"y\":{},\"z\":{},\"intensity\":{},\"radius\":{}}},\"front_pos\":[{},{}],\"center\":[{},{}],\"hand\":[{},{},{}],\"filaments\":[",
            f4(t),
            f4(snare.front()),
            f4(snare.u()),
            f4(snare.open()),
            f4(snare.climb()),
            f4(snare.cage_fade()),
            f4(light.x),
            f4(light.y),
            f4(light.z),
            f4(light.intensity),
            f4(light.radius),
            f4(fp[0]),
            f4(fp[1]),
            f4(snare.center()[0]),
            f4(snare.center()[1]),
            f4(hand[0]),
            f4(hand[1]),
            f4(hand[2]),
        ));
        let samples = snare.samples();
        for (si, s) in samples.iter().enumerate() {
            if si > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"role\":\"{}\",\"i\":{},\"fan\":{},\"flash\":{},\"dim\":{},\"nodes\":[",
                role_name(s.role),
                s.index,
                f4(s.fan),
                f4(s.flash),
                f4(s.dim),
            ));
            let mut first = true;
            for (ni, n) in s.nodes.iter().enumerate() {
                if ni % NODE_STRIDE != 0 && ni + 1 != s.nodes.len() {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!(
                    "{{\"x\":{},\"y\":{},\"z\":{},\"t\":{},\"w\":{}}}",
                    f4(n.x),
                    f4(n.y),
                    f4(n.z),
                    f4(n.t),
                    f4(n.half_width),
                ));
            }
            out.push_str("]}");
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
        "wrote {} frames (total {:.3}s, length {:.2}m, zone_r {:.2}m) to {out_path}",
        frame_count,
        total,
        snare.length(),
        zone_r
    );
}
