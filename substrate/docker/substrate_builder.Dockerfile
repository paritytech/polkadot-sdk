# This is the build stage for Substrate. Here we create the binary.
FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /substrate
COPY . /substrate

# this should not be necessary. instead this https://hub.docker.com/layers/paritytech/ci-linux
# should be updated to use a a newer version of Rust
# we don't want the old version of RUST_NIGHTLY=2023-05-23
RUN RUST_NIGHTLY="-2023-05-23" && \
	CARGO_HOME=/usr/local/cargo && \
	# uninstall old version of Rust to avoid conflicting rust installations
	rustup toolchain uninstall stable && \
	rustup toolchain uninstall "nightly${RUST_NIGHTLY}" && \
	rustup toolchain uninstall nightly && \
	# uninstall and reinstall git to avoid this error
	# == Info: GnuTLS recv error (-9): Error decoding the received TLS packet.
	apt-get -y remove --purge git && \
	apt-get -y update && \
	apt-get -y install git && \
	# reinstall Rust and use stable
	rustup toolchain install stable --profile minimal --component rustfmt && \
	rustup toolchain install nightly --profile minimal --component rustfmt && \
	rustup default stable && \
	rustup update && \
	rustup update nightly && \
	rustup target add wasm32-unknown-unknown --toolchain nightly && \
	rustup component add rust-src rustfmt clippy && \
	rustup target add wasm32-unknown-unknown && \
	# ln -s "/usr/local/rustup/toolchains/nightly${RUST_NIGHTLY}-x86_64-unknown-linux-gnu" /usr/local/rustup/toolchains/nightly-x86_64-unknown-linux-gnu && \
	echo "rustc version is:" && \
	rustc --version && \
	# attempts below to try to to fix `unexpected disconnect while reading sideband packet`
	# note: possibly due to poor connection
	# `error: RPC failed; curl 56 GnuTLS recv error (-9): Error decoding the received TLS packet`
	# https://stackoverflow.com/questions/66366582/github-unexpected-disconnect-while-reading-sideband-packet
	export GIT_TRACE_PACKET=1 && \
	export GIT_TRACE=1 && \
	export GIT_CURL_VERBOSE=1 && \
	git config --global pack.window 1 && \
	git config --global http.postBuffer 1048576000 && \
	git config --global https.postBuffer 1048576000 && \
	git config --global core.compression 0 && \
	git config --system core.longpaths true && \
	git config --global http.version HTTP/1.1 && \
	cargo install cargo-web wasm-pack cargo-deny cargo-spellcheck cargo-hack mdbook mdbook-mermaid mdbook-linkcheck mdbook-graphviz mdbook-last-changed && \
	cargo install cargo-nextest --locked && \
	cargo install diener --version 0.4.6 && \
	cargo install --version 0.2.73 wasm-bindgen-cli && \
	cargo install wasm-gc && \
	apt-get -y update && \
	apt-get install -y binutils-dev libunwind-dev libblocksruntime-dev && \
	cargo install honggfuzz && \
	rustup show && \
	cargo --version && \
	# don't do the below whilst testing since it takes so long to re-build
	#
	# apt-get autoremove -y && \
	# apt-get clean && \
	# rm -rf /var/lib/apt/lists/* && \
	# rm -rf "${CARGO_HOME}/registry" "${CARGO_HOME}/git" /root/.cache/sccache && \
	#
	# overcome error `network failure seems to have happened` by using `CARGO_NET_GIT_FETCH_WITH_CLI=true`
	# as mentioned here https://stackoverflow.com/questions/73738004/how-can-i-fix-unable-to-update-registry-network-failure-seems-to-have-happened
	CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build --locked --release \
	# -p polkadot \
	# -p polkadot-parachain-bin \
	# -p staging-node-cli \
	# -p staging-chain-spec-builder \
	# -p subkey \
	# -p solochain-template-node \
	-p parachain-template-node \
	# -p minimal-template-node \
	# -p test-parachain-adder-collator \
	# or just build all package binaries in workspace
	# cargo build --workspace --release && \
	&& ls -al /substrate/target/release/
# RUN cargo build --locked --release

# This is the 2nd stage: a very small image where we copy the Substrate binary."
FROM docker.io/library/ubuntu:20.04
LABEL description="Multistage Docker image for Substrate: a platform for web3" \
	io.parity.image.type="builder" \
	io.parity.image.authors="chevdor@gmail.com, devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.description="Substrate is a next-generation framework for blockchain innovation 🚀" \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/substrate/docker/substrate_builder.Dockerfile" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk"

