// src/storage/encrypted.rs
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rocksdb::{DB, Options};

pub struct EncryptedStore {
    db: DB,
    cipher: Aes256Gcm,
}

impl EncryptedStore {
    pub fn new(config: StorageConfig, encryption_key: &[u8]) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        
        let db = DB::open(&opts, config.path)?;
        let key = Key::from_slice(encryption_key);
        let cipher = Aes256Gcm::new(key);
        
        Ok(Self { db, cipher })
    }

    pub async fn store_keypair(&self, keypair: &KeyPair) -> Result<()> {
        let encrypted = self.encrypt_data(&serde_json::to_vec(keypair)?)?;
        self.db.put(keypair.id.as_bytes(), encrypted)?;
        Ok(())
    }

    fn encrypt_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        todo!("Implement encryption")
    }
}