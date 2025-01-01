use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, error};
use crate::utils::error::Result;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub address: String,
    pub port: u16,
}

pub struct P2PNetwork {
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
}

impl P2PNetwork {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self, bind_address: &str, bind_port: u16) -> Result<()> {
        let listener = TcpListener::bind((bind_address, bind_port)).await?;
        info!("P2P network listening on {}:{}", bind_address, bind_port);

        loop {
            let (socket, addr) = listener.accept().await?;
            info!("New connection from {:?}", addr);

            let peers = self.peers.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(socket, peers).await {
                    error!("Connection error: {:?}", e);
                }
            });
        }
    }

    pub async fn add_peer(&self, peer_info: PeerInfo) -> Result<()> {
        let mut peers = self.peers.write().await;
        peers.insert(peer_info.id.clone(), peer_info);
        Ok(())
    }

    pub async fn broadcast(&self, message: &[u8]) -> Result<()> {
        let peers = self.peers.read().await;
        for peer in peers.values() {
            if let Err(e) = send_message(&peer.address, peer.port, message).await {
                error!("Failed to send message to {}: {:?}", peer.id, e);
            }
        }
        Ok(())
    }
}

async fn handle_connection(mut socket: TcpStream, peers: Arc<RwLock<HashMap<String, PeerInfo>>>) -> Result<()> {
    let mut buffer = [0; 1024];
    loop {
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            break;
        }

        let message = &buffer[..n];
        info!("Received message: {:?}", message);

        // Handle message (e.g., update peer info, forward message, etc.)
    }
    Ok(())
}

async fn send_message(address: &str, port: u16, message: &[u8]) -> Result<()> {
    let mut stream = TcpStream::connect((address, port)).await?;
    stream.write_all(message).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_p2p_network() {
        let network = P2PNetwork::new();
        let peer_info = PeerInfo {
            id: "peer1".to_string(),
            address: "127.0.0.1".to_string(),
            port: 8081,
        };

        network.add_peer(peer_info.clone()).await.unwrap();
        assert!(network.peers.read().await.contains_key(&peer_info.id));

        // Test broadcasting (this is a placeholder, actual test would require a running peer)
        assert!(network.broadcast(b"test message").await.is_ok());
    }
}
