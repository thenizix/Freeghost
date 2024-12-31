// src/storage/distributed/consensus.rs

use crate::storage::types::{Block, Node, ConsensusState, Transaction};
use crate::utils::error::Result;
use crate::utils::metrics::ConsensusMetrics;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use sha3::{Sha3_256, Digest};

// PBFT states
#[derive(Debug, Clone, PartialEq)]
enum PBFTState {
    PrePrepare,
    Prepare,
    Commit,
    ViewChange,
}

pub struct ConsensusProtocol {
    nodes: HashMap<String, Node>,
    state: ConsensusState,
    tx: mpsc::Sender<Block>,
    rx: mpsc::Receiver<Block>,
    consensus_timeout: Duration,
    view_number: u64,
    primary: String,
    pbft_state: PBFTState,
    prepared_blocks: HashMap<String, HashSet<String>>, // block_hash -> node_ids
    committed_blocks: HashMap<String, HashSet<String>>,
    metrics: ConsensusMetrics,
}

impl ConsensusProtocol {
    pub fn new(timeout: Duration) -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            nodes: HashMap::new(),
            state: ConsensusState::default(),
            tx,
            rx,
            consensus_timeout: timeout,
            view_number: 0,
            primary: String::new(),
            pbft_state: PBFTState::PrePrepare,
            prepared_blocks: HashMap::new(),
            committed_blocks: HashMap::new(),
            metrics: ConsensusMetrics::new(),
        }
    }

    pub async fn start_consensus(&mut self) -> Result<()> {
        let start = Instant::now();
        
        // Main PBFT loop
        loop {
            match self.pbft_state {
                PBFTState::PrePrepare => {
                    self.handle_pre_prepare().await?;
                }
                PBFTState::Prepare => {
                    self.handle_prepare().await?;
                }
                PBFTState::Commit => {
                    self.handle_commit().await?;
                }
                PBFTState::ViewChange => {
                    self.handle_view_change().await?;
                }
            }

            // Check timeout
            if start.elapsed() > self.consensus_timeout {
                self.initiate_view_change().await?;
            }
        }
    }

    async fn handle_pre_prepare(&mut self) -> Result<()> {
        if self.is_primary() {
            // Collect and validate transactions
            let txs = self.collect_transactions().await?;
            let valid_txs = self.validate_transactions(&txs).await?;
            
            // Create block proposal
            let block = self.create_block(valid_txs)?;
            
            // Broadcast pre-prepare message
            self.broadcast_pre_prepare(block).await?;
            
            self.pbft_state = PBFTState::Prepare;
        }
        Ok(())
    }

    async fn handle_prepare(&mut self) -> Result<()> {
        // Collect prepare messages
        let prepares = self.collect_prepare_messages().await?;
        
        // Verify prepare quorum
        if self.verify_prepare_quorum(&prepares)? {
            // Add to prepared blocks
            let block_hash = self.calculate_block_hash(&prepares[0])?;
            self.prepared_blocks.insert(block_hash, prepares.iter().map(|p| p.node_id.clone()).collect());
            
            self.pbft_state = PBFTState::Commit;
        }
        Ok(())
    }

    async fn handle_commit(&mut self) -> Result<()> {
        // Collect commit messages
        let commits = self.collect_commit_messages().await?;
        
        // Verify commit quorum
        if self.verify_commit_quorum(&commits)? {
            // Finalize block
            self.finalize_block(&commits[0]).await?;
            
            // Update state
            self.state.current_height += 1;
            self.state.last_block_hash = self.calculate_block_hash(&commits[0])?;
            
            self.pbft_state = PBFTState::PrePrepare;
        }
        Ok(())
    }

    async fn handle_view_change(&mut self) -> Result<()> {
        // Collect view change votes
        let votes = self.collect_view_change_votes().await?;
        
        if self.verify_view_change_quorum(&votes)? {
            // Update view number and primary
            self.view_number += 1;
            self.update_primary()?;
            
            // Reset consensus state
            self.pbft_state = PBFTState::PrePrepare;
            self.prepared_blocks.clear();
            self.committed_blocks.clear();
        }
        Ok(())
    }

    async fn validate_transactions(&self, txs: &[Transaction]) -> Result<Vec<Transaction>> {
        let mut valid_txs = Vec::new();
        
        for tx in txs {
            if self.verify_transaction(tx).await? {
                valid_txs.push(tx.clone());
            }
        }
        
        Ok(valid_txs)
    }

    async fn verify_transaction(&self, tx: &Transaction) -> Result<bool> {
        // Verify signature
        if !tx.verify_signature()? {
            return Ok(false);
        }
        
        // Check for double spending
        if self.is_double_spend(tx).await? {
            return Ok(false);
        }
        
        // Validate transaction specific rules
        if !self.validate_tx_rules(tx).await? {
            return Ok(false);
        }
        
        Ok(true)
    }

    fn is_primary(&self) -> bool {
        self.primary == self.node_id()
    }

    fn calculate_block_hash(&self, block: &Block) -> Result<String> {
        let mut hasher = Sha3_256::new();
        hasher.update(block.encode()?);
        Ok(hex::encode(hasher.finalize()))
    }

    fn verify_prepare_quorum(&self, prepares: &[Block]) -> Result<bool> {
        let quorum_size = (2 * self.nodes.len()) / 3 + 1;
        Ok(prepares.len() >= quorum_size)
    }

    fn verify_commit_quorum(&self, commits: &[Block]) -> Result<bool> {
        let quorum_size = (2 * self.nodes.len()) / 3 + 1;
        Ok(commits.len() >= quorum_size)
    }

    async fn finalize_block(&mut self, block: &Block) -> Result<()> {
        // Add block to chain
        self.state.add_block(block.clone())?;
        
        // Update metrics
        self.metrics.record_block_finalized();
        
        // Notify peers
        self.broadcast_finalized_block(block).await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transaction_validation() {
        let consensus = ConsensusProtocol::new(Duration::from_secs(5));
        let tx = Transaction::new_test();
        assert!(consensus.verify_transaction(&tx).await.unwrap());
    }

    #[tokio::test]
    async fn test_prepare_quorum() {
        let consensus = ConsensusProtocol::new(Duration::from_secs(5));
        let blocks = vec![Block::new_test(); 3];
        assert!(consensus.verify_prepare_quorum(&blocks).unwrap());
    }
}