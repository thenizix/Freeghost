// src/network/transport/tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use std::net::SocketAddr;
    use uuid::Uuid;

    async fn setup_tcp_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    while let Ok(n) = socket.read(&mut buf).await {
                        if n == 0 { break; }
                        socket.write_all(&buf[..n]).await.unwrap();
                    }
                });
            }
        });
        
        addr
    }

    #[tokio::test]
    async fn test_tcp_transport() {
        let server_addr = setup_tcp_server().await;
        let mut transport = TcpTransport::new(format!("127.0.0.1:{}", server_addr.port()));
        
        // Test connection
        transport.connect().await.unwrap();
        assert!(transport.is_connected().await);
        
        // Test message sending/receiving
        let test_message = NetworkMessage {
            id: Uuid::new_v4(),
            message_type: MessageType::HealthCheck,
            payload: vec![1, 2, 3],
            timestamp: chrono::Utc::now().timestamp(),
            sender: "test".to_string(),
            recipient: None,
        };
        
        transport.send(test_message.clone()).await.unwrap();
        let received = transport.receive().await.unwrap();
        
        assert_eq!(test_message.id, received.id);
        assert_eq!(test_message.payload, received.payload);
        
        // Test disconnection
        transport.disconnect().await.unwrap();
        assert!(!transport.is_connected().await);
    }
}