FROM "mcr.microsoft.com/devcontainers/typescript-node:1-22-bookworm"

# Install system dependencies required for Rust builds and Tauri
RUN apt-get update && apt-get install -y \
    libdbus-1-dev \
    pkg-config \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libssl-dev \
    libglib2.0-dev \
    libjavascriptcoregtk-4.1-dev \
    libsoup-3.0-dev \
    libatk1.0-dev \
    libpango1.0-dev \
    libgdk-pixbuf-2.0-dev \
    libcairo2-dev \
    && rm -rf /var/lib/apt/lists/*

RUN corepack enable pnpm

ENV SHELL=/bin/bash
ENV PNPM_HOME="/home/node/.local/share/pnpm"
ENV PATH="$PNPM_HOME:$PATH"

USER node

RUN pnpm setup

RUN pnpm install -g @google/gemini-cli @anthropic-ai/claude-code

RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN curl -fsSL https://dprint.dev/install.sh | sh
RUN curl -fsSL https://bun.sh/install | bash

# Create Doppler directory with proper permissions (mimicking the feature's behavior)
USER root
RUN mkdir -p /var/lib/doppler && \
    chown -R node:node /var/lib/doppler && \
    chmod -R 755 /var/lib/doppler

USER node

# Set Doppler config directory
ENV DOPPLER_CONFIG_DIR=/var/lib/doppler

RUN pnpm dlx playwright install
RUN npm install -g claude-flow@alpha
