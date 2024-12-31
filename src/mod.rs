// src/network/mod.rs
mod p2p;
mod tor;
mod fallback;
mod types;
mod protocols;

pub use p2p::P2PManager;
pub use tor::TorLayer;
pub use fallback::FallbackProtocols;
pub use types::*;