# COPY --from=builder /substrate/target/release/substrate /usr/local/bin
# COPY --from=builder /substrate/target/release/subkey /usr/local/bin
# COPY --from=builder /substrate/target/release/node-template /usr/local/bin
# COPY --from=builder /substrate/target/release/chain-spec-builder /usr/local/bin
# COPY --from=builder /substrate/target/release/polkadot /usr/local/bin
# COPY --from=builder /substrate/target/release/polkadot-parachain /usr/local/bin
# COPY --from=builder /substrate/target/release/substrate-node /usr/local/bin
# COPY --from=builder /substrate/target/release/chain-spec-builder /usr/local/bin
# COPY --from=builder /substrate/target/release/subkey /usr/local/bin
# COPY --from=builder /substrate/target/release/solochain-template-node /usr/local/bin
COPY --from=builder /substrate/target/release/parachain-template-node /usr/local/bin
# COPY --from=builder /substrate/target/release/minimal-template-node /usr/local/bin
# COPY --from=builder /substrate/target/release/adder-collator /usr/local/bin

# polkadot						- OK
# polkadot-parachain-bin		- ?? - error[E0583]: file not found for module `target_chain`
# staging-node-cli				- OK
# staging-chain-spec-builder    - OK - not a dynamic executable / rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2 / Trace/breakpoint trap
# subkey						- OK - 36 mins - not a dynamic executable / rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2 / Trace/breakpoint trap
# solochain-template-node		- OK - not a dynamic executable / rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2 / Trace/breakpoint trap
# parachain-template-node
# minimal-template-node			- OK - not a dynamic executable / rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2 / Trace/breakpoint trap
# test-parachain-adder-collator - OK - not a dynamic executable / rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2 / Trace/breakpoint trap

RUN echo "running" && \
	# # polkadot
	# export PKG=polkadot && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# ldd /usr/local/bin/$PKG && \
	# /usr/local/bin/$PKG --version && \
	# # polkadot-parachain-bin / polkadot-parachain
	# export PKG=polkadot-parachain && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# ldd /usr/local/bin/$PKG && \
	# /usr/local/bin/$PKG --version && \
	# staging-node-cli / substrate-node
	# export PKG=substrate-node && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# ldd /usr/local/bin/$PKG && \
	# /usr/local/bin/$PKG --version
	# # staging-chain-spec-builder / chain-spec-builder
	# export PKG=chain-spec-builder && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# Sanity checks
	# note: do not use `ldd` with chain-spec-builder otherwise you get error `not a dynamic executable`
	# # ldd /usr/local/bin/$PKG && \
	# note: running the following generates error
	# ```
	# #13 0.167 rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2
	# #13 0.167  Trace/breakpoint trap
	# ```
	# /usr/local/bin/$PKG --version && \
	# # subkey
	# export PKG=subkey && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# # note: do not use `ldd` with subkey otherwise you get error `not a dynamic executable`
	# # ldd /usr/local/bin/$PKG && \
	# /usr/local/bin/$PKG --version && \
	# # solochain-template-node
	# export PKG=solochain-template-node && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# # note: do not use `ldd` with subkey otherwise you get error `not a dynamic executable`
	# # ldd /usr/local/bin/$PKG && \
	# # TODO - restore the below once solve rosetta error
	# /usr/local/bin/$PKG --version && \
	# parachain-template-node
	export PKG=parachain-template-node && \
	echo "linking package name ${PKG}" && \
	useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	mkdir -p /data /$PKG/.local/share/$PKG && \
	chown -R sdk-user:sdk-user /data && \
	ln -s /data /$PKG/.local/share/$PKG && \
	ls -al /data && \
	# Sanity checks
	# # note: do not use `ldd` with subkey otherwise you get error `not a dynamic executable`
	# ldd /usr/local/bin/$PKG && \
	/usr/local/bin/$PKG --version
	# # minimal-template-node
	# export PKG=minimal-template-node && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# # note: do not use `ldd` with subkey otherwise you get error `not a dynamic executable`
	# # ldd /usr/local/bin/$PKG
	# /usr/local/bin/$PKG --version
	# TODO - fix ordering of above and below rows
	# # test-parachain-adder-collator / adder-collator
	# export PKG=adder-collator && \
	# echo "linking package name ${PKG}" && \
	# useradd -m -u 1000 -U -s /bin/sh -d /$PKG sdk-user && \
	# mkdir -p /data /$PKG/.local/share/$PKG && \
	# chown -R sdk-user:sdk-user /data && \
	# ln -s /data /$PKG/.local/share/$PKG && \
	# ls -al /data && \
	# # Sanity checks
	# # ldd /usr/local/bin/$PKG && \
	# /usr/local/bin/$PKG --version
	# # unclutter and minimize the attack surface
	# # rm -rf /usr/bin /usr/sbin

USER sdk-user
EXPOSE 30333 9933 9944 9615
VOLUME ["/data"]
