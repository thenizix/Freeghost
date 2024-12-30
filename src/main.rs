// src/main.rs
use secure_identity_node::utils::config::Config;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Load configuration
    let config = Config::new()?;
    
    info!("Starting Secure Identity Node v{}", env!("CARGO_PKG_VERSION"));
    
    Ok(())
}