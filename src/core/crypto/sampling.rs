//! Sampling functions for Kyber
//! Implements noise sampling using binomial distribution and uniform sampling

use crate::utils::error::{Result, NodeError};
use ring::rand::{SecureRandom, SystemRandom};
use sha3::{Shake256, digest::{Update, ExtendableOutput, XofReader}};

/// Generate a random polynomial in R_q with coefficients uniformly random mod q
pub fn random_poly(q: i16) -> Result<Vec<i16>> {
    let mut coeffs = vec![0i16; 256];
    let rng = SystemRandom::new();
    
    // We need ceil(log2(q)) bits per coefficient
    let bits_needed = 32 - (q - 1).leading_zeros() as usize;
    let bytes_per_coeff = (bits_needed + 7) / 8;
    
    let mut bytes = vec![0u8; 256 * bytes_per_coeff];
    rng.fill(&mut bytes)
        .map_err(|_| NodeError::Crypto("Failed to generate random bytes".into()))?;

    for (i, chunk) in bytes.chunks(bytes_per_coeff).enumerate() {
        let mut val = 0u32;
        for &byte in chunk {
            val = (val << 8) | byte as u32;
        }
        val &= (1 << bits_needed) - 1;
        
        // Rejection sampling to ensure uniform distribution
        if val < q as u32 {
            coeffs[i] = val as i16;
        } else {
            // Try again for this coefficient
            let mut retry_bytes = vec![0u8; bytes_per_coeff];
            while val >= q as u32 {
                rng.fill(&mut retry_bytes)
                    .map_err(|_| NodeError::Crypto("Failed to generate random bytes".into()))?;
                val = 0;
                for &byte in &retry_bytes {
                    val = (val << 8) | byte as u32;
                }
                val &= (1 << bits_needed) - 1;
            }
            coeffs[i] = val as i16;
        }
    }

    Ok(coeffs)
}

/// Sample from centered binomial distribution with parameter eta
pub fn sample_cbd(eta: u8) -> Result<Vec<i16>> {
    let mut coeffs = vec![0i16; 256];
    let rng = SystemRandom::new();
    
    // We need 2*eta bits per coefficient
    let bytes_needed = (256 * 2 * eta as usize + 7) / 8;
    let mut bytes = vec![0u8; bytes_needed];
    
    rng.fill(&mut bytes)
        .map_err(|_| NodeError::Crypto("Failed to generate random bytes".into()))?;

    let mut bit_pos = 0;
    for coeff in &mut coeffs {
        let mut a = 0u32;
        let mut b = 0u32;
        
        // Count 1s in first eta bits
        for _ in 0..eta {
            let byte_idx = bit_pos / 8;
            let bit_idx = bit_pos % 8;
            a += ((bytes[byte_idx] >> bit_idx) & 1) as u32;
            bit_pos += 1;
        }
        
        // Count 1s in second eta bits
        for _ in 0..eta {
            let byte_idx = bit_pos / 8;
            let bit_idx = bit_pos % 8;
            b += ((bytes[byte_idx] >> bit_idx) & 1) as u32;
            bit_pos += 1;
        }
        
        *coeff = (a as i16) - (b as i16);
    }

    Ok(coeffs)
}

/// Expand seed into matrix A using SHAKE-128
pub fn expand_a(seed: &[u8], k: usize) -> Result<Vec<Vec<Vec<i16>>>> {
    let mut a = vec![vec![vec![0i16; 256]; k]; k];
    let mut shake = Shake256::default();
    
    for i in 0..k {
        for j in 0..k {
            // Create unique input for each matrix element
            shake.update(seed);
            shake.update(&[i as u8, j as u8]);
            
            let mut reader = shake.finalize_xof();
            let mut buf = [0u8; 3];  // 3 bytes gives us enough bits for q=3329
            
            for coeff in &mut a[i][j] {
                loop {
                    reader.read(&mut buf);
                    let val = u32::from_le_bytes([buf[0], buf[1], buf[2], 0]) & 0x0FFF;
                    if val < 3329 {
                        *coeff = val as i16;
                        break;
                    }
                }
            }
        }
    }
    
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_random_poly() {
        let q = 3329;
        let poly = random_poly(q).unwrap();
        
        // Check length
        assert_eq!(poly.len(), 256);
        
        // Check range
        for coeff in poly {
            assert!(coeff >= 0 && coeff < q);
        }
    }

    #[test]
    fn test_cbd_distribution() {
        let eta = 2;
        let samples = sample_cbd(eta).unwrap();
        let mut histogram = HashMap::new();
        
        // Count occurrences of each value
        for &x in &samples {
            *histogram.entry(x).or_insert(0) += 1;
        }
        
        // Check range
        for &x in samples.iter() {
            assert!(x >= -(eta as i16) && x <= eta as i16);
        }
        
        // Verify rough symmetry of distribution
        for x in 0..=eta as i16 {
            let pos_count = histogram.get(&x).unwrap_or(&0);
            let neg_count = histogram.get(&(-x)).unwrap_or(&0);
            // Allow some variance but they should be roughly equal
            assert!((pos_count - neg_count).abs() < 50);
        }
    }

    #[test]
    fn test_expand_a() {
        let seed = [0u8; 32];
        let k = 3;
        let a = expand_a(&seed, k).unwrap();
        
        // Check dimensions
        assert_eq!(a.len(), k);
        for row in &a {
            assert_eq!(row.len(), k);
            for poly in row {
                assert_eq!(poly.len(), 256);
            }
        }
        
        // Check range
        for i in 0..k {
            for j in 0..k {
                for coeff in &a[i][j] {
                    assert!(*coeff >= 0 && *coeff < 3329);
                }
            }
        }
        
        // Verify deterministic expansion
        let a2 = expand_a(&seed, k).unwrap();
        assert_eq!(a, a2);
    }
}
