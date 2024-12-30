#!/bin/bash

# Create project structure
cargo new secure-identity-node
cd secure-identity-node

# Create directories
for dir in \
    src/{core/{identity,crypto,blockchain},network,api/handlers,plugins/{traits,official/{face_recognition,behavior_analysis,quantum_resistant}},storage,utils} \
    tests/{common,integration,unit} \
    benches \
    config; do
    mkdir -p "$dir"
done

# Create initial Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "secure-identity-node"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.0", features = ["full"] }
solana-sdk = "1.17"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
config = "0.13"
dilithium = "0.1"
libp2p = "0.52"

[dev-dependencies]
tokio-test = "0.4"
criterion = "0.4"

[[bench]]
name = "identity_bench"
harness = false

[[bench]]
name = "crypto_bench"
harness = false
EOF

# Create core type definitions
cat > src/core/types.rs << 'EOF'
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Identity error: {0}")]
    Identity(String),
    #[error("Crypto error: {0}")]
    Crypto(String),
    #[error("Network error: {0}")]
    Network(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
EOF

# Initialize git repository
git init
cat > .gitignore << 'EOF'
/target
**/*.rs.bk
Cargo.lock
.env
*.pem
*.key
EOF

echo "Project structure created successfully!"