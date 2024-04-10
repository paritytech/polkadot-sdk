# Parity Bridges Common

This is a collection of components for building bridges.

These components include runtime modules to help you construct your bridge's runtime, as well as
bridge relays for cross-chain communication.

A bridge node is also available. The node can be used to run a test network which has support for bridging Ethereum
PoA chains to Substrate. We're working on expanding this functionality in the future.

🚧 The bridges are currently under construction - a hardhat is recommended beyond this point 🚧

## Contents
- [Installation](#installation)
- [Project Layout](#project-layout)
- [Rialto Runtime](#rialto-runtime)
- [Ethereum Node](#ethereum-node)
- [Bridge Relay](#bridge-relay)
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

## Project Layout
Here's an overview of how the project is laid out. The main bits are the `node`, which is the actual
"blockchain", the `modules` which are used to build the blockchain's logic (a.k.a the runtime) and
the `relays` which are used to pass messages between chains.

```
├── bin
│  └── node              // Bridge ready chain implementation
├── modules              // Runtime Modules
│  ├── ethereum          // Manage Ethereum PoA chain info
│  ├── ethereum-contract // Ethereum built-in for validating Substrate block info
│  ├── currency-exchange // Cross-chain fund transfers
│  └── substrate         // Manage Substrate chain info
├── primitives           // Shared runtime and node code
│  └── ethereum-poa      // Helpers for Ethereum PoA
├── relays               // Cross-chain communication
│  ├── ethereum          // Sync and communicate between Ethereum PoA + Substrate chains
│  └── substrate         // 🚧 WIP 🚧
```

## Rialto Runtime
The node runtime consists of several runtime modules, however not all of them are used at the same
time. When running an Ethereum PoA to Substrate bridge the modules required are the Ethereum module
and the currency exchange module. When running a Substrate to Substrate bridge the Substrate and
currency exchange modules are required.

Below is a brief description of each of the runtime modules.

### Ethereum Bridge Runtime Module
The main job of this runtime module is to keep track of useful information an Ethereum PoA chain
which has been submitted by a bridge relayer. This includes:

  - Ethereum headers and their status (e.g are they the best header, are they finalized, etc.)
  - Current validator set, and upcoming validator sets

This runtime module has more responsibilties than simply storing headers and validator sets. It is
able to perform checks on the incoming headers to verify their general integrity, as well as whether
or not they've been finalized by the authorities on the PoA chain.

This module is laid out as so:

```
├── ethereum
│  └── src
│     ├── error.rs        // Runtime error handling
│     ├── finality.rs     // Manage finality operations
│     ├── import.rs       // Import new Ethereum headers
│     ├── lib.rs          // Store headers and validator set info
│     ├── validators.rs   // Track current and future PoA validator sets
│     └── verification.rs // Verify validity of incoming Ethereum headers
```

### Currency Exchange Runtime Module
The currency exchange module is used to faciliate cross-chain funds transfers. It works by accepting
a transaction which proves that funds were locked on one chain, and releases a corresponding amount
of funds on the recieving chain.

For example: Alice would like to send funds from chain A to chain B. What she would do is send a
transaction to chain A indicating that she would like to send funds to an address on chain B. This
transaction would contain the amount of funds she would like to send, as well as the address of the
recipient on chain B. These funds would now be locked on chain A. Once the block containing this
"locked-funds" transaction is finalized it can be relayed to chain B. Chain B will verify that this
transaction was included in a finalized block on chain A, and if successful deposit funds into the
recipient account on chain B.

Chain B would need a way to convert from a foreign currency to its local currency. How this is done
is left to the runtime developer for chain B.

This module is one example of how an on-chain light client can be used to prove a particular action
was taken on a foreign chain. In particular it enables transfers of the foreign chain's native
currency, but more sophisticated modules such as ERC20 token transfers or arbitrary message transfers
are being worked on as well.

### Substrate Bridge Runtime Module
👷 Under Construction 👷‍♀️

## Ethereum Node
On the Ethereum side of things, we require two things. First, a Solidity smart contract to track the
Substrate headers which have been submitted to the bridge (by the relay), and a built-in contract to
be able to verify that headers have been finalized by the Grandpa finality gadget. Together this
allows the Ethereum PoA chain to verify the integrity and finality of incoming Substrate headers.

The Solidity smart contract is not part of this repo, but can be found
[here](https://github.com/svyatonik/substrate-bridge-sol/blob/master/substrate-bridge.sol) if you're
curious. We have the contract ABI in the `ethereum/relays/res` directory.

## Bridge Relay
The bridge relay is responsible for syncing the chains which are being bridged, and passing messages
between them. The current implementation of the relay supportings syncing and interacting with
Ethereum PoA and Substrate chains.

The folder structure of the bridge relay is as follows:

```
├── relays
│  ├── ethereum
│  │  ├── res
│  │  │  └── ...
│  │  └── src
│  │     ├── ethereum_client.rs          // Interface for Ethereum RPC
│  │     ├── ethereum_deploy_contract.rs // Utility for deploying bridge contract to Ethereum
│  │     ├── ethereum_exchange.rs        // Relay proof of PoA -> Substrate exchange transactions
│  │     ├── ethereum_sync_loop.rs       // Sync headers from Ethereum, submit to Substrate
│  │     ├── ethereum_types.rs           // Useful Ethereum types
│  │     ├── exchange.rs                 // Relay proof of exchange transactions
│  │     ├── headers.rs                  // Track synced and incoming block headers
│  │     ├── main.rs                     // Entry point to binary
│  │     ├── substrate_client.rs         // Interface for Substrate RPC
│  │     ├── substrate_sync_loop.rs      // Sync headers from Substrate, submit to Ethereum
│  │     ├── substrate_types.rs          // Useful Ethereum types
│  │     ├── sync.rs                     // Sync configuration and helpers
│  │     ├── sync_loop.rs                // Header synchronization between source and target chains
│  │     ├── sync_types.rs               // Useful sync types
│  │     └── utils.rs                    // General utilities
```

## Running the Bridge

To run the Bridge you need to be able to connect the bridge relay node to the RPC interface of nodes
on each side of the bridge (home & foreign chain). An easy way to build all the required nodes is
through Docker.

### Local Development Build

#### Building

First you'll need to build the bridge node and relay. This can be done as follows:

```bash
# In `parity-bridges-common` folder
cargo build -p rialto-bridge-node
cargo build -p ethereum-poa-relay
```

Next you'll need to clone the following [fork of OpenEthereum](https://github.com/paritytech/openethereum).
If you're doing development which only involves the Ethereum to Substrate side of the bridge you may
use the `master` branch. Otherwise you'll need to checkout the `substrate-builtins-stubs` branch.

```bash
# Should be at the same level as `parity-bridges-common` folder
git clone https://github.com/paritytech/openethereum.git openethereum
git fetch
git checkout substrate-builtins-stubs
```

If you've checked out the `substrate-builtins-stubs` branch make sure you've cloned the OpenEthereum
repo at the same level as `parity-bridges-common` since it references the repo.

Next you'll need to build the Ethereum node:

```bash
# In `openethereum` folder
cargo build
```

#### Running

To run a simple dev network you'll can use the scripts located in
[the `scripts` folder](./scripts). Since the relay connects to both the Substrate and Ethereum
chains it must be run last.

```bash
# In `parity-bridges-common` folder
./scripts/run-openethereum-node.sh
./scripts/run-rialto-bridge-node.sh
./scripts/run-eth2sub-relay.sh
```
At this point you should see the relayer submitting blocks from the Ethereum chain
to the Substrate chain.

### Local Docker Build
If you want to make a Docker container using your local source files you can run the following
command at the top level of the repository:

```bash
docker build . -t bridge-relay-dev
```

You can also build and run the Substrate based node as follows:

```bash
docker build . -t bridge-node-dev --build-arg PROJECT=rialto-bridge-node
```

To run the Substrate node you can do the following:

```bash
docker run -it bridge-node-dev --dev --tmp
```

Notice that the `docker run` command will accept all the normal Substrate flags. For local
development you should at minimum run with the `--dev` flag or else no blocks will be produced.

### Full Network Docker Setup
See [Deployments README](./deployments/README.md) to learn more about how to run
a more sophisticated test network using `docker-compose` setup.
