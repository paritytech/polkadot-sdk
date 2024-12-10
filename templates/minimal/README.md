<div align="center">

# Polkadot SDK's Minimal Template

<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only"/>
<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only"/>

> This is a minimal template for creating a blockchain based on Polkadot SDK.
>
> This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

</div>

* 🤏 This template is a minimal (in terms of complexity and the number of components)
template for building a blockchain node.

* 🔧 Its runtime is configured with a single custom pallet as a starting point, and a handful of ready-made pallets
such as a [Balances pallet](https://paritytech.github.io/polkadot-sdk/master/pallet_balances/index.html).

* 👤 The template has no consensus configured - it is best for experimenting with a single node network.

## Template Structure

A Polkadot SDK based project such as this one consists of:

* 💿 a [Node](./node/README.md) - the binary application.
* 🧮 the [Runtime](./runtime/README.md) - the core logic of the blockchain.
* 🎨 the [Pallets](./pallets/README.md) - from which the runtime is constructed.

## Getting Started

* 🦀 The template is using the Rust language.

* 👉 Check the
[Rust installation instructions](https://www.rust-lang.org/tools/install) for your system.

* 🛠️ Depending on your operating system and Rust version, there might be additional
packages required to compile this template - please take note of the Rust compiler output.

### Build

🔨 Use the following command to build the node without launching it:

```sh
cargo build --package minimal-template-node --release
```

<<<<<<< HEAD
🐳 Alternatively, build the docker image:
=======
## Starting a Minimal Template Chain

### Omni Node

[Omni Node](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html) can
be used to run the minimal template's runtime. `polkadot-omni-node` binary crate usage is described at a high-level
[on crates.io](https://crates.io/crates/polkadot-omni-node).

#### Install `polkadot-omni-node`

Please see installation section on [crates.io/omni-node](https://crates.io/crates/polkadot-omni-node).

#### Build `minimal-template-runtime`

```sh
cargo build -p minimal-template-runtime --release
```

#### Install `staging-chain-spec-builder`

Please see the installation section at [`crates.io/staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder).

#### Use chain-spec-builder to generate the chain_spec.json file

```sh
chain-spec-builder create --relay-chain "dev" --para-id 1000 --runtime \
    target/release/wbuild/minimal-template-runtime/minimal_template_runtime.wasm named-preset development
```

**Note**: the `relay-chain` and `para-id` flags are extra bits of information required to
configure the node for the case of representing a parachain that is connected to a relay chain.
They are not relevant to minimal template business logic, but they are mandatory information for
Omni Node, nonetheless.

#### Run Omni Node

Start Omni Node in development mode (sets up block production and finalization based on manual seal,
sealing a new block every 3 seconds), with a minimal template runtime chain spec.

```sh
polkadot-omni-node --chain <path/to/chain_spec.json> --dev
```

### Minimal Template Node

#### Build both node & runtime

```sh
cargo build --workspace --release
```

🐳 Alternatively, build the docker image which builds all the workspace members,
and has as entry point the node binary:
>>>>>>> 48c28d4c (omni-node: --dev sets manual seal and allows --chain to be set (#6646))

```sh
docker build . -t polkadot-sdk-minimal-template
```

### Single-Node Development Chain

👤 The following command starts a single-node development chain:

```sh
./target/release/minimal-template-node --dev

# docker version:
docker run --rm polkadot-sdk-minimal-template --dev
```

Development chains:

<<<<<<< HEAD
* 🧹 Do not persist the state.
* 💰 Are pre-configured with a genesis state that includes several pre-funded development accounts.
* 🧑‍⚖️ One development account (`ALICE`) is used as `sudo` accounts.
=======
#### Install `zombienet`

We can install `zombienet` as described [here](https://paritytech.github.io/zombienet/install.html#installation),
and `zombienet-omni-node.toml` contains the network specification we want to start.

#### Update `zombienet-omni-node.toml` with a valid chain spec path

Before starting the network with zombienet we must update the network specification
with a valid chain spec path. If we need to generate one, we can look up at the previous
section for chain spec creation [here](#use-chain-spec-builder-to-generate-the-chain_specjson-file).

Then make the changes in the network specification like so:

```toml
# ...
chain = "dev"
chain_spec_path = "<TO BE UPDATED WITH A VALID PATH>"
default_args = ["--dev"]
# ..
```

#### Start the network

```sh
zombienet --provider native spawn zombienet-omni-node.toml
```

### Zombienet with `minimal-template-node`

For this one we just need to have `zombienet` installed and run:

```sh
zombienet --provider native spawn zombienet-multi-node.toml
```
>>>>>>> 48c28d4c (omni-node: --dev sets manual seal and allows --chain to be set (#6646))

### Connect with the Polkadot-JS Apps Front-End

* 🌐 You can interact with your local node using the
hosted version of the [Polkadot/Substrate
Portal](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944).

* 🪐 A hosted version is also
available on [IPFS](https://dotapps.io/).

* 🧑‍🔧 You can also find the source code and instructions for hosting your own instance in the
[`polkadot-js/apps`](https://github.com/polkadot-js/apps) repository.

## Contributing

* 🔄 This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

* ➡️ Any pull requests should be directed to this [source](https://github.com/paritytech/polkadot-sdk/tree/master/templates/minimal).

* 😇 Please refer to the monorepo's
[contribution guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md) and
[Code of Conduct](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CODE_OF_CONDUCT.md).

## Getting Help

* 🧑‍🏫 To learn about Polkadot in general, [Polkadot.network](https://polkadot.network/) website is a good starting point.

* 🧑‍🔧 For technical introduction, [here](https://github.com/paritytech/polkadot-sdk#-documentation) are
the Polkadot SDK documentation resources.

* 👥 Additionally, there are [GitHub issues](https://github.com/paritytech/polkadot-sdk/issues) and
[Substrate StackExchange](https://substrate.stackexchange.com/).


## Release

polkadot v1.15.0
