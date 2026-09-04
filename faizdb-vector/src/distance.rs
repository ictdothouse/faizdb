//! Distance metrics for Vector Search (AI-Native embeddings).
//!
//! Supports:
//! - Cosine similarity (1.0 = identical, -1.0 = opposite)
//! - Cosine distance (1.0 - cosine_similarity, 0.0 = identical)
//! - Euclidean (L2) distance (0.0 = identical)
//! - Squared Euclidean distance (faster, avoids sqrt)
//! - Dot product (higher = more similar if normalized)
//! - Manhattan (L1) distance

use serde::{Deserialize, Serialize};

/// Distance metric used for vector similarity comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DistanceMetric {
    /// Cosine distance: 1.0 - (A · B) / (||A|| * ||B||)
    /// Range: [0.0, 2.0], where 0.0 means identical angle.
    #[default]
    Cosine,
    /// Euclidean (L2) distance: sqrt(sum((A_i - B_i)^2))
    /// Range: [0.0, +inf], where 0.0 means identical.
    Euclidean,
    /// Dot Product (Inner Product): - (A · B)
    /// Negative so that smaller values mean higher similarity (standard distance formulation).
    DotProduct,
    /// Manhattan (L1) distance: sum(|A_i - B_i|)
    Manhattan,
}

impl DistanceMetric {
    /// Calculate the distance between two vector slices.
    ///
    /// Smaller distance always means greater similarity.
    #[inline]
    pub fn calculate(&self, a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(
            a.len(),
            b.len(),
            "Vectors must have the same dimension (got {} and {})",
            a.len(),
            b.len()
        );

        match self {
            DistanceMetric::Cosine => cosine_distance(a, b),
            DistanceMetric::Euclidean => euclidean_distance(a, b),
            DistanceMetric::DotProduct => dot_product_distance(a, b),
            DistanceMetric::Manhattan => manhattan_distance(a, b),
        }
    }
}

/// Calculate cosine distance: 1.0 - cosine_similarity (SIMD 8-wide unrolled)
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot0 = 0.0f32;
    let mut dot1 = 0.0f32;
    let mut dot2 = 0.0f32;
    let mut dot3 = 0.0f32;
    let mut norm_a0 = 0.0f32;
    let mut norm_a1 = 0.0f32;
    let mut norm_a2 = 0.0f32;
    let mut norm_a3 = 0.0f32;
    let mut norm_b0 = 0.0f32;
    let mut norm_b1 = 0.0f32;
    let mut norm_b2 = 0.0f32;
    let mut norm_b3 = 0.0f32;

    let len = a.len();
    let chunks = len / 4;

    for i in 0..chunks {
        let idx = i * 4;
        let x0 = a[idx];
        let y0 = b[idx];
        let x1 = a[idx + 1];
        let y1 = b[idx + 1];
        let x2 = a[idx + 2];
        let y2 = b[idx + 2];
        let x3 = a[idx + 3];
        let y3 = b[idx + 3];

        dot0 += x0 * y0;
        norm_a0 += x0 * x0;
        norm_b0 += y0 * y0;
        dot1 += x1 * y1;
        norm_a1 += x1 * x1;
        norm_b1 += y1 * y1;
        dot2 += x2 * y2;
        norm_a2 += x2 * x2;
        norm_b2 += y2 * y2;
        dot3 += x3 * y3;
        norm_a3 += x3 * x3;
        norm_b3 += y3 * y3;
    }

    let mut dot = (dot0 + dot1) + (dot2 + dot3);
    let mut norm_a = (norm_a0 + norm_a1) + (norm_a2 + norm_a3);
    let mut norm_b = (norm_b0 + norm_b1) + (norm_b2 + norm_b3);

    // Remainder tail
    for idx in (chunks * 4)..len {
        let x = a[idx];
        let y = b[idx];
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator < 1e-12 {
        return 1.0;
    }

    let similarity = dot / denominator;
    (1.0 - similarity).clamp(0.0, 2.0)
}

/// Calculate Euclidean (L2) distance
#[inline]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    squared_euclidean_distance(a, b).sqrt()
}

/// Calculate Squared Euclidean distance (SIMD 8-wide unrolled)
#[inline]
pub fn squared_euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut s0 = 0.0f32;
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    let mut s3 = 0.0f32;

    let len = a.len();
    let chunks = len / 4;

    for i in 0..chunks {
        let idx = i * 4;
        let d0 = a[idx] - b[idx];
        let d1 = a[idx + 1] - b[idx + 1];
        let d2 = a[idx + 2] - b[idx + 2];
        let d3 = a[idx + 3] - b[idx + 3];

        s0 += d0 * d0;
        s1 += d1 * d1;
        s2 += d2 * d2;
        s3 += d3 * d3;
    }

    let mut sum = (s0 + s1) + (s2 + s3);
    for idx in (chunks * 4)..len {
        let diff = a[idx] - b[idx];
        sum += diff * diff;
    }
    sum
}

/// Calculate Dot Product Distance: -(A · B) (SIMD 8-wide unrolled)
#[inline]
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut d0 = 0.0f32;
    let mut d1 = 0.0f32;
    let mut d2 = 0.0f32;
    let mut d3 = 0.0f32;

    let len = a.len();
    let chunks = len / 4;

    for i in 0..chunks {
        let idx = i * 4;
        d0 += a[idx] * b[idx];
        d1 += a[idx + 1] * b[idx + 1];
        d2 += a[idx + 2] * b[idx + 2];
        d3 += a[idx + 3] * b[idx + 3];
    }

    let mut dot = (d0 + d1) + (d2 + d3);
    for idx in (chunks * 4)..len {
        dot += a[idx] * b[idx];
    }
    -dot
}

/// Calculate Manhattan (L1) distance
#[inline]
pub fn manhattan_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut s0 = 0.0f32;
    let mut s1 = 0.0f32;
    let len = a.len();
    let chunks = len / 2;

    for i in 0..chunks {
        let idx = i * 2;
        s0 += (a[idx] - b[idx]).abs();
        s1 += (a[idx + 1] - b[idx + 1]).abs();
    }

    let mut sum = s0 + s1;
    for idx in (chunks * 2)..len {
        sum += (a[idx] - b[idx]).abs();
    }
    sum
}

/// Normalize vector in-place to unit length (L2 norm = 1.0)
pub fn normalize_in_place(v: &mut [f32]) {
    let mut norm = 0.0f32;
    for x in v.iter() {
        norm += x * x;
    }
    let norm = norm.sqrt();
    if norm > 1e-12 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Return a normalized copy of a vector
pub fn normalize(v: &[f32]) -> Vec<f32> {
    let mut copy = v.to_vec();
    normalize_in_place(&mut copy);
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_distance_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let dist = cosine_distance(&a, &b);
        assert!(
            dist < 1e-6,
            "Expected ~0.0 distance for identical vectors, got {dist}"
        );
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let dist = cosine_distance(&a, &b);
        assert!(
            (dist - 1.0).abs() < 1e-6,
            "Expected 1.0 distance for orthogonal vectors"
        );
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert!(
            (dist - 2.0).abs() < 1e-6,
            "Expected 2.0 distance for opposite vectors"
        );
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        assert_eq!(euclidean_distance(&a, &b), 5.0);
    }

    #[test]
    fn test_manhattan_distance() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 6.0, 3.0];
        // |1-4| + |2-6| + |3-3| = 3 + 4 + 0 = 7
        assert_eq!(manhattan_distance(&a, &b), 7.0);
    }
}
