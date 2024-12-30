// src/storage/consensus.rs

use tokio::sync::mpsc;
use crate::network::p2p_manager::PeerMessage;
use crate::storage::distributed_storage::StorageBlock;

pub struct ConsensusManager {
    peers: Vec<PeerId>,
    msg_sender: mpsc::Sender<PeerMessage>,
    storage_blocks: HashMap<BlockHash, StorageBlock>,
}

impl ConsensusManager {
    pub async fn new(msg_sender: mpsc::Sender<PeerMessage>) -> Self {
        Self {
            peers: Vec::new(),
            msg_sender,
            storage_blocks: HashMap::new(),
        }
    }

    // Consensus protocol for storage blocks
    pub async fn propose_block(&mut self, block: StorageBlock) -> Result<()> {
        // Generate block hash
        let block_hash = block.calculate_hash();
        
        // Broadcast proposal to peers
        self.broadcast_proposal(block.clone()).await?;
        
        // Wait for consensus (2/3 majority)
        let votes = self.collect_votes(block_hash).await?;
        if self.check_consensus(votes) {
            self.storage_blocks.insert(block_hash, block);
            Ok(())
        } else {
            Err(ConsensusError::InsufficientVotes)
        }
    }

    // Peer synchronization mechanism
    pub async fn sync_with_peers(&mut self) -> Result<()> {
        // Request block lists from peers
        let peer_blocks = self.request_peer_blocks().await?;
        
        // Compare and fetch missing blocks
        for (peer_id, blocks) in peer_blocks {
            for block_hash in blocks {
                if !self.storage_blocks.contains_key(&block_hash) {
                    let block = self.fetch_block_from_peer(peer_id, block_hash).await?;
                    self.storage_blocks.insert(block_hash, block);
                }
            }
        }
        Ok(())
    }
}