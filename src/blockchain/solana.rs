use solana_sdk::{
    client::RpcClient,
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use crate::utils::error::Result;

pub struct SolanaClient {
    rpc_client: Arc<RpcClient>,
    keypair: Arc<Keypair>,
}

impl SolanaClient {
    pub fn new(rpc_url: &str, keypair: Keypair) -> Self {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            rpc_url.to_string(),
            CommitmentConfig::confirmed(),
        ));
        let keypair = Arc::new(keypair);

        Self { rpc_client, keypair }
    }

    pub async fn send_transaction(&self, transaction: Transaction) -> Result<()> {
        let signature = self.rpc_client.send_and_confirm_transaction(&transaction)
            .map_err(|e| {
                error!("Failed to send transaction: {:?}", e);
                e
            })?;
        info!("Transaction sent with signature: {:?}", signature);
        Ok(())
    }

    pub async fn get_balance(&self) -> Result<u64> {
        let balance = self.rpc_client.get_balance(&self.keypair.pubkey())
            .map_err(|e| {
                error!("Failed to get balance: {:?}", e);
                e
            })?;
        info!("Current balance: {:?}", balance);
        Ok(balance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::read_keypair_file;

    #[tokio::test]
    async fn test_solana_client() {
        let keypair = read_keypair_file("path/to/keypair.json").expect("Failed to read keypair");
        let solana_client = SolanaClient::new("https://api.mainnet-beta.solana.com", keypair);

        // Test getting balance (this is a placeholder, actual test would require a funded account)
        assert!(solana_client.get_balance().await.is_ok());
    }
}
