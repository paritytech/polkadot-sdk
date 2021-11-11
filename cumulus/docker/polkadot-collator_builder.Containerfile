# This file is sourced from https://github.com/paritytech/polkadot/blob/master/scripts/dockerfiles/polkadot/polkadot_builder.Dockerfile
# This is the build stage for Polkadot-collator. Here we create the binary in a temporary image.
FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /cumulus
COPY . /cumulus

RUN cargo build --release --locked -p polkadot-collator

# This is the 2nd stage: a very small image where we copy the Polkadot binary."
FROM docker.io/library/ubuntu:20.04

LABEL io.parity.image.type="builder" \
    io.parity.image.authors="devops-team@parity.io" \
    io.parity.image.vendor="Parity Technologies" \
    io.parity.image.description="Multistage Docker image for Polkadot-collator" \
    io.parity.image.source="https://github.com/paritytech/polkadot/blob/${VCS_REF}/docker/test-parachain-collator.dockerfile" \
    io.parity.image.documentation="https://github.com/paritytech/cumulus"

COPY --from=builder /cumulus/target/release/polkadot-collator /usr/local/bin

RUN useradd -m -u 1000 -U -s /bin/sh -d /cumulus polkadot-collator && \
    mkdir -p /data /cumulus/.local/share && \
    chown -R polkadot-collator:polkadot-collator /data && \
    ln -s /data /cumulus/.local/share/polkadot-collator && \
# unclutter and minimize the attack surface
    rm -rf /usr/bin /usr/sbin && \
# check if executable works in this container
    /usr/local/bin/polkadot-collator --version

USER polkadot-collator

EXPOSE 30333 9933 9944 9615
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/polkadot-collator"]
