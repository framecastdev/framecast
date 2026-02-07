FROM ubuntu:24.04

# Avoid interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    unzip \
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
    ca-certificates \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install AWS CLI v2 (supports AWS_ENDPOINT_URL env var)
RUN curl -fsSL "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o awscliv2.zip \
    && unzip -q awscliv2.zip \
    && ./aws/install \
    && rm -rf awscliv2.zip aws

# Install Node.js 22.x LTS (required for GitHub Actions like actions/checkout)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
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

# Pre-compile Cargo tools in a single layer (smaller image, all --locked for reproducibility)
RUN cargo install --locked just \
    && cargo install --locked sqlx-cli --no-default-features --features postgres \
    && cargo install --locked cargo-lambda \
    && cargo install --locked cargo-watch \
    && cargo install --locked cargo-mutants

# Install Python tools via pipx
RUN pipx install uv
RUN pipx install pre-commit

# Install OpenTofu
RUN curl --proto '=https' --tlsv1.2 -fsSL https://get.opentofu.org/install-opentofu.sh -o install-opentofu.sh \
    && chmod +x install-opentofu.sh \
    && ./install-opentofu.sh --install-method deb \
    && rm install-opentofu.sh

# Verify installations
RUN just --version && \
    sqlx --version && \
    cargo lambda --version && \
    cargo mutants --version && \
    tofu --version && \
    uv --version && \
    aws --version && \
    zig version

# Set working directory
WORKDIR /workspace

# Default command
CMD ["/bin/bash"]
