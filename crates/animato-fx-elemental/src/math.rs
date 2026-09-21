//! Small maths helpers mirroring `src/utils/math.js` of the original
//! Three.js sandbox. Allocation-free; `libm`-backed so the crate builds for
//! `no_std` too.

/// Clamp `v` into `[a, b]`.
#[inline]
pub fn clamp(v: f32, a: f32, b: f32) -> f32 {
    if v < a {
        a
    } else if v > b {
        b
    } else {
        v
    }
}

/// Clamp `v` into `[0, 1]`.
#[inline]
pub fn saturate(v: f32) -> f32 {
    clamp(v, 0.0, 1.0)
}

/// Linear interpolation `a + (b - a) * t`.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Smoothstep between edges `e0` and `e1` (GLSL semantics).
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let denom = e1 - e0;
    let denom = if denom == 0.0 { 1e-6 } else { denom };
    let t = saturate((x - e0) / denom);
    t * t * (3.0 - 2.0 * t)
}

/// `t * t`.
#[inline]
pub fn in_quad(t: f32) -> f32 {
    t * t
}

/// `t * (2 - t)` — the fracture-front ease off a standstill.
#[inline]
pub fn out_quad(t: f32) -> f32 {
    t * (2.0 - t)
}

/// `t * t * t` — the field-withdrawal curve.
#[inline]
pub fn in_cubic(t: f32) -> f32 {
    t * t * t
}

/// `1 - (1 - t)^5` — the crystal eruption rise.
#[inline]
pub fn out_quint(t: f32) -> f32 {
    1.0 - libm::powf(1.0 - t, 5.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_match_js() {
        assert_eq!(in_quad(0.0), 0.0);
        assert_eq!(in_quad(1.0), 1.0);
        assert_eq!(out_quad(0.0), 0.0);
        assert_eq!(out_quad(1.0), 1.0);
        assert_eq!(in_cubic(1.0), 1.0);
        assert_eq!(out_quint(1.0), 1.0);
        assert!((out_quad(0.5) - 0.75).abs() < 1e-6);
        assert!((out_quint(0.5) - 0.96875).abs() < 1e-5);
    }

    #[test]
    fn smoothstep_matches_glsl() {
        assert_eq!(smoothstep(0.0, 1.0, 0.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 1.0), 1.0);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(smoothstep(1.0, 1.0, 0.5), smoothstep(1.0, 1.0, 0.5));
    }
}
