<div align="center">

# Polkadot SDK's Minimal Template

<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only"/>
<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only"/>

> This is a minimal template for creating a blockchain based on Polkadot SDK.
>
> This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

</div>

## Table of Contents

- [Intro](#intro)

- [Template Structure](#template-structure)

- [Getting Started](#getting-started)

- [Starting a Minimal Template Chain](#starting-a-minimal-template-chain)

  - [Minimal Template Node](#minimal-template-node)
  - [Zombienet with Minimal Template Node](#zombienet-with-minimal-template-node)
  - [Connect with the Polkadot-JS Apps Front-End](#connect-with-the-polkadot-js-apps-front-end)
  - [Takeaways](#takeaways)

- [Contributing](#contributing)

- [Getting Help](#getting-help)

## Intro

- 🤏 This template is a minimal (in terms of complexity and the number of components)
template for building a blockchain node.

- 🔧 Its runtime is configured with a single custom pallet as a starting point, and a handful of ready-made pallets
such as a [Balances pallet](https://paritytech.github.io/polkadot-sdk/master/pallet_balances/index.html).

- 👤 The template has no consensus configured - it is best for experimenting with a single node network.


## Template Structure

A Polkadot SDK based project such as this one consists of:

- 🧮 the [Runtime](./runtime/README.md) - the core logic of the blockchain.
- 🎨 the [Pallets](./pallets/README.md) - from which the runtime is constructed.
- 💿 a [Node](./node/README.md) - the binary application (which is not part of the cargo default-members list and is not
compiled unless building the entire workspace).

## Getting Started

- 🦀 The template is using the Rust language.

- 👉 Check the
[Rust installation instructions](https://www.rust-lang.org/tools/install) for your system.

- 🛠️ Depending on your operating system and Rust version, there might be additional
packages required to compile this template - please take note of the Rust compiler output.

Fetch minimal template code.

```sh
git clone https://github.com/paritytech/polkadot-sdk-minimal-template.git minimal-template

cd minimal-template
```

## Starting a Minimal Template Chain

### Minimal Template Node

#### Build both node & runtime

```sh
cargo build --workspace --release
```

🐳 Alternatively, build the docker image which builds all the workspace members,
and has as entry point the node binary:

```sh
docker build . -t polkadot-sdk-minimal-template
```

#### Start the `minimal-template-node`

The `minimal-template-node` has dependency on the `minimal-template-runtime`. It will use
the `minimal_template_runtime::WASM_BINARY` constant (which holds the WASM blob as a byte
array) for chain spec building, while starting.

```sh
<target/release/path/to/minimal-template-node> --tmp --consensus manual-seal-3000
# or via docker
docker run --rm polkadot-sdk-minimal-template
```

#### Zombienet with `minimal-template-node`

For this one we just need to have `zombienet` installed and run:

```sh
zombienet --provider native spawn zombienet-multi-node.toml
```

### Connect with the Polkadot-JS Apps Front-End

- 🌐 You can interact with your local node using the
hosted version of the [Polkadot/Substrate
Portal](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944).

- 🪐 A hosted version is also
available on [IPFS](https://dotapps.io/).

- 🧑‍🔧 You can also find the source code and instructions for hosting your own instance in the
[`polkadot-js/apps`](https://github.com/polkadot-js/apps) repository.

### Takeaways

Previously minimal template's development chains:

- ❌ Started in a multi-node setup will produce forks because minimal lacks consensus.
- 🧹 Do not persist the state.
- 💰 Are pre-configured with a genesis state that includes several pre-funded development accounts.
- 🧑‍⚖️ One development account (`ALICE`) is used as `sudo` accounts.

## Contributing

- 🔄 This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

- ➡️ Any pull requests should be directed to this [source](https://github.com/paritytech/polkadot-sdk/tree/master/templates/minimal).

- 😇 Please refer to the monorepo's
[contribution guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md) and
[Code of Conduct](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CODE_OF_CONDUCT.md).

## Getting Help

- 🧑‍🏫 To learn about Polkadot in general, [docs.Polkadot.com](https://docs.polkadot.com/) website is a good starting point.

- 🧑‍🔧 For technical introduction, [here](https://github.com/paritytech/polkadot-sdk#-documentation) are
the Polkadot SDK documentation resources.

- 👥 Additionally, there are [GitHub issues](https://github.com/paritytech/polkadot-sdk/issues) and
[Substrate StackExchange](https://substrate.stackexchange.com/).
- 👥You can also reach out on the [Official Polkdot discord server](https://polkadot-discord.w3f.tools/)
- 🧑Reach out on [Telegram](https://t.me/substratedevs) for more questions and discussions
