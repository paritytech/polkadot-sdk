# Parity Bridges Common

This is a collection of components for building bridges.

These components include Substrate pallets for syncing headers, passing arbitrary messages, as well
as libraries for building relayers to provide cross-chain communication capabilities.

Three bridge nodes are also available. The nodes can be used to run test networks which bridge other
Substrate chains or Ethereum Proof-of-Authority chains.

🚧 The bridges are currently under construction - a hardhat is recommended beyond this point 🚧

## Contents
- [Installation](#installation)
- [High-Level Architecture](#high-level-architecture)
- [Project Layout](#project-layout)
- [Running the Bridge](#running-the-bridge)

## Installation
To get up and running you need both stable and nightly Rust. Rust nightly is used to build the Web
Assembly (WASM) runtime for the node. You can configure the WASM support as so:

```
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

If you need more information about setting up your development environment Substrate's
[Getting Started](https://substrate.dev/docs/en/knowledgebase/getting-started/) page is a good
resource.

## High-Level Architecture

This repo has support for bridging foreign chains together using a combination of Substrate pallets
and external processes called relayers. A bridge chain is one that is able to follow the consensus
of a foreign chain independently. For example, consider the case below where we want to bridge two
Substrate based chains.

```
+---------------+                 +---------------+
|               |                 |               |
|     Rialto    |                 |    Millau     |
|               |                 |               |
+-------+-------+                 +-------+-------+
        ^                                 ^
        |       +---------------+         |
        |       |               |         |
        +-----> | Bridge Relay  | <-------+
                |               |
                +---------------+
```

The Millau chain must be able to accept Rialto headers and verify their integrity. It does this by
using a runtime module designed to track GRANDPA finality. Since two blockchains can't interact
directly they need an external service, called a relayer, to communicate. The relayer will subscribe
to new Rialto headers via RPC and submit them to the Millau chain for verification.

Take a look at [Bridge High Level Documentation](./docs/high-level-overview.md) for more in-depth
description of the bridge interaction.

## Project Layout
Here's an overview of how the project is laid out. The main bits are the `node`, which is the actual
"blockchain", the `modules` which are used to build the blockchain's logic (a.k.a the runtime) and
the `relays` which are used to pass messages between chains.

```
├── bin             // Node and Runtime for the various Substrate chains
│  └── ...
├── deployments     // Useful tools for deploying test networks
│  └──  ...
├── diagrams        // Pretty pictures of the project architecture
│  └──  ...
├── modules         // Substrate Runtime Modules (a.k.a Pallets)
│  ├── ethereum     // Ethereum PoA Header Sync Module
│  ├── substrate    // Substrate Based Chain Header Sync Module
│  ├── message-lane // Cross Chain Message Passing
│  └──  ...
├── primitives      // Code shared between modules, runtimes, and relays
│  └──  ...
├── relays          // Application for sending headers and messages between chains
│  └──  ...
└── scripts         // Useful development and maintenence scripts
 ```

## Running the Bridge

To run the Bridge you need to be able to connect the bridge relay node to the RPC interface of nodes
on each side of the bridge (source and target chain).

There are 3 ways to run the bridge, described below:
 - building & running from source,
 - building or using Docker images for each individual component,
 - running a Docker Compose setup (recommended).

### Building

First you'll need to build the bridge nodes and relay. This can be done as follows:

```bash
# In `parity-bridges-common` folder
cargo build -p rialto-bridge-node
cargo build -p millau-bridge-node
cargo build -p substrate-relay
```

### Running

To run a simple dev network you'll can use the scripts located in
[the `scripts` folder](./scripts). Since the relay connects to both Substrate chains it must be run
last.

```bash
# In `parity-bridges-common` folder
./deployments/local-scripts/run-rialto-bridge-node.sh
./deployments/local-scripts/run-millau-bridge-node.sh
./deployments/local-scripts/run-millau-to-rialto-relay.sh
./deployments/local-scripts/run-rialto-to-millau-relay.sh
```

At this point you should see the relayer submitting blocks from the Millau Substrate chain to the
Rialto Substrate chain and vice-versa.

### Local Docker Build
If you want to make a Docker container using your local source files you can run the following
command at the top level of the repository:

```bash
docker build . -t local/rialto-bridge-node --build-arg PROJECT=rialto-bridge-node
docker build . -t local/millau-bridge-node --build-arg PROJECT=millau-bridge-node
docker build . -t local/substrate-relay --build-arg PROJECT=substrate-relay
```

You can then run the network as follows:

```bash
docker run -it local/rialto-bridge-node --dev --tmp
docker run -it local/millau-bridge-node --dev --tmp
docker run -it local/substrate-relay
```

Notice that the `docker run` command will accept all the normal Substrate flags. For local
development you should at minimum run with the `--dev` flag or else no blocks will be produced.

### Full Network Docker Compose Setup

For a more sophisticated deployment which includes bidirectional header sync, message passing,
monitoring dashboards, etc. see the [Deployments README](./deployments/README.md).
