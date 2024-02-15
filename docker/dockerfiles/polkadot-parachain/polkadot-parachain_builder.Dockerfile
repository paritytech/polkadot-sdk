# This file is sourced from https://github.com/paritytech/polkadot-sdk/blob/master/docker/dockerfiles/polkadot-parachain/polkadot-parachain_builder.Dockerfile
# This is the build stage for polkadot-parachain. Here we create the binary in a temporary image.
FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /cumulus
COPY . /cumulus

RUN cargo build --release --locked -p polkadot-parachain

# This is the 2nd stage: a very small image where we copy the Polkadot binary."
FROM docker.io/library/ubuntu:20.04

LABEL io.parity.image.type="builder" \
    io.parity.image.authors="devops-team@parity.io" \
    io.parity.image.vendor="Parity Technologies" \
    io.parity.image.description="Multistage Docker image for polkadot-parachain" \
    io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/docker/dockerfiles/polkadot-parachain/polkadot-parachain_builder.Dockerfile" \
    io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk/tree/master/cumulus"

COPY --from=builder /cumulus/target/release/polkadot-parachain /usr/local/bin

RUN useradd -m -u 1000 -U -s /bin/sh -d /cumulus polkadot-parachain && \
    mkdir -p /data /cumulus/.local/share && \
    chown -R polkadot-parachain:polkadot-parachain /data && \
    ln -s /data /cumulus/.local/share/polkadot-parachain && \
# unclutter and minimize the attack surface
    rm -rf /usr/bin /usr/sbin && \
# check if executable works in this container
    /usr/local/bin/polkadot-parachain --version

USER polkadot-parachain

EXPOSE 30333 9933 9944 9615
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/polkadot-parachain"]
