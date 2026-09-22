//! Shared 2D point type for `animato-procgen`.

use core::ops::{Add, Div, Mul, Sub};

/// A 2D point in single precision.
///
/// All procgen geometry works in plain `f32` model space; mapping to pixels,
/// UVs, or world units is left to the caller / renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

impl Point {
    /// Create a point.
    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Squared Euclidean distance to `other`.
    #[inline]
    pub fn dist2(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    /// Euclidean distance to `other`.
    #[inline]
    pub fn distance(self, other: Self) -> f32 {
        libm::sqrtf(self.dist2(other))
    }

    /// Dot product.
    #[inline]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }
}

impl Add for Point {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Point {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}

impl Div<f32> for Point {
    type Output = Self;
    #[inline]
    fn div(self, s: f32) -> Self {
        Self::new(self.x / s, self.y / s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_ops() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert_eq!(a.distance(b), 5.0);
        assert_eq!((a + b), Point::new(3.0, 4.0));
        assert_eq!((b - a), b);
        assert_eq!((b * 2.0), Point::new(6.0, 8.0));
    }
}
