//! CRYSTALS-Kyber implementation for post-quantum key encapsulation
//! Based on the specification: https://pq-crystals.org/kyber/

use crate::{
    utils::error::{Result, NodeError},
    core::crypto::{
        ntt::NTTContext,
        sampling::{random_poly, sample_cbd, expand_a},
    },
};
use ring::rand::SystemRandom;
use sha3::{Sha3_256, Shake256, digest::{Update, ExtendableOutput, XofReader}};
use std::convert::TryInto;

// Kyber parameters for different security levels
const KYBER_N: usize = 256;
const KYBER_Q: i16 = 3329;
const KYBER_K: usize = 3; // Kyber768
const KYBER_ETA1: u8 = 2;
const KYBER_ETA2: u8 = 2;
const KYBER_DU: u8 = 10;
const KYBER_DV: u8 = 4;

/// Represents a polynomial in R_q = Z_q[X]/(X^n + 1)
#[derive(Clone, Debug, PartialEq)]
pub struct Polynomial {
    coeffs: [i16; KYBER_N],
    is_ntt: bool,  // Track if polynomial is in NTT form
}

/// Public key for Kyber KEM
#[derive(Clone, Debug)]
pub struct PublicKey {
    // Matrix A (k x k)
    a: Vec<Vec<Polynomial>>,
    // Vector t
    t: Vec<Polynomial>,
}

/// Secret key for Kyber KEM
#[derive(Clone, Debug)]
pub struct SecretKey {
    // Vector s
    s: Vec<Polynomial>,
    // Cached public key for re-encryption
    public_key: PublicKey,
}

/// Ciphertext for Kyber KEM
#[derive(Clone, Debug)]
pub struct Ciphertext {
    // Vector u
    u: Vec<Polynomial>,
    // Scalar v
    v: Polynomial,
}

impl Polynomial {
    /// Create a new polynomial with zero coefficients
    fn new() -> Self {
        Self {
            coeffs: [0; KYBER_N],
            is_ntt: false,
        }
    }

    /// Create a polynomial from coefficients
    fn from_coeffs(coeffs: [i16; KYBER_N], is_ntt: bool) -> Self {
        Self { coeffs, is_ntt }
    }

    /// Add two polynomials in R_q
    fn add(&self, other: &Self) -> Self {
        assert_eq!(self.is_ntt, other.is_ntt, "Polynomials must be in same form");
        let mut result = Self::new();
        for i in 0..KYBER_N {
            result.coeffs[i] = (self.coeffs[i] + other.coeffs[i]) % KYBER_Q;
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
        for i in 0..KYBER_N {
            result.coeffs[i] = ((a.coeffs[i] as i32 * b.coeffs[i] as i32) % KYBER_Q as i32) as i16;
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
            coeffs: random_poly(KYBER_Q)?.try_into().map_err(|_| 
                NodeError::Crypto("Invalid coefficient count".into()))?,
            is_ntt: false,
        })
    }
}

pub struct KyberKEM;

impl KyberKEM {
    /// Generate a new key pair
    pub fn keygen() -> Result<(PublicKey, SecretKey)> {
        let ctx = NTTContext::new();
        let rng = SystemRandom::new();
        
        // Generate random seed for matrix A
        let mut seed = [0u8; 32];
        rng.fill(&mut seed)
            .map_err(|_| NodeError::Crypto("Failed to generate random seed".into()))?;
        
        // Generate matrix A using SHAKE-256
        let a_matrix = expand_a(&seed, KYBER_K)?;
        let mut a = vec![vec![Polynomial::new(); KYBER_K]; KYBER_K];
        for i in 0..KYBER_K {
            for j in 0..KYBER_K {
                a[i][j] = Polynomial::from_coeffs(
                    a_matrix[i][j].try_into()
                        .map_err(|_| NodeError::Crypto("Invalid matrix dimensions".into()))?,
                    false
                );
                a[i][j].to_ntt(&ctx);
            }
        }
        
        // Sample secret vector s
        let mut s = Vec::with_capacity(KYBER_K);
        for _ in 0..KYBER_K {
            let mut si = Polynomial::sample_noise(KYBER_ETA1)?;
            si.to_ntt(&ctx);
            s.push(si);
        }
        
        // Sample error vector e
        let mut e = Vec::with_capacity(KYBER_K);
        for _ in 0..KYBER_K {
            e.push(Polynomial::sample_noise(KYBER_ETA1)?);
        }
        
        // Compute t = As + e
        let mut t = Vec::with_capacity(KYBER_K);
        for i in 0..KYBER_K {
            let mut ti = Polynomial::new();
            for j in 0..KYBER_K {
                let prod = a[i][j].multiply(&s[j], &ctx)?;
                ti = ti.add(&prod);
            }
            let mut ei = e[i].clone();
            ei.to_ntt(&ctx);
            ti = ti.add(&ei);
            t.push(ti);
        }
        
        let pk = PublicKey { a, t };
        let sk = SecretKey {
            s,
            public_key: pk.clone(),
        };
        
        Ok((pk, sk))
    }

