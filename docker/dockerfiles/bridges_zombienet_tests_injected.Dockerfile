# this image is built on top of existing Zombienet image
ARG ZOMBIENET_IMAGE
# this image uses substrate-relay image built elsewhere
ARG SUBSTRATE_RELAY_IMAGE=docker.io/paritytech/substrate-relay:v2023-11-07-rococo-westend-initial-relayer

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG IMAGE_NAME

# we need `substrate-relay` binary, built elsewhere
FROM ${SUBSTRATE_RELAY_IMAGE} as relay-builder

# the base image is the zombienet image - we are planning to run zombienet tests using native
# provider here
FROM ${ZOMBIENET_IMAGE}

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="${IMAGE_NAME}" \
	io.parity.image.description="Bridges Zombienet tests." \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/docker/dockerfiles/bridges_zombienet_tests_injected.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk/bridges/zombienet"

# show backtraces
ENV RUST_BACKTRACE 1
USER root

# for native provider to work (TODO: fix in zn docker?)
RUN apt-get update && apt-get install -y procps sudo
RUN yarn global add @polkadot/api-cli

# add polkadot binary to the docker image
COPY ./artifacts/polkadot /usr/local/bin/
COPY ./artifacts/polkadot-execute-worker /usr/local/bin/
COPY ./artifacts/polkadot-prepare-worker /usr/local/bin/
# add polkadot-parachain binary to the docker image
COPY ./artifacts/polkadot-parachain /usr/local/bin
# copy substrate-relay to the docker image
COPY --from=relay-builder /home/user/substrate-relay /usr/local/bin/
# we need bridges zombienet runner and tests
RUN	mkdir -p /home/nonroot/bridges-polkadot-sdk
COPY ./artifacts/bridges-polkadot-sdk /home/nonroot/bridges-polkadot-sdk
# also prepare `generate_hex_encoded_call` for running
RUN set -eux; \
	cd /home/nonroot/bridges-polkadot-sdk/cumulus/scripts/generate_hex_encoded_call; \
	npm install

# check if executable works in this container
USER nonroot
RUN /usr/local/bin/polkadot --version
RUN /usr/local/bin/polkadot-parachain --version
RUN /usr/local/bin/substrate-relay --version

# https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:{PORT}#/explorer
EXPOSE 9942 9910 8943 9945 9010 8945
