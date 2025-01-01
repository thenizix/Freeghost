pub mod api;
pub mod core;
pub mod network;
pub mod plugins;
pub mod storage;
pub mod utils;

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

use crate::{
    utils::{config::Config, error::{Result, NodeError}},
    core::services::{identity::IdentityService, verification::VerificationService},
    network::p2p::P2PNetwork,
    storage::encrypted::EncryptedStore,
    plugins::manager::PluginManager,
};

pub struct Application {
    config: Arc<Config>,
    identity_service: Arc<IdentityService>,
    verification_service: Arc<VerificationService>,
    network: Arc<P2PNetwork>,
    storage: Arc<RwLock<EncryptedStore>>,
    plugin_manager: Arc<PluginManager>,
}

impl Application {
    pub async fn new(config: Config) -> Result<Self> {
        let config = Arc::new(config);
        
        info!("Initializing storage...");
        let storage = Arc::new(RwLock::new(
            EncryptedStore::new(&config.storage).await
                .map_err(|e| NodeError::Storage(e.to_string()))?
        ));

        info!("Initializing network...");
        let network = Arc::new(
            P2PNetwork::new(config.network.clone()).await
                .map_err(|e| NodeError::Network(e.to_string()))?
        );

        info!("Initializing services...");
        let identity_service = Arc::new(IdentityService::new(&config).await?);
        let verification_service = Arc::new(VerificationService::new(&config).await?);

        info!("Initializing plugin system...");
        let plugin_manager = Arc::new(
            PluginManager::new(&config.plugins).await
                .map_err(|e| NodeError::Plugin(e.to_string()))?
        );

        Ok(Self {
            config,
            identity_service,
            verification_service,
            network,
            storage,
            plugin_manager,
        })
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting network services...");
        self.network.start_network_maintenance().await;

        info!("Loading plugins...");
        self.plugin_manager.load_plugins().await
            .map_err(|e| NodeError::Plugin(e.to_string()))?;

        info!("Starting API server...");
        self.start_api_server().await?;

        info!("Application successfully started");
        Ok(())
    }

    async fn start_api_server(&self) -> Result<()> {
        use actix_web::{web, App, HttpServer};
        use crate::api::handlers;

        let identity_service = self.identity_service.clone();
        let verification_service = self.verification_service.clone();
        let storage = self.storage.clone();

        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(identity_service.clone()))
                .app_data(web::Data::new(verification_service.clone()))
                .app_data(web::Data::new(storage.clone()))
                .service(handlers::identity::scope())
                .service(handlers::verification::scope())
        })
        .bind((
            self.config.node.host.as_str(),
            self.config.node.port,
        ))
        .map_err(|e| NodeError::Init(format!("Failed to bind API server: {}", e)))?
        .run();

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down application...");
        
        info!("Unloading plugins...");
        self.plugin_manager.unload_plugins().await
            .map_err(|e| NodeError::Plugin(e.to_string()))?;

        info!("Closing storage...");
        self.storage.write().await.close().await
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        info!("Application shutdown complete");
        Ok(())
    }
}
