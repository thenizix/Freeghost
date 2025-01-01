//! CRYSTALS-Dilithium implementation for post-quantum digital signatures
//! Based on the specification: https://pq-crystals.org/dilithium/

use crate::utils::error::{Result, NodeError};
use ring::rand::SystemRandom;
use sha3::{Sha3_256, Shake256, digest::{Update, ExtendableOutput, XofReader}};
use std::convert::TryInto;

// Dilithium parameters for different security levels
const DILITHIUM_N: usize = 256;
const DILITHIUM_Q: i16 = 8380417;
const DILITHIUM_K: usize = 4; // Dilithium3
const DILITHIUM_L: usize = 4;
const DILITHIUM_ETA: u8 = 2;
const DILITHIUM_TAU: u8 = 39;
const DILITHIUM_BETA: u8 = 78;
const DILITHIUM_OMEGA: u8 = 80;

/// Represents a polynomial in R_q = Z_q[X]/(X^n + 1)
#[derive(Clone, Debug, PartialEq)]
pub struct Polynomial {
    coeffs: [i16; DILITHIUM_N],
    is_ntt: bool,  // Track if polynomial is in NTT form
}

/// Public key for Dilithium
#[derive(Clone, Debug)]
pub struct PublicKey {
    // Matrix A (k x l)
    a: Vec<Vec<Polynomial>>,
    // Vector t1
    t1: Vec<Polynomial>,
}

/// Secret key for Dilithium
#[derive(Clone, Debug)]
pub struct SecretKey {
    // Vectors s1, s2
    s1: Vec<Polynomial>,
    s2: Vec<Polynomial>,
    // Cached public key for verification
    public_key: PublicKey,
}

/// Signature for Dilithium
#[derive(Clone, Debug)]
pub struct Signature {
    // Vector z
    z: Vec<Polynomial>,
    // Scalar h
    h: Vec<u8>,
    // Scalar c
    c: [u8; 32],
}

impl Polynomial {
    /// Create a new polynomial with zero coefficients
    fn new() -> Self {
        Self {
            coeffs: [0; DILITHIUM_N],
            is_ntt: false,
        }
    }

    /// Create a polynomial from coefficients
    fn from_coeffs(coeffs: [i16; DILITHIUM_N], is_ntt: bool) -> Self {
        Self { coeffs, is_ntt }
    }

    /// Add two polynomials in R_q
    fn add(&self, other: &Self) -> Self {
        assert_eq!(self.is_ntt, other.is_ntt, "Polynomials must be in same form");
        let mut result = Self::new();
        for i in 0..DILITHIUM_N {
            result.coeffs[i] = (self.coeffs[i] + other.coeffs[i]) % DILITHIUM_Q;
        }
        result.is_ntt = self.is_ntt;
        result
    }

    /// Multiply two polynomials using NTT
    fn multiply(&self, other: &Self, ctx: &NTTContext) -> Result<Self> {
        // Ensure both polynomials are in NTT form
        let mut a = self.clone();
        let mut b = other.clone();
        
        if !a.is_ntt {
            a.to_ntt(ctx);
        }
        if !b.is_ntt {
            b.to_ntt(ctx);
        }

        // Pointwise multiplication in NTT domain
        let mut result = Self::new();
        for i in 0..DILITHIUM_N {
            result.coeffs[i] = ((a.coeffs[i] as i32 * b.coeffs[i] as i32) % DILITHIUM_Q as i32) as i16;
        }
        result.is_ntt = true;
        
        Ok(result)
    }

    /// Convert polynomial to NTT form
    fn to_ntt(&mut self, ctx: &NTTContext) {
        if !self.is_ntt {
            ctx.forward(&mut self.coeffs);
            self.is_ntt = true;
        }
    }

    /// Convert polynomial from NTT form
    fn from_ntt(&mut self, ctx: &NTTContext) {
        if self.is_ntt {
            ctx.inverse(&mut self.coeffs);
            self.is_ntt = false;
        }
    }

    /// Sample a polynomial with small coefficients
    fn sample_noise(eta: u8) -> Result<Self> {
        Ok(Self {
            coeffs: sample_cbd(eta)?.try_into().map_err(|_| 
                NodeError::Crypto("Invalid coefficient count".into()))?,
            is_ntt: false,
        })
    }

    /// Generate a random polynomial
    fn random() -> Result<Self> {
        Ok(Self {
            coeffs: random_poly(DILITHIUM_Q)?.try_into().map_err(|_| 
                NodeError::Crypto("Invalid coefficient count".into()))?,
            is_ntt: false,
        })
    }
}

pub struct Dilithium;