    /// Encapsulate a shared secret
    pub fn encapsulate(pk: &PublicKey) -> Result<(Vec<u8>, Ciphertext)> {
        let ctx = NTTContext::new();
        
        // Sample random message
        let mut m = [0u8; 32];
        SystemRandom::new()
            .fill(&mut m)
            .map_err(|_| NodeError::Crypto("Failed to generate random message".into()))?;
        
        // Sample noise vector r
        let mut r = Vec::with_capacity(KYBER_K);
        for _ in 0..KYBER_K {
            let mut ri = Polynomial::sample_noise(KYBER_ETA1)?;
            ri.to_ntt(&ctx);
            r.push(ri);
        }
        
        // Sample error vectors e1, e2
        let mut e1 = Vec::with_capacity(KYBER_K);
        for _ in 0..KYBER_K {
            e1.push(Polynomial::sample_noise(KYBER_ETA2)?);
        }
        let e2 = Polynomial::sample_noise(KYBER_ETA2)?;
        
        // Compute u = A^T r + e1
        let mut u = Vec::with_capacity(KYBER_K);
        for i in 0..KYBER_K {
            let mut ui = Polynomial::new();
            for j in 0..KYBER_K {
                let prod = pk.a[j][i].multiply(&r[j], &ctx)?;
                ui = ui.add(&prod);
            }
            let mut e1i = e1[i].clone();
            e1i.to_ntt(&ctx);
            ui = ui.add(&e1i);
            ui.from_ntt(&ctx);
            u.push(ui);
        }
        
        // Compute v = t^T r + e2 + ⌈q/2⌋ m
        let mut v = Polynomial::new();
        for i in 0..KYBER_K {
            let prod = pk.t[i].multiply(&r[i], &ctx)?;
            v = v.add(&prod);
        }
        v.from_ntt(&ctx);
        v = v.add(&e2);
        
        // Encode message in v
        for i in 0..KYBER_N {
            let m_bit = (m[i/8] >> (i%8)) & 1;
            v.coeffs[i] = (v.coeffs[i] + ((KYBER_Q as i32 * m_bit as i32) / 2) as i16) % KYBER_Q;
        }
        
        // Derive shared secret
        let mut hasher = Sha3_256::new();
        hasher.update(&m);
        let mut shared_secret = [0u8; 32];
        shared_secret.copy_from_slice(&hasher.finalize());
        
        Ok((
            shared_secret.to_vec(),
            Ciphertext { u, v },
        ))
    }

    /// Decapsulate a shared secret
    pub fn decapsulate(sk: &SecretKey, ct: &Ciphertext) -> Result<Vec<u8>> {
        let ctx = NTTContext::new();
        
        // Compute v' = v - s^T u
        let mut v_prime = ct.v.clone();
        for i in 0..KYBER_K {
            let mut ui = ct.u[i].clone();
            ui.to_ntt(&ctx);
            let prod = sk.s[i].multiply(&ui, &ctx)?;
            let mut sub = prod;
            sub.from_ntt(&ctx);
            for j in 0..KYBER_N {
                v_prime.coeffs[j] = (v_prime.coeffs[j] - sub.coeffs[j]) % KYBER_Q;
            }
        }
        
        // Decode message
        let mut m = [0u8; 32];
        for i in 0..KYBER_N {
            let mut diff = v_prime.coeffs[i];
            if diff < 0 {
                diff += KYBER_Q;
            }
            let threshold = KYBER_Q / 4;
            m[i/8] |= ((diff > threshold && diff <= (KYBER_Q - threshold)) as u8) << (i%8);
        }
        
        // Derive shared secret
        let mut hasher = Sha3_256::new();
        hasher.update(&m);
        let mut shared_secret = [0u8; 32];
        shared_secret.copy_from_slice(&hasher.finalize());
        
        Ok(shared_secret.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kyber_correctness() {
        // Generate keys
        let (pk, sk) = KyberKEM::keygen().unwrap();
        
        // Encapsulate
        let (secret1, ct) = KyberKEM::encapsulate(&pk).unwrap();
        
        // Decapsulate
        let secret2 = KyberKEM::decapsulate(&sk, &ct).unwrap();
        
        // Verify shared secrets match
        assert_eq!(secret1, secret2);
    }

    #[test]
    fn test_polynomial_operations() {
        let p1 = Polynomial::new();
        let p2 = Polynomial::new();
        
        // Test addition
        let p3 = p1.add(&p2);
        assert_eq!(p3.coeffs, [0; KYBER_N]);
        
        // Test multiplication
        let p4 = p1.multiply(&p2);
        assert_eq!(p4.coeffs, [0; KYBER_N]);
    }

    #[test]
    fn test_noise_sampling() {
        let p = Polynomial::sample_noise(KYBER_ETA1).unwrap();
        
        // Verify coefficients are within bounds
        for coeff in &p.coeffs {
            assert!(coeff.abs() <= KYBER_ETA1 as i16);
        }
    }
}
