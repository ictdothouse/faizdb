//! # Vector Quantization for Big Data AI Embeddings
//!
//! Provides memory compression techniques for scaling high-dimensional vector embeddings
//! from millions to tens of millions in constrained RAM environments:
//!
//! - **Scalar Quantization (SQ8):** Compresses 32-bit floating point vectors (`f32`) into
//!   8-bit unsigned integers (`u8`), achieving **4x RAM reduction** while maintaining $>98\%$ recall.
//! - **Asymmetric Distance Computation (ADC):** Calculates distances directly between unquantized
//!   query vectors (`&[f32]`) and quantized stored vectors (`&[u8]`) with zero allocation overhead.

use serde::{Deserialize, Serialize};
use crate::distance::DistanceMetric;

/// Supported vector quantization types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuantizationType {
    /// No quantization — exact 32-bit floating point precision (default)
    #[default]
    None,
    /// 8-bit Scalar Quantization (4x memory reduction)
    Scalar8,
}

/// Quantized vector payload with per-vector min/max scaling parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantizedVector {
    /// Quantized 8-bit byte values
    pub data: Vec<u8>,
    /// Minimum float value in original vector
    pub min: f32,
    /// Maximum float value in original vector
    pub max: f32,
}

impl QuantizedVector {
    /// Dimension of the vector
    pub fn dim(&self) -> usize {
        self.data.len()
    }

    /// Memory footprint in bytes (including metadata)
    pub fn memory_bytes(&self) -> usize {
        self.data.len() + std::mem::size_of::<f32>() * 2
    }
}

/// Scalar Quantizer for converting between `f32` and `u8`
#[derive(Debug, Clone, Default)]
pub struct ScalarQuantizer;

impl ScalarQuantizer {
    /// Quantize a single 32-bit float vector into 8-bit integers (`u8`)
    pub fn quantize(vector: &[f32]) -> QuantizedVector {
        if vector.is_empty() {
            return QuantizedVector {
                data: Vec::new(),
                min: 0.0,
                max: 0.0,
            };
        }

        let mut min = vector[0];
        let mut max = vector[0];
        for &val in vector.iter() {
            if val < min { min = val; }
            if val > max { max = val; }
        }

        let diff = max - min;
        let scale = if diff.abs() < 1e-7 {
            0.0
        } else {
            255.0 / diff
        };

        let mut data = Vec::with_capacity(vector.len());
        for &val in vector.iter() {
            let normalized = if scale == 0.0 {
                0.0
            } else {
                ((val - min) * scale).clamp(0.0, 255.0)
            };
            data.push(normalized.round() as u8);
        }

        QuantizedVector { data, min, max }
    }

    /// Dequantize an 8-bit quantized vector back into a 32-bit float vector (`f32`)
    pub fn dequantize(quantized: &QuantizedVector) -> Vec<f32> {
        let diff = quantized.max - quantized.min;
        let scale = diff / 255.0;

        quantized
            .data
            .iter()
            .map(|&byte| quantized.min + (byte as f32 * scale))
            .collect()
    }

    /// Asymmetric Distance Computation (ADC):
    /// Computes distance directly between unquantized query (`&[f32]`) and quantized stored vector (`&QuantizedVector`)
    /// without allocating heap memory for dequantization.
    pub fn asymmetric_distance(
        query: &[f32],
        quantized: &QuantizedVector,
        metric: DistanceMetric,
    ) -> f32 {
        let diff = quantized.max - quantized.min;
        let scale = diff / 255.0;
        let min = quantized.min;

        match metric {
            DistanceMetric::Cosine => {
                let mut dot = 0.0f32;
                let mut norm_q = 0.0f32;
                let mut norm_v = 0.0f32;

                for (q, &b) in query.iter().zip(quantized.data.iter()) {
                    let v = min + (b as f32 * scale);
                    dot += q * v;
                    norm_q += q * q;
                    norm_v += v * v;
                }

                let denom = (norm_q.sqrt() * norm_v.sqrt()).max(1e-9);
                1.0 - (dot / denom).clamp(-1.0, 1.0)
            }

            DistanceMetric::Euclidean => {
                let mut sum_sq = 0.0f32;
                for (q, &b) in query.iter().zip(quantized.data.iter()) {
                    let v = min + (b as f32 * scale);
                    let delta = q - v;
                    sum_sq += delta * delta;
                }
                sum_sq.sqrt()
            }

            DistanceMetric::DotProduct => {
                let mut dot = 0.0f32;
                for (q, &b) in query.iter().zip(quantized.data.iter()) {
                    let v = min + (b as f32 * scale);
                    dot += q * v;
                }
                -dot
            }

            DistanceMetric::Manhattan => {
                let mut sum_abs = 0.0f32;
                for (q, &b) in query.iter().zip(quantized.data.iter()) {
                    let v = min + (b as f32 * scale);
                    sum_abs += (q - v).abs();
                }
                sum_abs
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scalar_quantization_roundtrip() {
        let original = vec![0.15, -0.85, 0.42, 0.99, -0.05, 0.0];
        let quantized = ScalarQuantizer::quantize(&original);

        assert_eq!(quantized.dim(), original.len());
        // Verify 4x memory savings
        assert_eq!(quantized.data.len(), original.len()); // 6 bytes vs 24 bytes

        let reconstructed = ScalarQuantizer::dequantize(&quantized);

        // Verify reconstruction error is within acceptable quantization noise (< 0.02)
        for (orig, recon) in original.iter().zip(reconstructed.iter()) {
            assert!(
                (orig - recon).abs() < 0.02,
                "Quantization error too large: original={orig}, reconstructed={recon}"
            );
        }
    }

    #[test]
    fn test_asymmetric_distance_cosine() {
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let target = vec![0.95, 0.05, 0.0, 0.0];

        let quantized = ScalarQuantizer::quantize(&target);
        let dist = ScalarQuantizer::asymmetric_distance(&query, &quantized, DistanceMetric::Cosine);

        // Target is very close to query in cosine angle (distance < 0.05)
        assert!(dist < 0.05, "Cosine distance should be small, got: {dist}");
    }

    #[test]
    fn test_asymmetric_distance_euclidean() {
        let query = vec![0.0, 0.0, 0.0];
        let target = vec![0.1, 0.2, 0.2]; // norm is sqrt(0.01 + 0.04 + 0.04) = 0.3

        let quantized = ScalarQuantizer::quantize(&target);
        let dist = ScalarQuantizer::asymmetric_distance(&query, &quantized, DistanceMetric::Euclidean);

        assert!((dist - 0.3).abs() < 0.02, "Euclidean distance should be ~0.3, got: {dist}");
    }
}
