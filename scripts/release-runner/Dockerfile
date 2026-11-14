FROM ubuntu:24.04

ARG RUNNER_VERSION=2.338.2
ARG RUNNER_ARCH=x64
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git gh jq sudo unzip xz-utils zstd file \
        build-essential cmake ninja-build pkg-config desktop-file-utils dpkg-dev \
        libfontconfig-dev libfreetype-dev libwayland-dev libx11-dev libx11-xcb-dev \
        libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
        libnss3 libcups2t64 libdbus-1-3 libatk1.0-0t64 libatk-bridge2.0-0t64 \
        libgbm1 libasound2t64 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 \
        libpango-1.0-0 libcairo2 libexpat1 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -s /bin/bash runner \
    && echo 'runner ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/runner

WORKDIR /home/runner/actions-runner
RUN curl -fsSL "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-${RUNNER_ARCH}-${RUNNER_VERSION}.tar.gz" | tar -xz \
    && ./bin/installdependencies.sh \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /home/runner/_work /home/runner/.cargo/registry \
    && chown -R runner:runner /home/runner

USER runner
ENV PATH=/home/runner/.cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.99.0 -c clippy -c rustfmt

COPY --chown=runner:runner entrypoint.sh /home/runner/entrypoint.sh
ENTRYPOINT ["/home/runner/entrypoint.sh"]
