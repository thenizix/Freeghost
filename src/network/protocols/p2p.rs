// src/network/protocols/p2p.rs
use libp2p::{
    identity,
    Swarm,
    NetworkBehaviour,
    gossipsub::{Gossipsub, GossipsubConfig},
};

#[derive(NetworkBehaviour)]
pub struct P2PBehavior {
    gossipsub: Gossipsub,
}

pub struct P2PProtocol {
    swarm: Swarm<P2PBehavior>,
    topics: HashMap<MessageType, TopicHash>,
}

#[async_trait]
impl NetworkProtocol for P2PProtocol {
    async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        let topic = self.topics.get(&message.message_type)
            .ok_or_else(|| NodeError::Network("Unknown message type".into()))?;
            
        self.swarm.behaviour_mut().gossipsub.publish(
            topic.clone(),
            serde_json::to_vec(&message)?
        )?;
        
        Ok(())
    }

    async fn verify_template(&self, template: &Template) -> Result<bool> {
        let message = NetworkMessage {
            id: Uuid::new_v4().to_string(),
            message_type: MessageType::TemplateVerification,
            payload: serde_json::to_vec(template)?,
            timestamp: chrono::Utc::now().timestamp(),
        };

        self.broadcast(message).await?;
        
        // Wait for consensus
        let mut verification_responses = 0;
        let timeout = tokio::time::sleep(Duration::from_secs(30));
        
        tokio::select! {
            _ = timeout => {
                Ok(false)
            }
            result = self.wait_for_consensus() => {
                Ok(result?)
            }
        }
    }
}