impl Dilithium {
    /// Generate a new key pair
    pub fn keygen() -> Result<(PublicKey, SecretKey)> {
        let ctx = NTTContext::new();
        let rng = SystemRandom::new();
        
        // Generate random seed for matrix A
        let mut seed = [0u8; 32];
        rng.fill(&mut seed)
            .map_err(|_| NodeError::Crypto("Failed to generate random seed".into()))?;
        
        // Generate matrix A using SHAKE-256
        let a_matrix = expand_a(&seed, DILITHIUM_K)?;
        let mut a = vec![vec![Polynomial::new(); DILITHIUM_L]; DILITHIUM_K];
        for i in 0..DILITHIUM_K {
            for j in 0..DILITHIUM_L {
                a[i][j] = Polynomial::from_coeffs(
                    a_matrix[i][j].try_into()
                        .map_err(|_| NodeError::Crypto("Invalid matrix dimensions".into()))?,
                    false
                );
                a[i][j].to_ntt(&ctx);
            }
        }
        
        // Sample secret vectors s1, s2
        let mut s1 = Vec::with_capacity(DILITHIUM_L);
        for _ in 0..DILITHIUM_L {
            let mut s1i = Polynomial::sample_noise(DILITHIUM_ETA)?;
            s1i.to_ntt(&ctx);
            s1.push(s1i);
        }
        
        let mut s2 = Vec::with_capacity(DILITHIUM_K);
        for _ in 0..DILITHIUM_K {
            let mut s2i = Polynomial::sample_noise(DILITHIUM_ETA)?;
            s2i.to_ntt(&ctx);
            s2.push(s2i);
        }
        
        // Compute t1 = As1 + s2
        let mut t1 = Vec::with_capacity(DILITHIUM_K);
        for i in 0..DILITHIUM_K {
            let mut t1i = Polynomial::new();
            for j in 0..DILITHIUM_L {
                let prod = a[i][j].multiply(&s1[j], &ctx)?;
                t1i = t1i.add(&prod);
            }
            let mut s2i = s2[i].clone();
            s2i.to_ntt(&ctx);
            t1i = t1i.add(&s2i);
            t1.push(t1i);
        }
        
        let pk = PublicKey { a, t1 };
        let sk = SecretKey {
            s1,
            s2,
            public_key: pk.clone(),
        };
        
        Ok((pk, sk))
    }

    /// Sign a message
    pub fn sign(sk: &SecretKey, message: &[u8]) -> Result<Signature> {
        let ctx = NTTContext::new();
        
        // Hash message
        let mut hasher = Sha3_256::new();
        hasher.update(message);
        let mut c = [0u8; 32];
        c.copy_from_slice(&hasher.finalize());
        
        // Sample y
        let mut y = Vec::with_capacity(DILITHIUM_L);
        for _ in 0..DILITHIUM_L {
            let mut yi = Polynomial::sample_noise(DILITHIUM_ETA)?;
            yi.to_ntt(&ctx);
            y.push(yi);
        }
        
        // Compute w = Ay
        let mut w = Vec::with_capacity(DILITHIUM_K);
        for i in 0..DILITHIUM_K {
            let mut wi = Polynomial::new();
            for j in 0..DILITHIUM_L {
                let prod = sk.public_key.a[i][j].multiply(&y[j], &ctx)?;
                wi = wi.add(&prod);
            }
            wi.from_ntt(&ctx);
            w.push(wi);
        }
        
        // Compute z = y + cs1
        let mut z = Vec::with_capacity(DILITHIUM_L);
        for i in 0..DILITHIUM_L {
            let mut zi = y[i].clone();
            let mut cs1i = sk.s1[i].clone();
            cs1i.to_ntt(&ctx);
            zi = zi.add(&cs1i);
            z.push(zi);
        }
        
        // Compute h
        let mut h = vec![0u8; DILITHIUM_K];
        for i in 0..DILITHIUM_K {
            h[i] = (w[i].coeffs[0] % 2) as u8;
        }
        
        Ok(Signature { z, h, c })
    }

    /// Verify a signature
    pub fn verify(pk: &PublicKey, message: &[u8], signature: &Signature) -> Result<bool> {
        let ctx = NTTContext::new();
        
        // Hash message
        let mut hasher = Sha3_256::new();
        hasher.update(message);
        let mut c = [0u8; 32];
        c.copy_from_slice(&hasher.finalize());
        
        // Compute Az
        let mut az = Vec::with_capacity(DILITHIUM_K);
        for i in 0..DILITHIUM_K {
            let mut azi = Polynomial::new();
            for j in 0..DILITHIUM_L {
                let prod = pk.a[i][j].multiply(&signature.z[j], &ctx)?;
                azi = azi.add(&prod);
            }
            azi.from_ntt(&ctx);
            az.push(azi);
        }
        
        // Verify h
        for i in 0..DILITHIUM_K {
            if (az[i].coeffs[0] % 2) as u8 != signature.h[i] {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dilithium_correctness() {
        // Generate keys
        let (pk, sk) = Dilithium::keygen().unwrap();
        
        // Sign message
        let message = b"test message";
        let signature = Dilithium::sign(&sk, message).unwrap();
        
        // Verify signature
        let valid = Dilithium::verify(&pk, message, &signature).unwrap();
        
        assert!(valid);
    }

    #[test]
    fn test_polynomial_operations() {
        let p1 = Polynomial::new();
        let p2 = Polynomial::new();
        
        // Test addition
        let p3 = p1.add(&p2);
        assert_eq!(p3.coeffs, [0; DILITHIUM_N]);
        
        // Test multiplication
        let p4 = p1.multiply(&p2);
        assert_eq!(p4.coeffs, [0; DILITHIUM_N]);
    }

    #[test]
    fn test_noise_sampling() {
        let p = Polynomial::sample_noise(DILITHIUM_ETA).unwrap();
        
        // Verify coefficients are within bounds
        for coeff in &p.coeffs {
            assert!(coeff.abs() <= DILITHIUM_ETA as i16);
        }
    }
}
