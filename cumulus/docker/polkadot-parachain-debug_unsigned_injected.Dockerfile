FROM docker.io/library/ubuntu:20.04

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG IMAGE_NAME

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="${IMAGE_NAME}" \
	io.parity.image.description="Cumulus, the Polkadot collator." \
	io.parity.image.source="https://github.com/paritytech/cumulus/blob/${VCS_REF}/scripts/docker/polkadot-parachain-debug_unsigned_injected.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/cumulus/"

# show backtraces
ENV RUST_BACKTRACE 1

# install tools and dependencies
RUN apt-get update && \
	DEBIAN_FRONTEND=noninteractive apt-get install -y \
	libssl1.1 \
	ca-certificates \
	curl && \
	# apt cleanup
	apt-get autoremove -y && \
	apt-get clean && \
	find /var/lib/apt/lists/ -type f -not -name lock -delete; \
	# add user and link ~/.local/share/polkadot-parachain to /data
	useradd -m -u 10000 -U -s /bin/sh -d /polkadot-parachain polkadot-parachain && \
	mkdir -p /data /polkadot-parachain/.local/share && \
	chown -R polkadot-parachain:polkadot-parachain /data && \
	ln -s /data /polkadot-parachain/.local/share/polkadot-parachain && \
	mkdir -p /specs

# add polkadot-parachain binary to the docker image
COPY ./artifacts/polkadot-parachain /usr/local/bin
COPY ./parachains/chain-specs/*.json /specs/

USER polkadot-parachain

# check if executable works in this container
RUN /usr/local/bin/polkadot-parachain --version

EXPOSE 30333 9933 9944
VOLUME ["/polkadot-parachain"]

ENTRYPOINT ["/usr/local/bin/polkadot-parachain"]
