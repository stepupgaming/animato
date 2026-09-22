//! Dump deterministic Cinder Fall rock / chunks / fissures per frame as JSON.
//!
//! The cinematic demo (`docs/examples/fx-elemental/render_fx_elemental.py`)
//! consumes this dump, so the pictures are proven from the real Rust
//! pipeline — not a Python reimplementation.
//!
//! ```sh
//! cargo run -p animato-fx-elemental --example dump_cinder_fall -- \
//!   docs/examples/fx-elemental/cinder_fall_frames.json
//! ```

use animato_fx_elemental::{CinderFall, CinderFallParams};

const SEED: u64 = 7;
const ORIGIN: [f32; 2] = [0.0, 0.0];
const DIRECTION: [f32; 2] = [0.0, 1.0];
const RAW_DISTANCE: f32 = 12.0;
const FPS: usize = 30;
/// Subsample fissure nodes for a compact dump.
const NODE_STRIDE: usize = 2;

fn f4(v: f32) -> String {
    if !v.is_finite() {
        return String::from("0.0");
    }
    format!("{:.4}", v)
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_cinder_fall <output.json>");
        std::process::exit(2);
    });

    let mut cast = CinderFall::with_params(CinderFallParams::default());
    cast.cast(ORIGIN, DIRECTION, RAW_DISTANCE, SEED)
        .expect("default aim must be valid");
    let total = cast.total_duration();
    let frame_count = ((total * FPS as f32).ceil() as usize).max(2);

    let probe_t = total * 0.35;
    let mut a = cast.clone();
    let mut b = cast.clone();
    a.seek_abs(probe_t);
    b.seek_abs(probe_t);
    assert_eq!(a.front(), b.front(), "seek_abs must be deterministic");
    assert_eq!(a.rock(), b.rock());
    assert_eq!(a.chunks(), b.chunks());
    assert_eq!(a.fissures(), b.fissures());
    eprintln!(
        "determinism ok: t={:.3}s front={:.3}m phase={:?}",
        probe_t,
        a.front(),
        a.phase()
    );

    let mut out = String::with_capacity(1 << 20);
    out.push_str(&format!(
        "{{\"seed\":{SEED},\"origin\":[{:.1},{:.1}],\"length\":{},\"total_duration\":{},\"fps\":{FPS},\"frame_count\":{frame_count},\"frames\":[",
        ORIGIN[0],
        ORIGIN[1],
        f4(cast.length()),
        f4(total),
    ));

    for frame in 0..frame_count {
        let t = total * frame as f32 / (frame_count - 1) as f32;
        cast.seek_abs(t);
        let light = cast.light();
        let rock = cast.rock();
        let fp = cast.front_position();
        let ip = cast.impact_position();
        let hand = cast.hand_point();
        let phase = format!("{:?}", cast.phase());
        out.push_str(&format!(
            "{{\"t\":{},\"phase\":\"{phase}\",\"front\":{},\"u\":{},\"progress\":{},\"charge\":{},\"light\":{{\"x\":{},\"y\":{},\"z\":{},\"intensity\":{},\"radius\":{}}},\"front_pos\":[{},{}],\"impact_pos\":[{},{}],\"hand\":[{},{},{}],\"rock\":{{\"x\":{},\"y\":{},\"z\":{},\"r\":{},\"charge\":{},\"angle\":{},\"visible\":{}}},\"chunks\":[",
            f4(t),
            f4(cast.front()),
            f4(cast.u()),
            f4(cast.progress()),
            f4(cast.charge()),
            f4(light.x),
            f4(light.y),
            f4(light.z),
            f4(light.intensity),
            f4(light.radius),
            f4(fp[0]),
            f4(fp[1]),
            f4(ip[0]),
            f4(ip[1]),
            f4(hand[0]),
            f4(hand[1]),
            f4(hand[2]),
            f4(rock.x),
            f4(rock.y),
            f4(rock.z),
            f4(rock.radius),
            f4(rock.charge),
            f4(rock.angle),
            if rock.visible { "true" } else { "false" },
        ));
        let chunks = cast.chunks();
        for (ci, ch) in chunks.iter().enumerate() {
            if ci > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"x\":{},\"y\":{},\"z\":{},\"r\":{},\"heat\":{},\"angle\":{}}}",
                f4(ch.x),
                f4(ch.y),
                f4(ch.z),
                f4(ch.radius),
                f4(ch.heat),
                f4(ch.angle),
            ));
        }
        out.push_str("],\"fissures\":[");
        let fissures = cast.fissures();
        for (fi, f) in fissures.iter().enumerate() {
            if fi > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"rank\":{},\"nodes\":[", f4(f.rank)));
            let mut first = true;
            for (ni, n) in f.nodes.iter().enumerate() {
                if ni % NODE_STRIDE != 0 && ni + 1 != f.nodes.len() {
                    continue;
                }
                if n.grown <= 0.02 {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!(
                    "{{\"x\":{},\"z\":{},\"d\":{},\"w\":{},\"g\":{}}}",
                    f4(n.x),
                    f4(n.z),
                    f4(n.dist),
                    f4(n.half_width),
                    f4(n.grown),
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
        "wrote {} frames (total {:.3}s, length {:.2}m) to {out_path}",
        frame_count,
        total,
        cast.length(),
    );
}
