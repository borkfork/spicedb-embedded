# syntax=docker/dockerfile:1
# Build and run tests for all languages (Rust, Java, Python, C#)
# Uses BuildKit cache mounts to speed up rebuilds (mise tools, cargo registry, rust target)
FROM debian:12-slim

RUN apt-get update \
    && apt-get -y --no-install-recommends install \
    curl \
    git \
    ca-certificates \
    build-essential \
    libicu72 \
    && rm -rf /var/lib/apt/lists/*

# Install mise
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
ENV MISE_DATA_DIR="/mise"
ENV MISE_CONFIG_DIR="/mise"
ENV MISE_INSTALL_PATH="/usr/local/bin/mise"
ENV PATH="/mise/shims:$PATH"

RUN curl https://mise.run | sh

WORKDIR /workspace
COPY . .

# Trust project config (required for non-interactive use)
RUN mise trust

# Install tools from mise.toml (cached between builds)
RUN --mount=type=cache,target=/mise \
    mise install

# Build shared/c once, then run all tests in parallel
# Cache mounts: cargo (registry, git, target), sccache, Maven, pip, nuget
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/workspace/target \
    --mount=type=cache,target=/root/.cache/sccache \
    --mount=type=cache,target=/root/.m2/repository \
    --mount=type=cache,target=/root/.cache/pip \
    --mount=type=cache,target=/root/.nuget/packages \
    mise run shared-c-build && mise run test 2> >(grep -v 'MADV_DONTNEED\|running under QEMU' >&2)
