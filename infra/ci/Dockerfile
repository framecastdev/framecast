FROM ubuntu:22.04

# Avoid interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    postgresql-client \
    git \
    docker.io \
    python3 \
    python3-pip \
    python3-venv \
    pipx \
    jq \
    awscli \
    ca-certificates \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20.x (required for GitHub Actions like actions/checkout)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Zig (required by cargo-lambda for cross-compilation)
RUN npm install -g @ziglang/cli

# Ensure pipx bin is in PATH
ENV PATH="/root/.local/bin:${PATH}"

# Install Rust toolchain to a shared location (works for any user)
ENV RUSTUP_HOME=/opt/rust/rustup
ENV CARGO_HOME=/opt/rust/cargo
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain stable
ENV PATH="/opt/rust/cargo/bin:${PATH}"

# Add Rust components
RUN rustup component add rustfmt clippy

# Pre-compile heavy Cargo tools (MAJOR TIME SAVINGS)
# These take 7-15 minutes each to compile
RUN cargo install just
RUN cargo install sqlx-cli --no-default-features --features postgres
RUN cargo install cargo-lambda
RUN cargo install cargo-watch

# Install Python tools via pipx
RUN pipx install aws-sam-cli
RUN pipx install uv
RUN pipx install pre-commit

# Verify installations
RUN just --version && \
    sqlx --version && \
    cargo lambda --version && \
    sam --version && \
    uv --version && \
    zig version

# Set working directory
WORKDIR /workspace

# Default command
CMD ["/bin/bash"]
