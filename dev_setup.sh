#!/bin/bash

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting Freeghost development environment setup...${NC}"

# Check for root privileges
if [ "$EUID" -ne 0 ]; then 
    echo -e "${YELLOW}Please run as root or with sudo${NC}"
    exit 1
fi

# Detect OS
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$NAME
else
    echo -e "${RED}Cannot detect operating system${NC}"
    exit 1
fi

# Install system dependencies based on OS
echo -e "${GREEN}Installing system dependencies...${NC}"

case $OS in
    "Ubuntu"|"Debian GNU/Linux")
        apt-get update
        apt-get install -y \
            build-essential \
            pkg-config \
            libssl-dev \
            librocksdb-dev \
            clang \
            cmake \
            git \
            curl \
            llvm \
            python3 \
            python3-pip
        ;;
    "Fedora")
        dnf install -y \
            gcc \
            gcc-c++ \
            make \
            pkg-config \
            openssl-devel \
            rocksdb-devel \
            clang \
            cmake \
            git \
            curl \
            llvm \
            python3 \
            python3-pip
        ;;
    *)
        echo -e "${RED}Unsupported operating system: $OS${NC}"
        exit 1
        ;;
esac

# Install Rust if not already installed
if ! command -v rustc &> /dev/null; then
    echo -e "${GREEN}Installing Rust...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
else
    echo -e "${YELLOW}Rust is already installed${NC}"
fi

# Update Rust
echo -e "${GREEN}Updating Rust...${NC}"
rustup update
rustup component add rustfmt clippy

# Create development directories
echo -e "${GREEN}Creating development directories...${NC}"
mkdir -p config/local
mkdir -p data/storage
mkdir -p certs

# Generate development certificates
echo -e "${GREEN}Generating development certificates...${NC}"
openssl req -x509 -newkey rsa:4096 -nodes -keyout certs/server.key -out certs/server.crt -days 365 -subj "/CN=localhost"

# Create local configuration
echo -e "${GREEN}Creating local configuration...${NC}"
if [ ! -f config/local.toml ]; then
    cp config/default.toml config/local.toml
    # Generate random encryption key
    ENCRYPTION_KEY=$(openssl rand -hex 32)
    # Update encryption key in local config
    sed -i "s/encryption_key = \"\"/encryption_key = \"$ENCRYPTION_KEY\"/" config/local.toml
fi

# Set up git hooks
echo -e "${GREEN}Setting up git hooks...${NC}"
cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash
set -e

echo "Running cargo fmt..."
cargo fmt -- --check

echo "Running cargo clippy..."
cargo clippy -- -D warnings

echo "Running tests..."
cargo test
EOF

chmod +x .git/hooks/pre-commit

# Build project
echo -e "${GREEN}Building project...${NC}"
cargo build

# Run tests
echo -e "${GREEN}Running tests...${NC}"
cargo test

# Final instructions
echo -e "${GREEN}Setup complete!${NC}"
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Review config/local.toml and adjust settings as needed"
echo "2. Start the development server with: cargo run"
echo "3. Run tests with: cargo test"
echo "4. Format code with: cargo fmt"
echo "5. Check for issues with: cargo clippy"

# Check if setup was successful
if [ $? -eq 0 ]; then
    echo -e "${GREEN}Development environment setup completed successfully!${NC}"
else
    echo -e "${RED}Setup failed. Please check the error messages above.${NC}"
    exit 1
fi
