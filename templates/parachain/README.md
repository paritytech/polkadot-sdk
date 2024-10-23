<div align="center">

# Polkadot SDK's Parachain Template

<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only"/>
<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only"/>

> This is a template for creating a [parachain](https://wiki.polkadot.network/docs/learn-parachains) based on Polkadot SDK.
>
> This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

</div>

* ⏫ This template provides a starting point to build a [parachain](https://wiki.polkadot.network/docs/learn-parachains).

* ☁️ It is based on the
[Cumulus](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/cumulus/index.html) framework.

* 🔧 Its runtime is configured with a single custom pallet as a starting point, and a handful of ready-made pallets
such as a [Balances pallet](https://paritytech.github.io/polkadot-sdk/master/pallet_balances/index.html).

* 👉 Learn more about parachains [here](https://wiki.polkadot.network/docs/learn-parachains)

## Template Structure

A Polkadot SDK based project such as this one consists of:

* 💿 a [Node](./node/README.md) - the binary application.
* 🧮 the [Runtime](./runtime/README.md) - the core logic of the parachain.
* 🎨 the [Pallets](./pallets/README.md) - from which the runtime is constructed.

## Getting Started

* 🦀 The template is using the Rust language.

* 👉 Check the
[Rust installation instructions](https://www.rust-lang.org/tools/install) for your system.

* 🛠️ Depending on your operating system and Rust version, there might be additional
packages required to compile this template - please take note of the Rust compiler output.

Fetch parachain template code:

```sh
git clone https://github.com/paritytech/polkadot-sdk-parachain-template.git parachain-template

cd parachain-template
```

### Build

🔨 Use the following command to build just the `runtime`. There is also
a `node` crate that when started can load the runtime accordingly, but the
recommended way of running the template is with `Omni Node` (TODO: add link to the docs).


```sh
cargo build --release
```

### Local Development Chain with Omni Node

⬇️  Omni Node can run by using the `polkadot-omni-node` binary, which can be downloaded
from [Polkadot SDK releases](https://github.com/paritytech/polkadot-sdk/releases/latest).

🔗 Once downloaded, add it to the `PATH` environment variable like so:

```sh
export PATH="<path-to-binary>:$PATH"
```

↩️  If not already built, we should build the `runtime` and generate a development chain spec.
The chain spec will be passed to the Omni Node binary when starting it.

```sh
# Build the parachain runtime.
cargo build --release
# Install chain-spec-builder if not installed already.
cargo install staging-chain-spec-builder
# Use chain-spec-builder to generate the chain_spec.json file based on the development preset.
chain-spec-builder create --relay-chain "rococo-local" --para-id 1000 --runtime \
    <target/release/wbuild/path/to/parachain-template-runtime.wasm> named-preset development
```

⚙️  The `relay-chain` and `para-id` flags in the chain spec generation above are extra bits of
information required to configure the node in relation to its parachain id (which must be set
to `1000` for the parachain template, to be the same as the `ParachainInfo` pallet [genesis config](https://github.com/paritytech/polkadot-sdk/blob/master/templates/parachain/runtime/src/genesis_config_presets.rs)).
The `relay-chain` must correspond to the relay chain id where the parachain connects to.

We'll start Omni Node with zombienet, but before doing that we must update the path to the
`chain_spec.json` file in the `parachains` section of the `zombienet-omni-node.toml` file,
which holds the zombienet network specification:

```toml
# ...
[[parachains]]
id = 1000
# insert the correct path on your file system
chain_spec_path = "<path/to/chain_spec.json>"
# ...
```

🚀 Start the parachain runtime with Omni Node like below. This will
start two relay chain nodes and one collator node:

```sh
zombienet --provider native spawn ./zombienet-omni-node.toml
```

### Local Development Chain

🧟 This project uses [Zombienet](https://github.com/paritytech/zombienet) to orchestrate the relaychain and parachain nodes.
You can grab a [released binary](https://github.com/paritytech/zombienet/releases/latest) or use an [npm version](https://www.npmjs.com/package/@zombienet/cli).

This template produces a parachain node. You can install it in your environment by running:

```sh
cargo install --path node
```

You still need a relaychain node - you can download the `polkadot`
(and the accompanying `polkadot-prepare-worker` and `polkadot-execute-worker`)
binaries from [Polkadot SDK releases](https://github.com/paritytech/polkadot-sdk/releases/latest).

In addition to the installed parachain node, make sure to bring
`zombienet`, `polkadot`, `polkadot-prepare-worker`, and `polkadot-execute-worker`
into `PATH`.

This way, we can conveniently use them in the following steps.

👥 The following command starts a local development chain, with a single relay chain node and a single parachain collator:

```sh
zombienet --provider native spawn ./zombienet.toml

# Alternatively, the npm version:
npx --yes @zombienet/cli --provider native spawn ./zombienet.toml
```

Development chains:

* 🧹 Do not persist the state.
* 💰 Are preconfigured with a genesis state that includes several prefunded development accounts.
* 🧑‍⚖️ Development accounts are used as validators, collators, and `sudo` accounts.

### Connect with the Polkadot-JS Apps Front-End

* 🌐 You can interact with your local node using the
hosted version of the Polkadot/Substrate Portal:
[relay chain](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944)
and [parachain](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9988).

* 🪐 A hosted version is also
available on [IPFS](https://dotapps.io/).

* 🧑‍🔧 You can also find the source code and instructions for hosting your own instance in the
[`polkadot-js/apps`](https://github.com/polkadot-js/apps) repository.

## Contributing

* 🔄 This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

* ➡️ Any pull requests should be directed to this [source](https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain).

* 😇 Please refer to the monorepo's
[contribution guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md) and
[Code of Conduct](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CODE_OF_CONDUCT.md).

## Getting Help

* 🧑‍🏫 To learn about Polkadot in general, [Polkadot.network](https://polkadot.network/) website is a good starting point.

* 🧑‍🔧 For technical introduction, [here](https://github.com/paritytech/polkadot-sdk#-documentation) are
the Polkadot SDK documentation resources.

* 👥 Additionally, there are [GitHub issues](https://github.com/paritytech/polkadot-sdk/issues) and
[Substrate StackExchange](https://substrate.stackexchange.com/).
