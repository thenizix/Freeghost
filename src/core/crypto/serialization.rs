//! Serialization utilities for cryptographic types
//! Handles conversion between internal types and byte representations

use crate::{
    utils::error::{Result, NodeError},
    core::crypto::kyber::{PublicKey, SecretKey, Ciphertext, Polynomial},
};

/// Maximum size for serialized public key
const MAX_PK_SIZE: usize = 1024;
/// Maximum size for serialized secret key
const MAX_SK_SIZE: usize = 1024;
/// Maximum size for serialized ciphertext
const MAX_CT_SIZE: usize = 1024;

/// Serialize a polynomial to bytes
pub fn serialize_polynomial(poly: &Polynomial) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(512);
    
    // Write NTT flag
    bytes.push(poly.is_ntt as u8);
    
    // Write coefficients
    for coeff in &poly.coeffs {
        bytes.extend_from_slice(&coeff.to_le_bytes());
    }
    
    bytes
}

/// Deserialize a polynomial from bytes
pub fn deserialize_polynomial(bytes: &[u8]) -> Result<Polynomial> {
    if bytes.len() < 513 {  // 1 byte flag + 256 * 2 bytes coeffs
        return Err(NodeError::Crypto("Invalid polynomial bytes".into()));
    }
    
    let is_ntt = bytes[0] != 0;
    let mut coeffs = [0i16; 256];
    
    for (i, chunk) in bytes[1..].chunks(2).enumerate() {
        if i >= 256 {
            break;
        }
        coeffs[i] = i16::from_le_bytes([chunk[0], chunk[1]]);
    }
    
    Ok(Polynomial::from_coeffs(coeffs, is_ntt))
}

/// Serialize a public key to bytes
pub fn serialize_public_key(pk: &PublicKey) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(MAX_PK_SIZE);
    
    // Write matrix A dimensions
    bytes.push(pk.a.len() as u8);
    if !pk.a.is_empty() {
        bytes.push(pk.a[0].len() as u8);
    }
    
    // Write matrix A
    for row in &pk.a {
        for poly in row {
            bytes.extend(serialize_polynomial(poly));
        }
    }
    
    // Write vector t
    bytes.push(pk.t.len() as u8);
    for poly in &pk.t {
        bytes.extend(serialize_polynomial(poly));
    }
    
    if bytes.len() > MAX_PK_SIZE {
        return Err(NodeError::Crypto("Public key too large".into()));
    }
    
    Ok(bytes)
}

/// Deserialize a public key from bytes
pub fn deserialize_public_key(bytes: &[u8]) -> Result<PublicKey> {
    if bytes.len() < 2 {
        return Err(NodeError::Crypto("Invalid public key bytes".into()));
    }
    
    let mut pos = 0;
    
    // Read matrix dimensions
    let rows = bytes[pos] as usize;
    pos += 1;
    let cols = bytes[pos] as usize;
    pos += 1;
    
    // Read matrix A
    let mut a = vec![vec![Polynomial::new(); cols]; rows];
    for i in 0..rows {
        for j in 0..cols {
            if pos + 513 > bytes.len() {
                return Err(NodeError::Crypto("Invalid public key bytes".into()));
            }
            a[i][j] = deserialize_polynomial(&bytes[pos..pos+513])?;
            pos += 513;
        }
    }
    
    // Read vector t
    if pos >= bytes.len() {
        return Err(NodeError::Crypto("Invalid public key bytes".into()));
    }
    let t_len = bytes[pos] as usize;
    pos += 1;
    
    let mut t = Vec::with_capacity(t_len);
    for _ in 0..t_len {
        if pos + 513 > bytes.len() {
            return Err(NodeError::Crypto("Invalid public key bytes".into()));
        }
        t.push(deserialize_polynomial(&bytes[pos..pos+513])?);
        pos += 513;
    }
    
    Ok(PublicKey { a, t })
}

/// Serialize a secret key to bytes
pub fn serialize_secret_key(sk: &SecretKey) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(MAX_SK_SIZE);
    
    // Write vector s
    bytes.push(sk.s.len() as u8);
    for poly in &sk.s {
        bytes.extend(serialize_polynomial(poly));
    }
    
    // Write public key
    bytes.extend(serialize_public_key(&sk.public_key)?);
    
    if bytes.len() > MAX_SK_SIZE {
        return Err(NodeError::Crypto("Secret key too large".into()));
    }
    
    Ok(bytes)
}

