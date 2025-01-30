FROM rust AS builder

RUN apt-get update && \
		DEBIAN_FRONTEND=noninteractive apt-get install -y \
		protobuf-compiler \
		clang libclang-dev

WORKDIR /polkadot
COPY . /polkadot
RUN rustup component add rust-src
RUN cargo build --locked --profile production -p pallet-revive-eth-rpc --bin eth-indexer

FROM docker.io/parity/base-bin:latest
COPY --from=builder /polkadot/target/production/eth-indexer /usr/local/bin

USER root
RUN useradd -m -u 1001 -U -s /bin/sh -d /polkadot polkadot && \
# unclutter and minimize the attack surface
	rm -rf /usr/bin /usr/sbin && \
# check if executable works in this container
	/usr/local/bin/eth-indexer --help

USER polkadot

ENTRYPOINT ["/usr/local/bin/eth-indexer"]

# We call the help by default
CMD ["--help"]
