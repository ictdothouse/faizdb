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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistanceMetric {
    /// Cosine distance: 1.0 - (A · B) / (||A|| * ||B||)
    /// Range: [0.0, 2.0], where 0.0 means identical angle.
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

impl Default for DistanceMetric {
    fn default() -> Self {
        DistanceMetric::Cosine
    }
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

/// Calculate cosine distance: 1.0 - cosine_similarity
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator < 1e-12 {
        return 1.0; // Zero vector fallback
    }

    let similarity = dot / denominator;
    (1.0 - similarity).max(0.0)
}

/// Calculate Euclidean (L2) distance
#[inline]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    squared_euclidean_distance(a, b).sqrt()
}

/// Calculate Squared Euclidean distance (avoids sqrt, good for sorting)
#[inline]
pub fn squared_euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        let diff = x - y;
        sum += diff * diff;
    }
    sum
}

/// Calculate Dot Product Distance: -(A · B)
#[inline]
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
    }
    -dot
}

/// Calculate Manhattan (L1) distance
#[inline]
pub fn manhattan_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        sum += (x - y).abs();
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
        assert!(dist < 1e-6, "Expected ~0.0 distance for identical vectors, got {dist}");
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let dist = cosine_distance(&a, &b);
        assert!((dist - 1.0).abs() < 1e-6, "Expected 1.0 distance for orthogonal vectors");
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert!((dist - 2.0).abs() < 1e-6, "Expected 2.0 distance for opposite vectors");
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
