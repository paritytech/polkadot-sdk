FROM docker.io/library/ubuntu:24.04

# metadata
ARG VCS_REF
ARG BUILD_DATE
ARG IMAGE_NAME

LABEL io.parity.image.authors="devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.title="${IMAGE_NAME}" \
	io.parity.image.description="Subkey: key generating utility for Substrate." \
	io.parity.image.source="https://github.com/paritytech/substrate/blob/${VCS_REF}/scripts/ci/docker/subkey.Dockerfile" \
	io.parity.image.revision="${VCS_REF}" \
	io.parity.image.created="${BUILD_DATE}" \
	io.parity.image.documentation="https://github.com/paritytech/substrate/tree/${VCS_REF}/subkey"

# show backtraces
ENV RUST_BACKTRACE 1

# add user
# ubuntu:24.04 ships a default 'ubuntu' user that occupies uid/gid 1000
RUN userdel -r ubuntu 2>/dev/null || true; \
	useradd -m -u 1000 -U -s /bin/sh -d /subkey subkey

# add subkey binary to docker image
COPY ./subkey /usr/local/bin

USER subkey

# check if executable works in this container
RUN /usr/local/bin/subkey --version

ENTRYPOINT ["/usr/local/bin/subkey"]
