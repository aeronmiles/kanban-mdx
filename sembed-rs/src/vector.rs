//! Vector math utilities for embedding operations.

use crate::Vector;

/// Returns the cosine similarity between two vectors.
///
/// Returns 0 if either vector is empty, different lengths, or zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut d = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        d += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        d / denom
    }
}

/// Returns the dot product of two vectors.
///
/// Returns 0 if vectors have different lengths.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Returns the L2 norm (magnitude) of the vector.
pub fn magnitude(v: &[f32]) -> f32 {
    let sum: f64 = v.iter().map(|x| (*x as f64) * (*x as f64)).sum();
    sum.sqrt() as f32
}

/// Returns a unit vector in the same direction.
///
/// Returns `None` if the input is empty or zero magnitude.
pub fn normalize(v: &[f32]) -> Option<Vector> {
    if v.is_empty() {
        return None;
    }
    let mag = magnitude(v);
    if mag == 0.0 {
        return None;
    }
    Some(v.iter().map(|x| x / mag).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_empty() {
        let a: Vec<f32> = Vec::new();
        let b: Vec<f32> = Vec::new();
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert!((dot(&a, &b) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn dot_different_lengths() {
        assert_eq!(dot(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn magnitude_basic() {
        let v = vec![3.0, 4.0];
        assert!((magnitude(&v) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn normalize_basic() {
        let v = vec![3.0, 4.0];
        let n = normalize(&v).unwrap();
        assert!((magnitude(&n) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normalize_empty() {
        assert!(normalize(&[]).is_none());
    }

    #[test]
    fn normalize_zero() {
        assert!(normalize(&[0.0, 0.0]).is_none());
    }
}
