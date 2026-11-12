# The polkadot release image built from injected binaries, for the platforms the
# deb package is not published for. It is published under the same tags as
# polkadot_injected_debian.Dockerfile, so it keeps the layout of the deb package:
# same paths, same user and same entrypoint.
FROM docker.io/paritytech/base-bin

# metadata
ARG VCS_REF
ARG BUILD_DATE

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="parity/polkadot" \
	io.parity.image.description="Polkadot: a platform for web3. This is the official Parity image with an injected binary." \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/docker/dockerfiles/polkadot/polkadot_injected_release_binary.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk/"

USER root

# show backtraces
ENV RUST_BACKTRACE 1

# add the polkadot and polkadot-*-worker binaries where the deb package installs them
COPY bin/polkadot /usr/bin/polkadot
COPY bin/polkadot-execute-worker bin/polkadot-prepare-worker /usr/lib/polkadot/

RUN chmod a+rx /usr/bin/polkadot /usr/lib/polkadot/polkadot-*-worker; \
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
