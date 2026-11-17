# Release .deb builder on Ubuntu 22.04 (glibc 2.35), so the binary runs on the oldest supported
# distros (Ubuntu 22.04, Debian 12). WebKitGTK 4.1 comes from jammy-updates (the release pocket
# has 2.36, below the 2.38 the package declares). See build.sh for the build itself.
FROM ubuntu:22.04
ARG NODE_VERSION=26.13.1
ARG RUST_VERSION=1.99.1
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils git file build-essential pkg-config \
      libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libssl-dev libdbus-1-dev \
 && rm -rf /var/lib/apt/lists/* \
 && dpkg --compare-versions "$(dpkg-query -W -f='${Version}' libwebkit2gtk-4.1-dev)" ge 2.38 \
 && dpkg --compare-versions "$(dpkg-query -W -f='${Version}' libc6)" lt 2.36
# Node: the official tarball, checked against the release's SHASUMS256.txt.
RUN cd /tmp && curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
 && curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" | grep " node-v${NODE_VERSION}-linux-x64.tar.xz\$" | sha256sum -c - \
 && tar -xJf "node-v${NODE_VERSION}-linux-x64.tar.xz" -C /opt && mv "/opt/node-v${NODE_VERSION}-linux-x64" /opt/node \
 && rm "node-v${NODE_VERSION}-linux-x64.tar.xz"
# Rust: rustup-init checked against its published SHA-256, then the pinned toolchain.
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/node/bin:/opt/cargo/bin:$PATH
RUN cd /tmp && curl -fsSLO https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init \
 && echo "$(curl -fsSL https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256 | cut -d' ' -f1)  rustup-init" | sha256sum -c - \
 && chmod +x rustup-init && ./rustup-init -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" -c rustfmt -c clippy \
 && rm rustup-init && rustc --version && node --version
COPY build.sh /usr/local/bin/stoneql-deb-build
