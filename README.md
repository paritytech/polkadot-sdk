# Parity Bridges Common

This is a collection of components for building bridges.

These components include Substrate pallets for syncing headers, passing arbitrary messages, as well as libraries for
building relayers to provide cross-chain communication capabilities.

Three bridge nodes are also available. The nodes can be used to run test networks which bridge other Substrate chains.

🚧 The bridges are currently under construction - a hardhat is recommended beyond this point 🚧

## Contents

- [Installation](#installation)
- [High-Level Architecture](#high-level-architecture)
- [Project Layout](#project-layout)
- [Running the Bridge](#running-the-bridge)
- [How to send a message](#how-to-send-a-message)
- [Community](#community)

## Installation

To get up and running you need both stable and nightly Rust. Rust nightly is used to build the Web Assembly (WASM)
runtime for the node. You can configure the WASM support as so:

```bash
rustup install nightly
rustup target add wasm32-unknown-unknown --toolchain nightly
```

Once this is configured you can build and test the repo as follows:

```
git clone https://github.com/paritytech/parity-bridges-common.git
cd parity-bridges-common
cargo build --all
cargo test --all
```

Also you can build the repo with [Parity CI Docker
image](https://github.com/paritytech/scripts/tree/master/dockerfiles/ci-unified):

```bash
docker pull paritytech/ci-unified:latest
mkdir ~/cache
chown 1000:1000 ~/cache #processes in the container runs as "nonroot" user with UID 1000
docker run --rm -it -w /shellhere/parity-bridges-common \
                    -v /home/$(whoami)/cache/:/cache/    \
                    -v "$(pwd)":/shellhere/parity-bridges-common \
                    -e CARGO_HOME=/cache/cargo/ \
                    -e SCCACHE_DIR=/cache/sccache/ \
                    -e CARGO_TARGET_DIR=/cache/target/  paritytech/ci-unified:latest cargo build --all
#artifacts can be found in ~/cache/target
```

If you want to reproduce other steps of CI process you can use the following
[guide](https://github.com/paritytech/scripts#reproduce-ci-locally).

If you need more information about setting up your development environment [Substrate's Installation
page](https://docs.substrate.io/main-docs/install/) is a good resource.

## High-Level Architecture

This repo has support for bridging foreign chains together using a combination of Substrate pallets and external
processes called relayers. A bridge chain is one that is able to follow the consensus of a foreign chain independently.
For example, consider the case below where we want to bridge two Substrate based chains.

```
+---------------+                 +---------------+
|               |                 |               |
|     Rococo    |                 |    Westend    |
|               |                 |               |
+-------+-------+                 +-------+-------+
        ^                                 ^
        |       +---------------+         |
        |       |               |         |
        +-----> | Bridge Relay  | <-------+
                |               |
                +---------------+
```

The Rococo chain must be able to accept Westend headers and verify their integrity. It does this by using a runtime
module designed to track GRANDPA finality. Since two blockchains can't interact directly they need an external service,
called a relayer, to communicate. The relayer will subscribe to new Rococo headers via RPC and submit them to the Westend
chain for verification.

Take a look at [Bridge High Level Documentation](./docs/high-level-overview.md) for more in-depth description of the
bridge interaction.

## Project Layout

Here's an overview of how the project is laid out. The main bits are the `bin`, which is the actual "blockchain", the
`modules` which are used to build the blockchain's logic (a.k.a the runtime) and the `relays` which are used to pass
messages between chains.

```
├── modules                  // Substrate Runtime Modules (a.k.a Pallets)
│  ├── beefy                 // On-Chain BEEFY Light Client (in progress)
│  ├── grandpa               // On-Chain GRANDPA Light Client
│  ├── messages              // Cross Chain Message Passing
│  ├── parachains            // On-Chain Parachains Light Client
│  ├── relayers              // Relayer Rewards Registry
│  ├── xcm-bridge-hub        // Multiple Dynamic Bridges Support
│  ├── xcm-bridge-hub-router // XCM Router that may be used to Connect to XCM Bridge Hub
├── primitives               // Code shared between modules, runtimes, and relays
│  └──  ...
├── relays                   // Application for sending finality proofs and messages between chains
│  └──  ...
└── scripts                  // Useful development and maintenance scripts
```

## Running the Bridge

Apart from live Rococo <> Westend bridge, you may spin up local networks and test see how it works locally. More
details may be found in
[this document](https://github.com/paritytech/polkadot-sdk/tree/master//cumulus/parachains/runtimes/bridge-hubs/README.md).
