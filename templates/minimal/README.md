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

Fetch minimal template code:

```sh
git clone https://github.com/paritytech/polkadot-sdk-minimal-template.git minimal-template

cd minimal-template
```

### Build

🔨 Use the following command to build the template, which by default
compiles just the runtime. There is also a `parachain-template-node`
crate that is able to run and load the runtime but the recommended
way of starting the template is based on `Omni Node`.

```sh
cargo build --release
```

### Single-node Development Chain with Omni Node

⬇️  Omni Node can be run by using the `polkadot-omni-node` binary, which can be
downloaded from [Polkadot SDK releases](https://github.com/paritytech/polkadot-sdk/releases/latest).

* 🔗 Once downloaded, it must be added to the `PATH` environment variable like so:

```sh
export PATH="<path-to-binary>:$PATH"
```

↩️  The Omni Node needs a runtime chainspec which can be generated based on
the `minimal-runtime`.

```sh
# Build the minimal runtime.
cargo build -p minimal-template-runtime --release
# Install chain-spec-builder if not installed already.
cargo install staging-chain-spec-builder
# Use chain-spec-builder to generate the chain_spec.json file based on the development preset.
chain-spec-builder create --relay-chain "dev" --para-id 1000 --runtime \
    <target/release/wbuild/path/to/minimal-template-runtime.wasm> named-preset development
```

⚙️  The `relay-chain` and `para-id` flags are extra bits of information to configure the node
for the case of representing a parachain that is connected to a relay chain. They are not relevant
to minimal template business logic, but they are mandatory information for Omni Node, nonetheless.

🚀 Start Omni Node with manual seal (3 seconds block times) and minimal template runtime based
chain spec.

```sh
polkadot-omni-node --chain <path/to/chain_spec.json> --dev-block-time 3000 --tmp
```

### Single-Node Development Chain with Minimal Template Node

⚙️  Use the following command to build the node as well:

```sh
cargo build --workspace --release
```

🐳 Alternatively, build the docker image which builds all the workspace members,
and has as entry point the node binary:

```sh
docker build . -t polkadot-sdk-minimal-template
```

👤 The following command starts a single-node development chain:

```sh
./target/release/minimal-template-node --dev

# docker version:
docker run --rm polkadot-sdk-minimal-template --dev
```

Development chains:

* 🧹 Do not persist the state.
* 💰 Are pre-configured with a genesis state that includes several pre-funded development accounts.
* 🧑‍⚖️ One development account (`ALICE`) is used as `sudo` accounts.

**Note**: running multiple nodes with the same command used for the single node setup is also possible and
it can work up to a certain moment. The nodes will be peers, taking their turn in block production if manual
seal is configured to allow nodes to produce blocks at certain intervals and in the meantime to
import the blocks produced by peers. However, there is a big chance that at some point in time at least two nodes
will overlap with the block production at a certain height, at which point they will fork and will not consider
each others blocks anymore (stopping from being peers). They will continue to participate in blocks production
of their own fork and possibly of other nodes too.

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
