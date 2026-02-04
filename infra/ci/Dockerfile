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
    git \
    docker.io \
    python3 \
    python3-pip \
    python3-venv \
    pipx \
    jq \
    awscli \
    && rm -rf /var/lib/apt/lists/*

# Ensure pipx bin is in PATH
ENV PATH="/root/.local/bin:${PATH}"

# Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

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
    uv --version

# Set working directory
WORKDIR /workspace

# Default command
CMD ["/bin/bash"]
