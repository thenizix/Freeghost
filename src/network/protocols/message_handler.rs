// src/network/protocols/message_handler.rs
use super::{NetworkMessage, MessageType, P2PNetwork};
use crate::utils::error::Result;
use tokio::sync::mpsc;
use std::{collections::HashMap, sync::Arc};

pub struct MessageHandler {
    network: Arc<P2PNetwork>,
    handlers: HashMap<MessageType, Box<dyn MessageProcessor>>,
    message_tx: mpsc::Sender<NetworkMessage>,
    message_rx: mpsc::Receiver<NetworkMessage>,
    running: Arc<RwLock<bool>>,
}

#[async_trait]
pub trait MessageProcessor: Send + Sync {
    async fn process(&self, message: NetworkMessage) -> Result<()>;
}

impl MessageHandler {
    pub fn new(network: Arc<P2PNetwork>) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        
        Self {
            network,
            handlers: HashMap::new(),
            message_tx: tx,
            message_rx: rx,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn register_handler(
        &mut self,
        message_type: MessageType,
        handler: Box<dyn MessageProcessor>,
    ) {
        self.handlers.insert(message_type, handler);
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        
        let running = self.running.clone();
        let mut rx = self.message_rx.clone();
        let handlers = self.handlers.clone();
        
        tokio::spawn(async move {
            while *running.read().await {
                if let Some(message) = rx.recv().await {
                    if let Some(handler) = handlers.get(&message.message_type) {
                        if let Err(e) = handler.process(message).await {
                            log::error!("Error processing message: {}", e);
                        }
                    }
                }
            }
        });
        
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        Ok(())
    }

    pub async fn handle_message(&self, message: NetworkMessage) -> Result<()> {
        self.message_tx.send(message).await
            .map_err(|e| NetworkError::MessageHandling(e.to_string()))?;
        Ok(())
    }
}

// Default message processors implementation
pub struct IdentityVerificationProcessor {
    identity_service: Arc<IdentityService>,
}

#[async_trait]
impl MessageProcessor for IdentityVerificationProcessor {
    async fn process(&self, message: NetworkMessage) -> Result<()> {
        match message.message_type {
            MessageType::IdentityVerification => {
                let verification = serde_json::from_slice(&message.payload)?;
                self.identity_service.verify_identity(verification).await?;
                Ok(())
            }
            _ => Err(NetworkError::InvalidMessageType),
        }
    }
}

pub struct TemplateUpdateProcessor {
    template_service: Arc<TemplateService>,
}

#[async_trait]
impl MessageProcessor for TemplateUpdateProcessor {
    async fn process(&self, message: NetworkMessage) -> Result<()> {
        match message.message_type {
            MessageType::TemplateUpdate => {
                let update = serde_json::from_slice(&message.payload)?;
                self.template_service.update_template(update).await?;
                Ok(())
            }
            _ => Err(NetworkError::InvalidMessageType),
        }
    }
}

// Update P2PProtocol to use new components
impl P2PProtocol {
    pub async fn new(config: P2PConfig) -> Result<Self> {
        let network = Arc::new(P2PNetwork::new(config).await?);
        let discovery_service = Arc::new(DiscoveryService::new(
            network.clone(),
            vec!["bootstrap1.node:7000", "bootstrap2.node:7000"].into_iter().map(String::from).collect(),
        ));

        let mut message_handler = MessageHandler::new(network.clone());
        
        // Register default message processors
        message_handler.register_handler(
            MessageType::IdentityVerification,
            Box::new(IdentityVerificationProcessor::new()),
        );
        message_handler.register_handler(
            MessageType::TemplateUpdate,
            Box::new(TemplateUpdateProcessor::new()),
        );

        Ok(Self {
            network,
            discovery_service,
            message_handler: Arc::new(message_handler),
        })
    }
}

// Implementation for metrics collection
pub struct NetworkMetrics {
    message_sent: AtomicU64,
    message_failed: AtomicU64,
    peers_connected: AtomicU64,
    peers_removed: AtomicU64,
    heartbeat_sent: AtomicU64,
    heartbeat_failed: AtomicU64,
}

impl NetworkMetrics {
    pub fn new() -> Self {
        Self {
            message_sent: AtomicU64::new(0),
            message_failed: AtomicU64::new(0),
            peers_connected: AtomicU64::new(0),
            peers_removed: AtomicU64::new(0),
            heartbeat_sent: AtomicU64::new(0),
            heartbeat_failed: AtomicU64::new(0),
        }
    }

    pub fn message_sent(&self) {
        self.message_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn message_failed(&self) {
        self.message_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn peer_connected(&self) {
        self.peers_connected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn peer_removed(&self) {
        self.peers_removed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn heartbeat_sent(&self) {
        self.heartbeat_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn heartbeat_failed(&self) {
        self.heartbeat_failed.fetch_add(1, Ordering::Relaxed);
    }
}