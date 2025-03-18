ARG DOCKERHUB_ORG_NAME
# Install BPF SDK
FROM rust:1.79.0 AS builder
RUN rustup component add rustfmt
RUN rustup component add clippy
RUN cargo install rustfilt
RUN apt-get update
RUN apt-get install -y libssl-dev libudev-dev pkg-config libprotobuf-dev protobuf-compiler
WORKDIR /opt
ARG SOLANA_BPF_VERSION
RUN sh -c "$(curl -sSfL https://release.anza.xyz/${SOLANA_BPF_VERSION}/install)" && \
    /root/.local/share/solana/install/active_release/bin/sdk/sbf/scripts/install.sh

ENV PATH=${PATH}:/root/.local/share/solana/install/active_release/bin


# Build evm_loader
FROM builder AS evm-loader-builder

COPY .git /opt/neon-evm/.git
COPY evm_loader /opt/neon-evm/evm_loader
WORKDIR /opt/neon-evm/evm_loader
ARG REVISION
ENV NEON_REVISION=${REVISION}

RUN cargo fmt --check && \
    cargo clippy --release \
      --config 'patch.crates-io.ethnum.git="https://github.com/neonlabsorg/ethnum.git"'\
      --config 'patch.crates-io.ethnum.branch="main"' && \
    cargo build --release \
      --config 'patch.crates-io.ethnum.git="https://github.com/neonlabsorg/ethnum.git"'\
      --config 'patch.crates-io.ethnum.branch="main"' && \
    cargo test --release && \
    cargo build-sbf --manifest-path program/Cargo.toml --features devnet && cp target/deploy/evm_loader.so target/deploy/evm_loader-devnet.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features devnet-2 && cp target/deploy/evm_loader.so target/deploy/evm_loader-devnet-2.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features testnet && cp target/deploy/evm_loader.so target/deploy/evm_loader-testnet.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features govertest && cp target/deploy/evm_loader.so target/deploy/evm_loader-govertest.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features govertest,emergency && cp target/deploy/evm_loader.so target/deploy/evm_loader-govertest-emergency.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features mainnet && cp target/deploy/evm_loader.so target/deploy/evm_loader-mainnet.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features mainnet,emergency && cp target/deploy/evm_loader.so target/deploy/evm_loader-mainnet-emergency.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features rollup && cp target/deploy/evm_loader.so target/deploy/evm_loader-rollup.so && \
    cargo build-sbf --manifest-path program/Cargo.toml --features ci --dump


# Add neon_test_invoke_program to the genesis
FROM ${DOCKERHUB_ORG_NAME}/neon_test_programs:latest AS neon_test_programs

# Define solana-image that contains utility
FROM builder AS base

ARG MAINNET_SOLANA_URL
RUN solana program dump metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s /opt/metaplex.so --url ${MAINNET_SOLANA_URL}

COPY --from=evm-loader-builder /opt/neon-evm/evm_loader/target/deploy/evm_loader*.so /opt/
COPY --from=evm-loader-builder /opt/neon-evm/evm_loader/target/deploy/evm_loader-dump.txt /opt/
COPY --from=evm-loader-builder /opt/neon-evm/evm_loader/target/release/neon-cli /opt/
COPY --from=evm-loader-builder /opt/neon-evm/evm_loader/target/release/neon-api /opt/

COPY --from=neon_test_programs /opt/deploy/ /opt/deploy/
COPY --from=evm-loader-builder /opt/neon-evm/evm_loader/target/release/neon-rpc /opt/
COPY --from=evm-loader-builder /opt/neon-evm/evm_loader/target/release/libneon_lib.so /opt/libs/current/

COPY ci/wait-for-solana.sh \
    ci/wait-for-neon.sh \
    ci/solana-run-neon.sh \
    ci/deploy-evm.sh \
    ci/deploy-multi-tokens.sh \
    ci/create-test-accounts.sh \
    ci/evm_loader-keypair.json \
    /opt/

COPY solidity/ /opt/solidity
COPY ci/operator-keypairs/ /opt/operator-keypairs
COPY ci/operator-keypairs/id.json /root/.config/solana/id.json
COPY ci/operator-keypairs/id2.json /root/.config/solana/id2.json
COPY ci/keys/ /opt/keys

ENV PATH=${PATH}:/opt

ENTRYPOINT [ "/opt/solana-run-neon.sh" ]
