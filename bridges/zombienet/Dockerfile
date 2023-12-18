# build me from the project root using
# docker build -f bridges/zombienet/Dockerfile ./target/release

ARG DEBIAN_CODENAME=bullseye

FROM paritytech/substrate-relay:v2023-11-07-rococo-westend-initial-relayer as relay
FROM paritypr/polkadot-debug:4ccd3fae58c406c2698198075d5e3c65d95708ee as polkadot
FROM paritypr/polkadot-parachain-debug:07127dea7fc0dcceca0c14e2f7081625eb24333b as polkadot-parachain

FROM docker.io/paritytech/zombienet:v1.3.79

USER root
WORKDIR /home/root

# install tools and dependencies
RUN set -eux; \
    apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 15CF4D18AF4F7421; \
	apt-get -y update; \
    apt-get install -y --no-install-recommends \
        libssl-dev make cmake graphviz \
        git pkg-config curl time rhash ca-certificates jq \
        python3 python3-pip lsof ruby ruby-bundler git-restore-mtime xz-utils zstd unzip gnupg protobuf-compiler && \
# add clang 14 repo
    echo "deb http://apt.llvm.org/bullseye/ llvm-toolchain-bullseye-14 main" >> /etc/apt/sources.list.d/llvm-toolchain-bullseye-14.list; \
    echo "deb-src http://apt.llvm.org/bullseye/ llvm-toolchain-bullseye-14 main" >> /etc/apt/sources.list.d/llvm-toolchain-bullseye-14.list; \
    apt-get -y update; \
    apt-get install -y --no-install-recommends \
        clang-14 lldb-14 lld-14 libclang-14-dev


COPY --from=relay /home/user/substrate-relay /home/nonroot/local_bridge_testing/bin/substrate-relay
COPY --from=polkadot /usr/local/bin/polkadot /home/nonroot/polkadot-sdk/target/release/polkadot
COPY --from=polkadot /usr/local/bin/polkadot-execute-worker /home/nonroot/polkadot-sdk/target/release/polkadot-execute-worker
COPY --from=polkadot /usr/local/bin/polkadot-prepare-worker /home/nonroot/polkadot-sdk/target/release/polkadot-prepare-worker
COPY --from=polkadot-parachain /usr/local/bin/polkadot-parachain /home/nonroot/polkadot-sdk/target/release/polkadot-parachain

#COPY artifacts/polkadot-parachain /home/nonroot/polkadot-sdk/target/release/polkadot-parachain
#COPY artifacts/polkadot /home/nonroot/polkadot-sdk/target/release/polkadot

COPY cumulus/scripts /home/nonroot/polkadot-sdk/cumulus/scripts
COPY cumulus/zombienet /home/nonroot/polkadot-sdk/cumulus/zombienet

COPY helpers /home/nonroot/polkadot-sdk/bridges/zombienet/helpers
COPY scripts /home/nonroot/polkadot-sdk/bridges/zombienet/scripts
COPY tests /home/nonroot/polkadot-sdk/bridges/zombienet/tests
COPY run-tests.sh /home/nonroot/polkadot-sdk/bridges/zombienet

# https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:{PORT}#/explorer
EXPOSE 9942 9910 8943 9945 9010 8945

USER nonroot

#ENTRYPOINT ["/home/nonroot/polkadot-sdk/target/release/polkadot"]
#ENTRYPOINT ["ls", "-la", "/usr/local/bin/"]
ENTRYPOINT ["/home/nonroot/polkadot-sdk/bridges/zombienet/run-tests.sh"]
