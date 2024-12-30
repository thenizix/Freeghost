// tests/integration/network_tests.rs
#[tokio::test]
async fn test_p2p_message_broadcast() {
    let mut network = P2PNetwork::new().await.expect("Failed to create network");
    let message = NetworkMessage {
        id: Uuid::new_v4().to_string(),
        message_type: MessageType::IdentityVerification,
        payload: vec![1, 2, 3],
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    let result = network.broadcast(message.clone()).await;
    assert!(result.is_ok());
}