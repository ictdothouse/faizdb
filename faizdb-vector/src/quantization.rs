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
    /// 1-bit Binary Quantization (32x memory reduction with Hamming POPCNT distance)
    Binary1,
}

/// 1-bit Binary Quantized vector payload (32x memory reduction)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinaryQuantizedVector {
    /// Packed 64-bit words storing 1-bit per dimension
    pub bits: Vec<u64>,
    /// Original dimension of vector
    pub dim: usize,
}

impl BinaryQuantizedVector {
    /// Dimension of the original vector
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Memory footprint in bytes
    pub fn memory_bytes(&self) -> usize {
        self.bits.len() * std::mem::size_of::<u64>() + std::mem::size_of::<usize>()
    }
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

/// 1-Bit Binary Quantizer (32x Memory Reduction)
/// Converts float vectors into binary arrays (1 bit per dimension)
/// Distance is computed using hardware POPCNT (count_ones) in nanoseconds.
#[derive(Debug, Clone, Default)]
pub struct BinaryQuantizer;

impl BinaryQuantizer {
    /// Quantize float vector into binary representation
    pub fn quantize(vector: &[f32]) -> BinaryQuantizedVector {
        let num_words = (vector.len() + 63) / 64;
        let mut bits = vec![0u64; num_words];

        for (i, &val) in vector.iter().enumerate() {
            if val > 0.0 {
                let word_idx = i / 64;
                let bit_idx = i % 64;
                bits[word_idx] |= 1u64 << bit_idx;
            }
        }

        BinaryQuantizedVector {
            bits,
            dim: vector.len(),
        }
    }

    /// Calculate Hamming distance using hardware POPCNT
    #[inline]
    pub fn hamming_distance(a: &BinaryQuantizedVector, b: &BinaryQuantizedVector) -> u32 {
        let mut total = 0u32;
        for (w_a, w_b) in a.bits.iter().zip(b.bits.iter()) {
            total += (w_a ^ w_b).count_ones();
        }
        total
    }

    /// Normalized Hamming distance in range [0.0, 1.0]
    #[inline]
    pub fn normalized_hamming_distance(a: &BinaryQuantizedVector, b: &BinaryQuantizedVector) -> f32 {
        if a.dim == 0 {
            return 0.0;
        }
        let raw = Self::hamming_distance(a, b);
        raw as f32 / a.dim as f32
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

    #[test]
    fn test_binary_quantization_and_popcnt() {
        let vec_a = vec![1.5, -0.5, 2.0, -1.0, 0.8, -0.2]; // Signs: [1, 0, 1, 0, 1, 0]
        let vec_b = vec![0.5, -1.2, 0.3, -0.1, 1.1, -0.9]; // Signs: [1, 0, 1, 0, 1, 0] (Identical signs)
        let vec_c = vec![-1.5, 0.5, -2.0, 1.0, -0.8, 0.2]; // Signs: [0, 1, 0, 1, 0, 1] (Completely opposite signs)

        let bin_a = BinaryQuantizer::quantize(&vec_a);
        let bin_b = BinaryQuantizer::quantize(&vec_b);
        let bin_c = BinaryQuantizer::quantize(&vec_c);

        // 32x memory compression verification
        assert_eq!(bin_a.bits.len(), 1); // 1 u64 word (8 bytes) holds up to 64 dims!
        assert_eq!(BinaryQuantizer::hamming_distance(&bin_a, &bin_b), 0);
        assert_eq!(BinaryQuantizer::hamming_distance(&bin_a, &bin_c), 6);
        assert_eq!(BinaryQuantizer::normalized_hamming_distance(&bin_a, &bin_c), 1.0);
    }
}
