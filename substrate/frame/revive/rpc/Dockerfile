FROM rust AS builder

RUN apt-get update && \
		DEBIAN_FRONTEND=noninteractive apt-get install -y \
		protobuf-compiler

WORKDIR /polkadot
COPY . /polkadot
RUN cargo build --locked --profile production -p pallet-revive-eth-rpc --bin eth-rpc

FROM docker.io/parity/base-bin:latest
COPY --from=builder /polkadot/target/production/eth-rpc /usr/local/bin

USER root
RUN useradd -m -u 1001 -U -s /bin/sh -d /polkadot polkadot && \
# unclutter and minimize the attack surface
	rm -rf /usr/bin /usr/sbin && \
# check if executable works in this container
	/usr/local/bin/eth-rpc --help

USER polkadot
EXPOSE 8545
ENTRYPOINT ["/usr/local/bin/eth-rpc"]
