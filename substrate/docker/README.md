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

> Note: `polkadot-parachain` from package "polkadot-parachain" is NOT currently supported due to this error https://github.com/paritytech/polkadot-sdk/issues/4448.

First, install [Docker](https://docs.docker.com/get-docker/).

Then to generate the latest `parity/substrate` image. Please run:
```sh
export DOCKER_DEFAULT_PLATFORM=linux/amd64
./build.sh
```

> Important: The build.sh script is configured to default to `linux/amd64` platform architecture. If you are using Apple Silicon then it will use `linux/x86_64`. If you are using a different platform architecture then please specify it for the value of `DOCKER_DEFAULT_PLATFORM`.

> Note: If you are in the root directory of the Polkadot SDK please run `./substrate/docker/build.sh`

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
  -lsync=debug
```

> Note: It may not let you run with `--telemetry-url "wss://telemetry.polkadot.io/submit/ 0"` in the above command unless you configure `docker run` to run using the option `-d`.

> Note: It is recommended to provide a custom `--base-path` to store the chain database. For example:

```sh
# Run Substrate Solo Node Template without re-compiling
./run.sh solochain-template-node --dev --rpc-external --base-path=/data
```

> To print logs follow the [Substrate debugging instructions](https://docs.substrate.io/test/debug/).

```sh
# Purge the local dev chain
./run.sh solochain-template-node purge-chain --dev --base-path=/data -y
```

> Important: The run.sh script is configured to default to `linux/amd64` platform architecture. If you are using Apple Silicon then it will use `linux/x86_64`. If you are using a different platform architecture then please specify it for the value of `DOCKER_DEFAULT_PLATFORM` and run `export DOCKER_DEFAULT_PLATFORM=xxx` prior to the above commands, replacig `xxx` with the value of your platform architecture.

> Note: If you run a chain with the run.sh script within the Docker container, chain syncing will utilize all available memory and CPU power as mentioned in /substrate/primitives/runtime/docs/contributor/docker.md, so it may be important to provide options to `docker run` to configure `--memory`, `--memory-reservation`, `--memory-swap`, and `--cpus` values to limit resources used. To run the commands within the Docker container itself instead of from the host machine, run the Docker container in the background in detached mode with `-d` (e.g. `docker run --platform $PLATFORM -it -d parity/substrate`) and then enter that Docker container with `docker exec -it parity-substrate /bin/bash` (where the Docker container name is `parity-substrate`, whereas the Docker image name is `parity/substrate`). If you want the Docker container to restart on failure then provide to `docker run` the option `--restart "on-failure"` instead of `--rm`. If you wish to stop and remove the Docker container that you can view running with `docker ps -a` then run `docker stop parity-substrate && docker rm parity-substrate`.

> Note: If you get error `Bind for 0.0.0.0:30333 failed: port is already allocated.` or similar, then you might be running a different Docker container that is using the ports you need. To resolve that either find that other container with `docker ps -a` and stop that other container with `docker stop <CONTAINER_ID>` or change the host side ports used in run.sh. For example, if you changed `--publish 0.0.0.0:9944:9944` to `--publish 0.0.0.0:9955:9944` where `9955` is an available port on the host machine, then if you wanted to connect to the node running in the Docker container using Polkadot.js Apps at https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9944, then you would have to change the port in that URL to `9955`.

> Note: In the [.Dockerfile](./substrate_builder.Dockerfile), the exposed ports are for running a Substrate-based node. In addition, ports 80 and 443 have been included incase you wishes to run a frontend from within the Docker container.
