FROM debian:bullseye-slim

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG IMAGE_NAME

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="${IMAGE_NAME}" \
	io.parity.image.description="Statement store benchmarking tools" \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/docker/dockerfiles/statement-store-tools_injected.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk/"

# show backtraces
ENV RUST_BACKTRACE 1

# install tools and dependencies
RUN apt-get update && \
	DEBIAN_FRONTEND=noninteractive apt-get install -y \
    ca-certificates \
    curl \
    libssl1.1 \
    tini && \
# apt cleanup
	apt-get autoremove -y && \
	apt-get clean && \
	find /var/lib/apt/lists/ -type f -not -name lock -delete; \
# add user
  groupadd --gid 10000 nonroot && \
  useradd  --home-dir /home/nonroot \
           --create-home \
           --shell /bin/bash \
           --gid nonroot \
           --groups nonroot \
           --uid 10000 nonroot


# add statement-store-tools binaries to docker image
COPY ./artifacts/statement-latency-bench ./artifacts/setup-allowances /usr/local/bin

USER nonroot

# check if executables work in this container
RUN /usr/local/bin/statement-latency-bench --help >/dev/null && \
	/usr/local/bin/setup-allowances --help >/dev/null

# Tini allows us to avoid several Docker edge cases, see https://github.com/krallin/tini.
ENTRYPOINT ["tini", "--", "/bin/bash"]
