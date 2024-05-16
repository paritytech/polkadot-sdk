# Substrate Builder Docker Image

The Docker image in this folder is a `builder` image. It is self contained and allows users to build the binaries
themselves. There is no requirement on having Rust or any other toolchain installed but a working Docker environment.

Unlike the `parity/polkadot` image which contains a single binary (`polkadot`!) used by default, the image in this
folder builds and contains several binaries and you need to provide the name of the binary to be called.

You should refer to the [.Dockerfile](./substrate_builder.Dockerfile) for the actual list. At the time of editing, the list of included binaries is:

- `polkadot` from package "polkadot"
- `substrate-node` from package "staging-node-cli" (previously `substrate`)
- `chain-spec-builder` from package "staging-chain-spec-builder"
- `subkey` from package "subkey"
- `solochain-template-node` from package "solochain-template-node" (previously `node-template`)
- `minimal-template-node` from package "minimal-template-node"
- `parachain-template-node` from package "parachain-template-node"
- `adder-collator` from package "test-parachain-adder-collator"

Note: `polkadot-parachain` from package "polkadot-parachain" is NOT currently supported due to this error https://github.com/paritytech/polkadot-sdk/issues/4448.

First, install [Docker](https://docs.docker.com/get-docker/).

Then to generate the latest `parity/substrate` image. Please run:
```sh
export DOCKER_DEFAULT_PLATFORM=linux/amd64
./build.sh
```

> IMPORTANT: The build.sh script is configured to default to `linux/amd64` platform architecture. If you are using Apple Silicon then it will use `linux/x86_64`. If you are using a different platform architecture then please specify it for the value of `DOCKER_DEFAULT_PLATFORM`.

> NOTE: If you are in the root directory of the Polkadot SDK please run `./substrate/docker/build.sh`

If you wish to create a debug build rather than a production build, then you may modify the
[.Dockerfile](./substrate_builder.Dockerfile) replacing `cargo build --locked --release` with just
`cargo build --locked` and replacing `target/release` with `target/debug`.

If you get an error that a tcp port address is already in use then find an available port to use for the host port in the [.Dockerfile](./substrate_builder.Dockerfile).

The image can be used by passing the selected binary followed by the appropriate tags for this binary.

Your best guess to get started is to pass the `--help flag`. Here are a few examples:

- `./run.sh substrate-node --version`
- `./run.sh subkey --help`
- `./run.sh solochain-template-node --version`
- `./run.sh minimal-template-node --version`
- `./run.sh chain-spec-builder --help`
- `./run.sh parachain-template-node --version`
- `./run.sh adder-collator --help`

Then try running the following command to start a single node development chain using the Substrate Node Template binary
`solochain-template-node`:

```sh
./run.sh solochain-template-node \
  --dev \
  --name "my-template-node" \
  --base-path=/data \
  --rpc-external \
  --rpc-methods Unsafe \
  --unsafe-rpc-external \
  --rpc-cors all \
  --prometheus-external \
  # FIXME: why doesn't this work
  # --telemetry-url "wss://telemetry.polkadot.io/submit/ 0" \
  -lsync=debug
```

Note: It is recommended to provide a custom `--base-path` to store the chain database. For example:

```sh
# Run Substrate Solo Node Template without re-compiling
./run.sh solochain-template-node --dev --rpc-external --base-path=/data
```

> To print logs follow the [Substrate debugging instructions](https://docs.substrate.io/test/debug/).

```sh
# Purge the local dev chain
./run.sh solochain-template-node purge-chain --dev --base-path=/data -y
```

> IMPORTANT: The run.sh script is configured to default to `linux/amd64` platform architecture. If you are using Apple Silicon then it will use `linux/x86_64`. If you are using a different platform architecture then please specify it for the value of `DOCKER_DEFAULT_PLATFORM` and run `export DOCKER_DEFAULT_PLATFORM=xxx` prior to the above commands, replacig `xxx` with the value of your platform architecture.
