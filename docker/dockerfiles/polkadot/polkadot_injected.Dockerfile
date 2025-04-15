FROM docker.io/paritytech/base-bin

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG IMAGE_NAME
# That can be a single one or a comma separated list
ARG BINARY=polkadot

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="parity/polkadot" \
	io.parity.image.description="Polkadot: a platform for web3. This is the official Parity image with an injected binary." \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/docker/dockerfiles/polkadot/polkadot_injected.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk/"

# show backtraces
ENV RUST_BACKTRACE 1

USER root
WORKDIR /app

# add polkadot and polkadot-*-worker binaries to the docker image
COPY bin/* /usr/local/bin/
COPY entrypoint.sh .


RUN chmod -R a+rx "/usr/local/bin"; \
		mkdir -p /data /polkadot/.local/share && \
		chown -R parity:parity /data && \
		ln -s /data /polkadot/.local/share/polkadot

USER parity

# check if executable works in this container
RUN /usr/local/bin/polkadot --version
RUN /usr/local/bin/polkadot-prepare-worker --version
RUN /usr/local/bin/polkadot-execute-worker --version


EXPOSE 30333 9933 9944 9615
VOLUME ["/polkadot"]

ENV BINARY=${BINARY}

# ENTRYPOINT
ENTRYPOINT ["/app/entrypoint.sh"]

# We call the help by default
CMD ["--help"]
