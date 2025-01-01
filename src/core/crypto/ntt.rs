//! Number Theoretic Transform implementation for Kyber
//! This module provides efficient polynomial multiplication in R_q

use crate::utils::error::{Result, NodeError};

// NTT parameters for Kyber
const N: usize = 256;
const Q: i16 = 3329;

// Primitive root of unity modulo q
const ZETA: i16 = 17;

/// Montgomery reduction constants
const QINV: i32 = 62209;  // q^(-1) mod 2^16
const R: i32 = 1 << 16;   // R = 2^16

/// Stores pre-computed twiddle factors for NTT
pub struct NTTContext {
    // Powers of zeta
    zetas: [i16; N],
    // Inverse powers of zeta
    zetas_inv: [i16; N],
}

impl NTTContext {
    /// Initialize NTT context with pre-computed twiddle factors
    pub fn new() -> Self {
        let mut ctx = Self {
            zetas: [0; N],
            zetas_inv: [0; N],
        };
        
        // Pre-compute twiddle factors
        ctx.precompute_twiddle_factors();
        ctx
    }

    /// Pre-compute twiddle factors (powers of zeta)
    fn precompute_twiddle_factors(&mut self) {
        let mut tmp = 1i16;
        for i in 0..N {
            self.zetas[i] = tmp;
            tmp = montgomery_reduce((tmp as i32 * ZETA as i32) % Q as i32) as i16;
        }

        // Compute inverse twiddle factors
        let zeta_inv = mod_inverse(ZETA, Q);
        tmp = 1i16;
        for i in 0..N {
            self.zetas_inv[i] = tmp;
            tmp = montgomery_reduce((tmp as i32 * zeta_inv as i32) % Q as i32) as i16;
        }
    }

    /// Forward Number Theoretic Transform
    pub fn forward(&self, a: &mut [i16; N]) {
        let mut k = 1;
        let mut len = N / 2;

        while len >= 1 {
            let mut start = 0;
            let mut j = 0;

            while start < N {
                let zeta = self.zetas[k];
                k += 1;

                for i in start..(start + len) {
                    let t = montgomery_reduce((zeta as i32 * a[i + len] as i32) % Q as i32) as i16;
                    a[i + len] = barrett_reduce(a[i] - t);
                    a[i] = barrett_reduce(a[i] + t);
                }

                start = j + 2 * len;
                j = start;
            }

            len >>= 1;
        }
    }

    /// Inverse Number Theoretic Transform
    pub fn inverse(&self, a: &mut [i16; N]) {
        let mut k = N - 1;
        let mut len = 1;

        while len < N {
            let mut start = 0;
            let mut j = 0;

            while start < N {
                let zeta_inv = self.zetas_inv[k];
                k -= 1;

                for i in start..(start + len) {
                    let t = a[i];
                    a[i] = barrett_reduce(t + a[i + len]);
                    a[i + len] = montgomery_reduce((zeta_inv as i32 * 
                        barrett_reduce(t - a[i + len]) as i32) % Q as i32) as i16;
                }

                start = j + 2 * len;
                j = start;
            }

            len <<= 1;
        }

        // Multiply by N^(-1) mod q
        let n_inv = mod_inverse(N as i16, Q);
        for i in 0..N {
            a[i] = montgomery_reduce((a[i] as i32 * n_inv as i32) % Q as i32) as i16;
        }
    }
}

/// Montgomery reduction
/// Computes aR^(-1) mod q where R = 2^16
fn montgomery_reduce(a: i32) -> i32 {
    let u = (a * QINV) & ((1 << 16) - 1);
    let t = (a - u * Q as i32) >> 16;
    if t >= Q as i32 {
        t - Q as i32
    } else {
        t
    }
}

/// Barrett reduction
/// Reduces input mod q
fn barrett_reduce(a: i16) -> i16 {
    let v = ((a as i32 * 20159) + (1 << 25)) >> 26;
    let t = a as i32 - v * Q as i32;
    t as i16
}

/// Compute modular multiplicative inverse using extended Euclidean algorithm
fn mod_inverse(a: i16, m: i16) -> i16 {
    let mut t = 0i16;
    let mut newt = 1i16;
    let mut r = m;
    let mut newr = a;

    while newr != 0 {
        let quotient = r / newr;
        (t, newt) = (newt, t - quotient * newt);
        (r, newr) = (newr, r - quotient * newr);
    }

    if t < 0 {
        t += m;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, thread_rng};

    #[test]
    fn test_ntt_roundtrip() {
        let ctx = NTTContext::new();
        let mut rng = thread_rng();
        
        // Generate random polynomial
        let mut a = [0i16; N];
        for coeff in &mut a {
            *coeff = rng.gen_range(0..Q);
        }
        
        // Save original
        let original = a;
        
        // Forward NTT
        ctx.forward(&mut a);
        
        // Inverse NTT
        ctx.inverse(&mut a);
        
        // Check if we got back the original polynomial
        for i in 0..N {
            assert_eq!(barrett_reduce(a[i]), barrett_reduce(original[i]));
        }
    }

    #[test]
    fn test_montgomery_reduction() {
        let a = 12345i32;
        let reduced = montgomery_reduce(a * R % (Q as i32));
        assert!(reduced >= 0 && reduced < Q as i32);
    }

    #[test]
    fn test_barrett_reduction() {
        let a = 12345i16;
        let reduced = barrett_reduce(a);
        assert!(reduced >= 0 && reduced < Q);
    }

    #[test]
    fn test_mod_inverse() {
        let a = 17i16;
        let inv = mod_inverse(a, Q);
        assert_eq!((a as i32 * inv as i32) % Q as i32, 1);
    }
}
