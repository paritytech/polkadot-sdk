FROM docker.io/parity/base-bin

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG POLKADOT_VERSION
ARG POLKADOT_GPGKEY=9D4B2B6EB8F97156D19669A9FF0812D491B96798
ARG GPG_KEYSERVER="keyserver.ubuntu.com"

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="parity/polkadot" \
	io.parity.image.description="Polkadot: a platform for web3. This is the official Parity image with an injected binary." \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/scripts/ci/dockerfiles/polkadot/polkadot_injected_debian.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk/"

USER root

# show backtraces
ENV RUST_BACKTRACE 1

RUN \
	apt-get update && \
	apt-get install -y --no-install-recommends polkadot=${POLKADOT_VERSION#?} && \
	apt-get autoremove -y && \
	apt-get clean && \
	rm -rf /var/lib/apt/lists/* ; \
	mkdir -p /data /polkadot/.local/share && \
	chown -R parity:parity /data && \
	ln -s /data /polkadot/.local/share/polkadot

USER parity

# check if executable works in this container
RUN /usr/bin/polkadot --version
RUN /usr/lib/polkadot/polkadot-execute-worker --version
RUN /usr/lib/polkadot/polkadot-prepare-worker --version

EXPOSE 30333 9933 9944 9615
VOLUME ["/polkadot"]

ENTRYPOINT ["/usr/bin/polkadot"]
