use secure_identity_node::{Application, utils::config::Config};
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging with better formatting
    tracing_subscriber::fmt()
        .with_target(true)
        .with_thread_ids(true)
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .init();
    
    info!("Starting Secure Identity Node v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config = Config::new().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;
    
    // Initialize application
    let app = Application::new(config).await.map_err(|e| {
        error!("Failed to initialize application: {}", e);
        e
    })?;
    
    // Start the application
    app.start().await.map_err(|e| {
        error!("Failed to start application: {}", e);
        e
    })?;
    
    info!("Application started successfully");

    // Handle shutdown signals
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received shutdown signal");
                let _ = shutdown_tx.send(());
            }
            Err(err) => {
                error!("Failed to listen for shutdown signal: {}", err);
            }
        }
    });

    // Wait for shutdown signal
    let _ = shutdown_rx.await;
    
    // Perform graceful shutdown
    if let Err(e) = app.shutdown().await {
        error!("Error during shutdown: {}", e);
    }
    
    info!("Application shutdown complete");
    Ok(())
}
