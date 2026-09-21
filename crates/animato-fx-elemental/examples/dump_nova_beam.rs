//! Dump deterministic Nova Beam tube + rings + orb per frame as JSON.
//!
//! The cinematic demo (`docs/examples/fx-elemental/render_fx_elemental.py`)
//! consumes this dump, so the pictures are proven from the real Rust
//! pipeline — not a Python reimplementation.
//!
//! ```sh
//! cargo run -p animato-fx-elemental --example dump_nova_beam -- \
//!   docs/examples/fx-elemental/nova_beam_frames.json
//! ```

use animato_fx_elemental::{NovaBeam, NovaBeamParams};

const SEED: u64 = 7;
const ORIGIN: [f32; 2] = [0.0, 0.0];
const DIRECTION: [f32; 2] = [0.0, 1.0];
const RAW_DISTANCE: f32 = 12.0;
const FPS: usize = 30;
/// Subsample tube nodes for a compact dump (full resolve uses 33).
const NODE_STRIDE: usize = 1;

fn f4(v: f32) -> String {
    if !v.is_finite() {
        return String::from("0.0");
    }
    format!("{:.4}", v)
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_nova_beam <output.json>");
        std::process::exit(2);
    });

    let mut beam = NovaBeam::with_params(NovaBeamParams::default());
    beam
        .cast(ORIGIN, DIRECTION, RAW_DISTANCE, SEED)
        .expect("default aim must be valid");
    let total = beam.total_duration();
    let frame_count = ((total * FPS as f32).ceil() as usize).max(2);

    let probe_t = total * 0.4;
    let mut a = beam.clone();
    let mut b = beam.clone();
    a.seek_abs(probe_t);
    b.seek_abs(probe_t);
    assert_eq!(a.front(), b.front(), "seek_abs must be deterministic");
    assert_eq!(a.tube(), b.tube());
    assert_eq!(a.rings(), b.rings());
    eprintln!(
        "determinism ok: t={:.3}s front={:.3}m phase={:?} rings={}",
        probe_t,
        a.front(),
        a.phase(),
        a.ring_records().len()
    );

    let mut out = String::with_capacity(1 << 20);
    out.push_str(&format!(
        "{{\"seed\":{SEED},\"origin\":[{:.1},{:.1}],\"length\":{},\"total_duration\":{},\"fps\":{FPS},\"frame_count\":{frame_count},\"frames\":[",
        ORIGIN[0],
        ORIGIN[1],
        f4(beam.length()),
        f4(total),
    ));

    for frame in 0..frame_count {
        let t = total * frame as f32 / (frame_count - 1) as f32;
        beam.seek_abs(t);
        let light = beam.light();
        let muzzle = beam.muzzle_light();
        let fp = beam.front_position();
        let ip = beam.impact_position();
        let hand = beam.hand_point();
        let orb = beam.orb();
        let phase = format!("{:?}", beam.phase());
        out.push_str(&format!(
            "{{\"t\":{},\"phase\":\"{phase}\",\"front\":{},\"u\":{},\"progress\":{},\"fade\":{},\"width_fade\":{},\"charge\":{},\"light\":{{\"x\":{},\"y\":{},\"z\":{},\"intensity\":{},\"radius\":{}}},\"muzzle\":{{\"x\":{},\"y\":{},\"z\":{},\"intensity\":{},\"radius\":{}}},\"front_pos\":[{},{}],\"impact_pos\":[{},{}],\"hand\":[{},{},{}],\"orb\":{{\"x\":{},\"y\":{},\"z\":{},\"radius\":{},\"charge\":{},\"visible\":{}}},\"tube\":[",
            f4(t),
            f4(beam.front()),
            f4(beam.u()),
            f4(beam.progress()),
            f4(beam.beam_fade()),
            f4(beam.width_fade()),
            f4(beam.charge()),
            f4(light.x),
            f4(light.y),
            f4(light.z),
            f4(light.intensity),
            f4(light.radius),
            f4(muzzle.x),
            f4(muzzle.y),
            f4(muzzle.z),
            f4(muzzle.intensity),
            f4(muzzle.radius),
            f4(fp[0]),
            f4(fp[1]),
            f4(ip[0]),
            f4(ip[1]),
            f4(hand[0]),
            f4(hand[1]),
            f4(hand[2]),
            f4(orb.x),
            f4(orb.y),
            f4(orb.z),
            f4(orb.radius),
            f4(orb.charge),
            if orb.visible { "true" } else { "false" },
        ));
        let tube = beam.tube();
        let mut first = true;
        for (ni, n) in tube.iter().enumerate() {
            if ni % NODE_STRIDE != 0 && ni + 1 != tube.len() {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"s\":{},\"x\":{},\"y\":{},\"z\":{},\"r\":{},\"d\":{}}}",
                f4(n.s),
                f4(n.x),
                f4(n.y),
                f4(n.z),
                f4(n.radius),
                f4(n.drawn),
            ));
        }
        out.push_str("],\"rings\":[");
        let rings = beam.rings();
        for (ri, r) in rings.iter().enumerate() {
            if ri > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"s\":{},\"x\":{},\"y\":{},\"z\":{},\"inner\":{},\"outer\":{},\"a\":{}}}",
                f4(r.s),
                f4(r.x),
                f4(r.y),
                f4(r.z),
                f4(r.inner),
                f4(r.outer),
                f4(r.alpha),
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
        "wrote {} frames (total {:.3}s, length {:.2}m, {} rings) to {out_path}",
        frame_count,
        total,
        beam.length(),
        beam.ring_records().len()
    );
}
