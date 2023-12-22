---
description: Set up a development environment and run the end to end test stack.
---

# Quick Start Guide

### System Requirements

* Ubuntu 22.04 LTS (Ubuntu 20.04 LTS should also work)

### Development Tools

* Utilities (`jq`, `direnv`, `sponge`, `gcc`, `g++`, `build-essential`)

```bash
sudo apt install jq direnv moreutils gcc g++ build-essential
```

Install hooks for `direnv`. Change this if you are using a different shell.

```bash
direnv hook bash >> .bashrc
source .bashrc
```

* Install https://github.com/nvm-sh/nvm

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.2/install.sh | bash
```

* Install Node

```bash
cd core && nvm use
```

* Install pnpm ([https://pnpm.io/](https://pnpm.io/))

```bash
corepack enable
corepack prepare pnpm@7.14.2 --activate
```

* Rust ([https://docs.substrate.io/install/linux/](https://docs.substrate.io/install/linux/))

```bash
sudo apt install -y git clang curl libssl-dev llvm libudev-dev make protobuf-compiler

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

rustup default stable
rustup update
rustup update nightly
rustup target add wasm32-unknown-unknown --toolchain nightly
```

* Typos ([https://crates.io/crates/typos-cli#install](https://crates.io/crates/typos-cli#install))

```bash
cargo install typos-cli
```

* Golang ([https://go.dev/doc/install](https://go.dev/doc/install))

```bash
curl -LO https://go.dev/dl/go1.19.3.linux-amd64.tar.gz

sudo rm -rf /usr/local/go
sudo tar -C /usr/local -xzf go1.19.3.linux-amd64.tar.gz

# Add to ~/.profile to persist
export PATH=$PATH:/usr/local/go/bin:$HOME/go/bin
```

* Mage and Revive ([https://magefile.org/](https://magefile.org/), [https://github.com/mgechev/revive#installation](https://github.com/mgechev/revive#installation))

```bash
go install github.com/magefile/mage@latest
go install github.com/mgechev/revive@master
```

* Geth ([https://geth.ethereum.org/docs/install-and-build/installing-geth](https://geth.ethereum.org/docs/install-and-build/installing-geth))

```bash
go install github.com/ethereum/go-ethereum/cmd/geth@latest
```

### Setup

This guide uses the root of the `$HOME/` folder for all source code.

1.  Clone the https://github.com/paritytech/polkadot repo.

    ```bash
    git clone -n https://github.com/paritytech/polkadot.git
    cd polkadot
    git checkout v0.9.30
    cargo build --release
    ```
2.  Clone the https://github.com/Snowfork/snowbridge repo.

    ```bash
    git clone https://github.com/Snowfork/snowbridge.git
    ```
3.  `yarn` install dependencies.

    ```bash
    cd snowbridge
    (cd core && pnpm install)
    ```
4.  Edit `.envrc` and `direnv allow`

    In the `web/packages/test` subfolder of the `snowbridge` repo copy the envrc-example.

    ```bash
    cp .envrc-example .envrc
    ```

    Modify the `POLKADOT_BIN` variable in `.envrc` to point to the `polkadot` binary. If you have checked out all source code to the `$HOME` folder you can use the relative path below: `export POLKADOT_BIN=../../../../polkadot/target/release/polkadot`

    Allow the variables to be automatically loaded by `direnv`

    ```bash
    direnv allow
    ```

    In the `contracts` subfolder of the `snowbridge` repo copy the envrc-example. Here we do not need to edit the `.envrc` as defaults are set.

    ```bash
    cp .envrc-example .envrc
    direnv allow
    ```

### Running the E2E stack

1. Start up the local E2E test stack

In a separate terminal change directory to the `web/packages/test` subfolder of the `snowbridge` repo. Run `start-services.sh` script to start the bridge.

```bash
scripts/start-services.sh
```

This script will:

1. Launch a local ethereum node (Geth & Lodestar)
2. Deploy contracts
3. Build and start the Snowbridge parachain
4. Configure the bridge
5. Start the relayers.

When this is complete `Testnet has been initialized` will be printed to the terminal. The bridge will continue to run until cancelled by `Ctrl+C` to kill the `start-services.sh` script.

1.  Bootstrap the bridge.

    The bridge requires a certain amount of funds (SnowDOT and SnowETH) in order for Incentivized channels to be used. The bootstrap process are the first two test cases and needs to be run before other tests will pass.

    In the `web/packages/test` subfolder of the `snowbridge` repo run the bootstrap tests:

    ```bash
    pnpm test:integration test/bootstrap.js
    ```
2.  Run any single test or all tests.

    To run all tests:

    ```bash
    pnpm test:integration
    ```

    To run a single test:

    ```bash
    pnpm test:integration --grep 'should transfer ETH from Substrate to Ethereum \(incentivized channel\)'
    ```

    ####

## Inspecting the E2E environment

1.  Ethereum

    The ethereum data directory is `/tmp/snowbridge/geth`.

    The ethereum log file is `/tmp/snowbridge/geth.log`.

    The Lodestar log file is `/tmp/snowbridge/lodestar.log`.
2.  Relaychain

    The relay chain log files are in the `web/packages/test` subdirectory of the `snowbridge` repo. `alice.log`, `bob.log`, `charlie.log`

    The relay chain can be accessed via the polkadot.js web using the following url:

    [https://polkadot.js.org/apps/?rpc=ws%3A%2F%127.0.0.1%3A9944#/explorer](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2Flocalhost%3A9944#/explorer)
3.  Parachain

    The Snowbridge parachain log files are in the `web/packages/test` subdirectory of the `snowbridge` repo. `11144.log`, `11155.log`

    The Snowbridge parachain can be accessed via the polkadot.js web using the following url:

    [https://polkadot.js.org/apps/?rpc=ws%3A%2F%127.0.0.1%3A11144#/explorer](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2Flocalhost%3A11144#/explorer)
4.  Test Parachain

    The third-party test parachain log files are in the `web/packages/test` subdirectory of the `snowbridge` repo. `13144.log`, `13155.log`

    The Snowbridge Test parachain can be accessed via the polkadot.js web using the following url:

    [https://polkadot.js.org/apps/?rpc=ws%3A%2F%127.0.0.1%3A13144#/explorer](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2Flocalhost%3A13144#/explorer)
5. Relayers

The relayers log files can be found in the `web/packages/test` subdirectory of the `snowbridge` repo.

* `beacon-relay.log`
* `parachain-relay.log`
* `beefy-relay.log`

The`start-services.sh` script will automatically restart the relayer processes if they exit and print to the terminal. Seeing a relayer restart constantly is a sign that something might be wrong with your environment. Grepping the relayer logs will help pin point the issue.
