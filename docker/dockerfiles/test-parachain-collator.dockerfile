# This file is sourced from https://github.com/paritytech/polkadot-sdk/blob/master/docker/dockerfiles/test-parachain-collator.Dockerfile
FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /cumulus
COPY . /cumulus

RUN cargo build --release --locked -p polkadot-parachain

# the collator stage is normally built once, cached, and then ignored, but can
# be specified with the --target build flag. This adds some extra tooling to the
# image, which is required for a launcher script. The script simply adds two
# arguments to the list passed in:
#
#   --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/PEER_ID
#
# with the appropriate ip and ID for both Alice and Bob
FROM debian:buster-slim as collator
RUN apt-get update && apt-get install jq curl bash -y && \
    curl -sSo /wait-for-it.sh https://raw.githubusercontent.com/vishnubob/wait-for-it/master/wait-for-it.sh && \
    chmod +x /wait-for-it.sh && \
    curl -sL https://deb.nodesource.com/setup_12.x | bash - && \
    apt-get install -y nodejs && \
    npm install --global yarn && \
    yarn global add @polkadot/api-cli@0.10.0-beta.14
COPY --from=builder \
    /paritytech/cumulus/target/release/polkadot-parachain /usr/bin
COPY ./docker/scripts/inject_bootnodes.sh /usr/bin
CMD ["/usr/bin/inject_bootnodes.sh"]
COPY ./docker/scripts/healthcheck.sh /usr/bin/
HEALTHCHECK --interval=300s --timeout=75s --start-period=30s --retries=3 \
    CMD ["/usr/bin/healthcheck.sh"]

# the runtime stage is normally built once, cached, and ignored, but can be
# specified with the --target build flag. This just preserves one of the builder's
# outputs, which can then be moved into a volume at runtime
FROM debian:buster-slim as runtime
COPY --from=builder \
    /paritytech/cumulus/target/release/wbuild/cumulus-test-parachain-runtime/cumulus_test_parachain_runtime.compact.wasm \
    /var/opt/
CMD ["cp", "-v", "/var/opt/cumulus_test_parachain_runtime.compact.wasm", "/runtime/"]

FROM debian:buster-slim
COPY --from=builder \
    /paritytech/cumulus/target/release/polkadot-parachain /usr/bin

CMD ["/usr/bin/polkadot-parachain"]
