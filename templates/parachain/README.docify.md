---
title: Parachain Template
---

# Parachain Template

The Parachain Template from the Polkadot SDK is a foundational starting point for developers aiming to build parachains compatible with the Polkadot network. This template is regularly updated following releases in the main Polkadot SDK monorepo.

## Key Features

- **Parachain Development**: The template is designed to facilitate the creation of parachains, leveraging the Cumulus framework to ensure seamless integration with relay chains like Polkadot or Kusama.
- **Runtime Configuration**: It comes pre-configured with a basic runtime that includes a custom pallet as a starting point, along with essential pallets such as the Balances pallet.

## Template Structure

A Polkadot SDK-based project, such as this template, comprises:

- **Node**: The executable binary application that interacts with other nodes in the network, aiming for consensus and acting as an RPC server for blockchain interactions. [Learn more](./node/README.md).
- **Runtime**: The core logic of the parachain, dictating state transitions and business logic.
- **Pallets**: Modular components that make up the runtime, each encapsulating specific functionality. [Learn more](./pallets/README.md).

## Getting Started

### Prerequisites

- **Rust Language**: The template is built using Rust. Ensure that Rust is installed on your system by following the [official installation instructions](https://www.rust-lang.org/tools/install).

### Building the Node

To compile the node without launching it, execute:

```bash
cargo build --release
```

Alternatively, to build the Docker image:

```bash
docker build . -t polkadot-sdk-parachain-template
```

### Local Development Chain

This project utilizes Zombienet to orchestrate relay chain and parachain nodes. After installing the necessary binaries and ensuring they are in your system's `PATH`, you can start a local development chain with:

```bash
zombienet --provider native spawn ./zombienet.toml

# Alternatively, using npm:
npx --yes @zombienet/cli --provider native spawn ./zombienet.toml
```

Development chains are ephemeral, preconfigured with a genesis state that includes several prefunded development accounts, and utilize development accounts as validators, collators, and `sudo` accounts.

### Connecting with Polkadot-JS Apps Front-End

Interact with your local node using the hosted version of the [Polkadot/Substrate Portal](https://polkadot.js.org/apps) for both the relay chain and parachain. A hosted version is also available on IPFS. For hosting your own instance, refer to the `polkadot-js/apps` repository.

## Contributing

This template is automatically updated after releases in the main Polkadot SDK monorepo. For contributions, direct pull requests to the source. Please adhere to the monorepo's [contribution guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/CONTRIBUTING.md) and [Code of Conduct](https://github.com/paritytech/polkadot-sdk/blob/master/CODE_OF_CONDUCT.md).

## Getting Help

- For general information about Polkadot, the [Polkadot.network](https://polkadot.network/) website is a good starting point.
- For technical documentation, refer to the [Polkadot SDK documentation resources](https://docs.polkadot.com/).
- Additionally, you can seek assistance through GitHub issues and the [Substrate StackExchange](https://substrate.stackexchange.com/).

This template serves as a robust foundation for developing parachains, providing the necessary tools and configurations to streamline the development process.
