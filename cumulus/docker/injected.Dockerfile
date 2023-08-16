FROM docker.io/parity/base-bin

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG IMAGE_NAME

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="${IMAGE_NAME}" \
	io.parity.image.description="Cumulus, the Polkadot collator." \
	io.parity.image.source="https://github.com/paritytech/polkadot/blob/${VCS_REF}/scripts/docker/Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/cumulus/"

# show backtraces
ENV RUST_BACKTRACE 1

USER root

RUN	mkdir -p /specs

# add polkadot-parachain binary to the docker image
COPY ./target/release-artifacts/* /usr/local/bin
COPY ./parachains/chain-specs/*.json /specs/

USER parity

# check if executable works in this container
RUN /usr/local/bin/polkadot-parachain --version

EXPOSE 30333 9933 9944 9615
VOLUME ["/polkadot", "/specs"]

ENTRYPOINT ["/usr/local/bin/polkadot-parachain"]
