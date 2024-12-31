// src/network/fallback.rs
use super::{
    transport::{Transport, TransportType},
    types::{NetworkMessage, NetworkPeer},
    protocols::p2p::P2PNetwork,
};
use crate::utils::metrics::NetworkMetrics;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;

pub struct FallbackManager {
    network: Arc<P2PNetwork>,
    fallback_chain: Vec<FallbackProtocol>,
    active_protocol: RwLock<usize>,
    metrics: Arc<NetworkMetrics>,
}

pub struct FallbackProtocol {
    protocol_type: TransportType,
    priority: u8,
    retry_policy: RetryPolicy,
    transport: Box<dyn Transport>,
}

#[derive(Clone)]
pub struct RetryPolicy {
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    jitter: bool,
}

impl FallbackManager {
    pub fn new(network: Arc<P2PNetwork>, metrics: Arc<NetworkMetrics>) -> Result<Self> {
        let fallback_chain = vec![
            FallbackProtocol::new(TransportType::Tor, 1)?,
            FallbackProtocol::new(TransportType::Quic, 2)?,
            FallbackProtocol::new(TransportType::Tcp, 3)?,
        ];

        Ok(Self {
            network,
            fallback_chain,
            active_protocol: RwLock::new(0),
            metrics,
        })
    }

    pub async fn send_with_fallback(&self, message: NetworkMessage) -> Result<()> {
        let mut attempts = 0;
        let mut last_error = None;

        for protocol in &self.fallback_chain {
            match self.try_send_with_protocol(protocol, &message).await {
                Ok(()) => {
                    self.metrics.message_sent();
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);
                    self.metrics.message_failed();
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            NetworkError::AllProtocolsFailed("No fallback protocols available".into())
        }))
    }

    async fn try_send_with_protocol(
        &self,
        protocol: &FallbackProtocol,
        message: &NetworkMessage,
    ) -> Result<()> {
        let mut attempt = 0;
        let mut delay = protocol.retry_policy.base_delay;

        while attempt < protocol.retry_policy.max_attempts {
            match protocol.transport.send(message.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    log::warn!(
                        "Failed to send via {:?} (attempt {}/{}): {}",
                        protocol.protocol_type,
                        attempt + 1,
                        protocol.retry_policy.max_attempts,
                        e
                    );

                    if attempt + 1 < protocol.retry_policy.max_attempts {
                        tokio::time::sleep(delay).await;
                        delay = Self::calculate_next_delay(delay, &protocol.retry_policy);
                    }
                }
            }
            attempt += 1;
        }

        Err(NetworkError::ProtocolFailed(format!(
            "{:?} failed after {} attempts",
            protocol.protocol_type, attempt
        )))
    }

    fn calculate_next_delay(current: Duration, policy: &RetryPolicy) -> Duration {
        let mut next = std::cmp::min(
            current * 2,
            policy.max_delay,
        );

        if policy.jitter {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let jitter = rng.gen_range(0.8..1.2);
            next = Duration::from_secs_f64(next.as_secs_f64() * jitter);
        }

        next
    }
}
