# Builds images used by the bridge using locally built binaries.
#
# In particular, it can be used to build Substrate nodes and bridge relayers. The binary that gets
# built can be specified with the `PROJECT` build-arg. For example, to build the `substrate-relay`
# you would do the following:
#
# `docker build . -f local.Dockerfile -t local/substrate-relay --build-arg=PROJECT=substrate-relay`
#
# See the `deployments/README.md` for all the available `PROJECT` values.
#
# You may use `scripts/build-containers.sh` to build all binaries and images at once.

# This image needs to be binary compatible with the host machine (where images are built).
ARG UBUNTU_RELEASE=20.04
FROM docker.io/library/ubuntu:${UBUNTU_RELEASE} as runtime

USER root
WORKDIR /home/root

# show backtraces
ENV RUST_BACKTRACE 1
ENV DEBIAN_FRONTEND=noninteractive

RUN set -eux; \
	apt-get update && \
	apt-get install -y --no-install-recommends \
        curl ca-certificates libssl-dev && \
    update-ca-certificates && \
	groupadd -g 1001 user && \
	useradd -u 1001 -g user -s /bin/sh -m user && \
	# apt clean up
	apt-get autoremove -y && \
	apt-get clean && \
	rm -rf /var/lib/apt/lists/*

# switch to non-root user
USER user

WORKDIR /home/user

ARG PROFILE=release
ARG PROJECT=substrate-relay

COPY --chown=user:user ./target/${PROFILE}/${PROJECT}* ./

# check if executable works in this container
RUN ./${PROJECT} --version

ENV PROJECT=$PROJECT
ENTRYPOINT ["/bin/sh"]