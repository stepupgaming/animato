//! Dump deterministic Kraken Crown tentacle + field/veil samples per frame as JSON.
//!
//! The cinematic demo (`docs/examples/fx-elemental/render_fx_elemental.py`)
//! consumes this dump, so the pictures are proven from the real Rust
//! pipeline — not a Python reimplementation.
//!
//! ```sh
//! cargo run -p animato-fx-elemental --example dump_kraken_crown -- \
//!   docs/examples/fx-elemental/kraken_crown_frames.json
//! ```

use animato_fx_elemental::{KrakenCrown, KrakenCrownParams, TentacleRole};

const SEED: u64 = 7;
const ORIGIN: [f32; 2] = [0.0, 0.0];
const DIRECTION: [f32; 2] = [0.0, 1.0];
const RAW_DISTANCE: f32 = 12.0;
const FPS: usize = 30;

fn f4(v: f32) -> String {
    if !v.is_finite() {
        return String::from("0.0");
    }
    format!("{:.4}", v)
}

fn role_name(r: TentacleRole) -> &'static str {
    match r {
        TentacleRole::Arm => "Arm",
        TentacleRole::Whip => "Whip",
    }
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_kraken_crown <output.json>");
        std::process::exit(2);
    });

    let mut crown = KrakenCrown::with_params(KrakenCrownParams::default());
    crown
        .cast(ORIGIN, DIRECTION, RAW_DISTANCE, SEED)
        .expect("default aim must be valid");
    let total = crown.total_duration();
    let frame_count = ((total * FPS as f32).ceil() as usize).max(2);
    let center = crown.center();
    let zone_r = crown.zone_radius();

    let probe_t = total * 0.45;
    let mut a = crown.clone();
    let mut b = crown.clone();
    a.seek_abs(probe_t);
    b.seek_abs(probe_t);
    assert_eq!(a.front(), b.front(), "seek_abs must be deterministic");
    assert_eq!(a.samples(), b.samples());
    eprintln!(
        "determinism ok: t={:.3}s front={:.3}m phase={:?} arms={}",
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
        f4(crown.length()),
        f4(total),
    ));

    for frame in 0..frame_count {
        let t = total * frame as f32 / (frame_count - 1) as f32;
        crown.seek_abs(t);
        let light = crown.light();
        let fp = crown.front_position();
        let hand = crown.hand_point();
        let phase = format!("{:?}", crown.phase());
        out.push_str(&format!(
            "{{\"t\":{},\"phase\":\"{phase}\",\"front\":{},\"u\":{},\"open\":{},\"close\":{},\"fade\":{},\"emerged\":{},\"light\":{{\"x\":{},\"y\":{},\"z\":{},\"intensity\":{},\"radius\":{}}},\"front_pos\":[{},{}],\"center\":[{},{}],\"hand\":[{},{},{}],",
            f4(t),
            f4(crown.front()),
            f4(crown.u()),
            f4(crown.open()),
            f4(crown.close()),
            f4(crown.crown_fade()),
            crown.emerged_count(),
            f4(light.x),
            f4(light.y),
            f4(light.z),
            f4(light.intensity),
            f4(light.radius),
            f4(fp[0]),
            f4(fp[1]),
            f4(crown.center()[0]),
            f4(crown.center()[1]),
            f4(hand[0]),
            f4(hand[1]),
            f4(hand[2]),
        ));
        if let Some(field) = crown.field() {
            out.push_str(&format!(
                "\"field\":{{\"x\":{},\"z\":{},\"r\":{},\"open\":{},\"fade\":{},\"h\":{},\"quad\":{}}},",
                f4(field.x),
                f4(field.z),
                f4(field.radius),
                f4(field.open),
                f4(field.fade),
                f4(field.height),
                f4(field.quad_size),
            ));
        } else {
            out.push_str("\"field\":null,");
        }
        if let Some(veil) = crown.veil() {
            out.push_str(&format!(
                "\"veil\":{{\"x\":{},\"y\":{},\"z\":{},\"r\":{},\"h\":{},\"opacity\":{},\"spin\":{}}},",
                f4(veil.x),
                f4(veil.y),
                f4(veil.z),
                f4(veil.radius),
                f4(veil.height),
                f4(veil.opacity),
                f4(veil.spin),
            ));
        } else {
            out.push_str("\"veil\":null,");
        }
        out.push_str("\"arms\":[");
        let samples = crown.samples();
        for (si, s) in samples.iter().enumerate() {
            if si > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"role\":\"{}\",\"x\":{},\"z\":{},\"tip\":[{},{},{}],\"len\":{},\"th\":{},\"lean\":{},\"curl\":{},\"wave\":{},\"emerge\":{},\"flash\":{},\"sink\":{},\"bearing\":{},\"striking\":{},\"finale\":{}}}",
                role_name(s.role),
                f4(s.x),
                f4(s.z),
                f4(s.tip_x),
                f4(s.tip_y),
                f4(s.tip_z),
                f4(s.length),
                f4(s.thickness),
                f4(s.lean),
                f4(s.curl),
                f4(s.wave),
                f4(s.emerge),
                f4(s.flash),
                f4(s.sink),
                f4(s.bearing),
                if s.striking { "true" } else { "false" },
                if s.finale { "true" } else { "false" },
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
        "wrote {} frames (total {:.3}s, length {:.2}m, zone_r {:.2}m) to {out_path}",
        frame_count,
        total,
        crown.length(),
        zone_r
    );
}
