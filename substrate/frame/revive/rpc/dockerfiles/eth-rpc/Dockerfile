FROM docker.io/paritytech/ci-unified:latest AS builder

WORKDIR /polkadot
COPY . /polkadot

RUN cargo fetch
RUN cargo build --workspace --locked --profile production -p pallet-revive-eth-rpc --bin eth-rpc

FROM docker.io/parity/base-bin:latest
COPY --from=builder /polkadot/target/production/eth-rpc /usr/local/bin

USER root
RUN useradd -m -u 1001 -U -s /bin/sh -d /polkadot polkadot && \
	rm -rf /usr/bin /usr/sbin && \
	/usr/local/bin/eth-rpc --help

USER polkadot

# 8545 is the default port for the RPC server
# 9616 is the default port for the prometheus metrics
EXPOSE 8545 9616
ENTRYPOINT ["/usr/local/bin/eth-rpc"]

# We call the help by default
CMD ["--help"]
