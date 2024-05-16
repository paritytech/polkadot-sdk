# This is the build stage for Substrate. Here we create the binary.
FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /substrate
COPY . /substrate

# this should not be necessary. instead this https://hub.docker.com/r/paritytech/ci-linux
# should be updated to use a a newer version of Rust since we don't want the old version of RUST_NIGHTLY=2023-05-23
RUN RUST_NIGHTLY="-2023-05-23" && \
	CARGO_HOME=/usr/local/cargo && \
	# uninstall old version of Rust to avoid conflicting rust installations
	rustup toolchain uninstall stable && \
	rustup toolchain uninstall "nightly${RUST_NIGHTLY}" && \
	rustup toolchain uninstall nightly && \
	# uninstall and reinstall git to avoid encountering this error
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
	echo "rustc version is:" && \
	rustc --version && \
	# below are attempts to try to to fix `unexpected disconnect while reading sideband packet` that may occur due to poor connection
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
	# additional dependencies that may not actually be necessary
	cargo install cargo-web wasm-pack cargo-deny cargo-spellcheck cargo-hack mdbook mdbook-mermaid mdbook-linkcheck mdbook-graphviz mdbook-last-changed && \
	cargo install cargo-nextest --locked && \
	cargo install diener --version 0.4.6 && \
	cargo install --version 0.2.73 wasm-bindgen-cli && \
	cargo install wasm-gc && \
	apt-get -y update && \
	apt-get install -y binutils-dev libunwind-dev libblocksruntime-dev && \
	cargo install honggfuzz && \
	###
	rustup show && \
	cargo --version && \
	# # don't do the below since it takes ~45 mins to re-build and users may want them in the container too
	# apt-get autoremove -y && \
	# apt-get clean && \
	# rm -rf /var/lib/apt/lists/* && \
	# rm -rf "${CARGO_HOME}/registry" "${CARGO_HOME}/git" /root/.cache/sccache && \
	#
	# overcome error `network failure seems to have happened` by using `CARGO_NET_GIT_FETCH_WITH_CLI=true`
	# as mentioned here https://stackoverflow.com/questions/73738004/how-can-i-fix-unable-to-update-registry-network-failure-seems-to-have-happened
	#
	# alternatively just build all package binaries in workspace with `cargo build --workspace --release`
	# however you may encounter the error with `polkadot-parachain-bin`
	CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build --locked --release \
	-p polkadot \
	# # do not use `polkadot-parachain-bin` since it generates error `file not found for module target_chain`
	# -p polkadot-parachain-bin \
	-p staging-node-cli \
	-p staging-chain-spec-builder \
	-p subkey \
	-p solochain-template-node \
	-p parachain-template-node \
	-p minimal-template-node \
	-p test-parachain-adder-collator \
	# show what binaries were generated
	&& ls -al /substrate/target/release/

# This is the 2nd stage: a very small image where we copy the Substrate binary."
FROM docker.io/library/ubuntu:20.04
LABEL description="Multistage Docker image for Substrate: a platform for web3" \
	io.parity.image.type="builder" \
	io.parity.image.authors="chevdor@gmail.com, devops-team@parity.io" \
	io.parity.image.vendor="Parity Technologies" \
	io.parity.image.description="Substrate is a next-generation framework for blockchain innovation 🚀" \
	io.parity.image.source="https://github.com/paritytech/polkadot-sdk/blob/${VCS_REF}/substrate/docker/substrate_builder.Dockerfile" \
	io.parity.image.documentation="https://github.com/paritytech/polkadot-sdk"

COPY --from=builder /substrate/target/release/polkadot /usr/local/bin
# # do not use `polkadot-parachain-bin` since it generates error `file not found for module target_chain`
# COPY --from=builder /substrate/target/release/polkadot-parachain /usr/local/bin
COPY --from=builder /substrate/target/release/substrate-node /usr/local/bin
COPY --from=builder /substrate/target/release/chain-spec-builder /usr/local/bin
COPY --from=builder /substrate/target/release/subkey /usr/local/bin
COPY --from=builder /substrate/target/release/solochain-template-node /usr/local/bin
COPY --from=builder /substrate/target/release/parachain-template-node /usr/local/bin
COPY --from=builder /substrate/target/release/minimal-template-node /usr/local/bin
COPY --from=builder /substrate/target/release/adder-collator /usr/local/bin

ENV SDK_USER=sdk-user

RUN echo "configuring binaries" && \
	# polkadot
	export PKG=polkadot && \
	# add non-root user and continue if exists
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	# link ~/.local/share/$SDK_USER to /data
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	# Sanity checks
	ldd /usr/local/bin/$PKG && \
	/usr/local/bin/$PKG --version && \
	# # do not use `polkadot-parachain-bin` since it generates error `file not found for module target_chain`
	# # polkadot-parachain-bin / polkadot-parachain
	# export PKG=polkadot-parachain && \
	# id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	# mkdir -p /data /$SDK_USER/.local/share && \
	# chown -R $SDK_USER:$SDK_USER /data && \
	# ln -s /data /$SDK_USER/.local/share/$PKG && \
	# # Sanity checks
	# /usr/local/bin/$PKG --version && \
	#
	# staging-node-cli / substrate-node
	export PKG=substrate-node && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	# Sanity checks
	ldd /usr/local/bin/$PKG && \
	/usr/local/bin/$PKG --version && \
	# staging-chain-spec-builder / chain-spec-builder
	export PKG=chain-spec-builder && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	/usr/local/bin/$PKG --help && \
	# subkey
	export PKG=subkey && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	/usr/local/bin/$PKG --version && \
	# solochain-template-node
	export PKG=solochain-template-node && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	/usr/local/bin/$PKG --version && \
	# parachain-template-node
	export PKG=parachain-template-node && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	/usr/local/bin/$PKG --version && \
	# minimal-template-node
	export PKG=minimal-template-node && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	/usr/local/bin/$PKG --version && \
	# test-parachain-adder-collator / adder-collator
	export PKG=adder-collator && \
	id -u root || useradd -m -u 1000 -U -s /bin/sh -d /$SDK_USER $SDK_USER && \
	mkdir -p /data /$SDK_USER/.local/share && \
	chown -R $SDK_USER:$SDK_USER /data && \
	ln -s /data /$SDK_USER/.local/share/$PKG && \
	/usr/local/bin/$PKG --help
	# # don't do the below since it takes ~45 mins to re-build and users may want them in the container too
	# # unclutter and minimize the attack surface
	# # rm -rf /usr/bin /usr/sbin

USER $SDK_USER
EXPOSE 30333 9933 9944 9615 443 80
VOLUME ["/data"]