/// Deserialize a secret key from bytes
pub fn deserialize_secret_key(bytes: &[u8]) -> Result<SecretKey> {
    if bytes.is_empty() {
        return Err(NodeError::Crypto("Invalid secret key bytes".into()));
    }
    
    let mut pos = 0;
    
    // Read vector s
    let s_len = bytes[pos] as usize;
    pos += 1;
    
    let mut s = Vec::with_capacity(s_len);
    for _ in 0..s_len {
        if pos + 513 > bytes.len() {
            return Err(NodeError::Crypto("Invalid secret key bytes".into()));
        }
        s.push(deserialize_polynomial(&bytes[pos..pos+513])?);
        pos += 513;
    }
    
    // Read public key
    let public_key = deserialize_public_key(&bytes[pos..])?;
    
    Ok(SecretKey { s, public_key })
}

/// Serialize a ciphertext to bytes
pub fn serialize_ciphertext(ct: &Ciphertext) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(MAX_CT_SIZE);
    
    // Write vector u
    bytes.push(ct.u.len() as u8);
    for poly in &ct.u {
        bytes.extend(serialize_polynomial(poly));
    }
    
    // Write polynomial v
    bytes.extend(serialize_polynomial(&ct.v));
    
    if bytes.len() > MAX_CT_SIZE {
        return Err(NodeError::Crypto("Ciphertext too large".into()));
    }
    
    Ok(bytes)
}

/// Deserialize a ciphertext from bytes
pub fn deserialize_ciphertext(bytes: &[u8]) -> Result<Ciphertext> {
    if bytes.is_empty() {
        return Err(NodeError::Crypto("Invalid ciphertext bytes".into()));
    }
    
    let mut pos = 0;
    
    // Read vector u
    let u_len = bytes[pos] as usize;
    pos += 1;
    
    let mut u = Vec::with_capacity(u_len);
    for _ in 0..u_len {
        if pos + 513 > bytes.len() {
            return Err(NodeError::Crypto("Invalid ciphertext bytes".into()));
        }
        u.push(deserialize_polynomial(&bytes[pos..pos+513])?);
        pos += 513;
    }
    
    // Read polynomial v
    if pos + 513 > bytes.len() {
        return Err(NodeError::Crypto("Invalid ciphertext bytes".into()));
    }
    let v = deserialize_polynomial(&bytes[pos..pos+513])?;
    
    Ok(Ciphertext { u, v })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::kyber::KyberKEM;

    #[test]
    fn test_polynomial_serialization() {
        let poly = Polynomial::new();
        let bytes = serialize_polynomial(&poly);
        let deserialized = deserialize_polynomial(&bytes).unwrap();
        assert_eq!(poly, deserialized);
    }

    #[test]
    fn test_key_serialization() {
        let (pk, sk) = KyberKEM::keygen().unwrap();
        
        // Test public key serialization
        let pk_bytes = serialize_public_key(&pk).unwrap();
        let pk_deserialized = deserialize_public_key(&pk_bytes).unwrap();
        
        // Test secret key serialization
        let sk_bytes = serialize_secret_key(&sk).unwrap();
        let sk_deserialized = deserialize_secret_key(&sk_bytes).unwrap();
        
        // Verify encapsulation works with serialized keys
        let (ss1, ct) = KyberKEM::encapsulate(&pk_deserialized).unwrap();
        let ss2 = KyberKEM::decapsulate(&sk_deserialized, &ct).unwrap();
        assert_eq!(ss1, ss2);
    }

    #[test]
    fn test_ciphertext_serialization() {
        let (pk, _) = KyberKEM::keygen().unwrap();
        let (_, ct) = KyberKEM::encapsulate(&pk).unwrap();
        
        let ct_bytes = serialize_ciphertext(&ct).unwrap();
        let ct_deserialized = deserialize_ciphertext(&ct_bytes).unwrap();
        
        // Verify ciphertext structure
        assert_eq!(ct.u.len(), ct_deserialized.u.len());
    }

    #[test]
    fn test_invalid_inputs() {
        assert!(deserialize_polynomial(&[0u8; 10]).is_err());
        assert!(deserialize_public_key(&[0u8; 10]).is_err());
        assert!(deserialize_secret_key(&[0u8; 10]).is_err());
        assert!(deserialize_ciphertext(&[0u8; 10]).is_err());
    }
}
