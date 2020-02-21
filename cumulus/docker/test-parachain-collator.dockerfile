FROM rust:buster as builder

RUN apt-get update && apt-get install time clang libclang-dev llvm -y
RUN rustup toolchain install nightly
RUN rustup target add wasm32-unknown-unknown --toolchain nightly
RUN command -v wasm-gc || cargo +nightly install --git https://github.com/alexcrichton/wasm-gc --force

WORKDIR /paritytech/cumulus

# Ideally, we could just do something like `COPY . .`, but that doesn't work:
# it busts the cache every time non-source files like inject_bootnodes.sh change,
# as well as when non-`.dockerignore`'d transient files (*.log and friends)
# show up. There is no way to exclude particular files, or write a negative
# rule, using Docker's COPY syntax, which derives from go's filepath.Match rules.
#
# We can't combine these into a single big COPY operation like
# `COPY collator consensus network runtime test Cargo.* .`, because in that case
# docker will copy the _contents_ of each directory into the image workdir,
# not the actual directory. We're stuck just enumerating them.
COPY . .

RUN cargo build --release -p cumulus-test-parachain-collator

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
    /paritytech/cumulus/target/release/cumulus-test-parachain-collator /usr/bin
COPY ./scripts/inject_bootnodes.sh /usr/bin
CMD ["/usr/bin/inject_bootnodes.sh"]
COPY ./scripts/healthcheck.sh /usr/bin/
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
    /paritytech/cumulus/target/release/cumulus-test-parachain-collator /usr/bin

CMD ["/usr/bin/cumulus-test-parachain-collator"]
