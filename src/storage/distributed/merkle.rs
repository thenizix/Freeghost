// src/storage/distributed/merkle.rs
pub struct MerkleTree {
    root: RwLock<Node>,
    hasher: Sha3_256,
}

impl MerkleTree {
    pub fn insert(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let mut root = self.root.write().await;
        root.insert(key, value, &self.hasher)?;
        Ok(())
    }

    pub fn verify(&self, proof: &MerkleProof) -> Result<bool> {
        let root = self.root.read().await;
        root.verify_proof(proof, &self.hasher)
    }
}