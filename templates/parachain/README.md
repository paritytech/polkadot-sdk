<div align="center">

# Polkadot SDK's Parachain Template

<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only"/>
<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only"/>

> This is a template for creating a [parachain](https://wiki.polkadot.network/docs/learn-parachains) based on Polkadot SDK.
>
> This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

</div>

## Table of Contents

- [Intro](#intro)

- [Template Structure](#template-structure)

- [Getting Started](#getting-started)

- [Starting a Development Chain](#starting-a-development-chain)

  - [Omni Node](#omni-node-prerequisites)
  - [Zombienet setup with Omni Node](#zombienet-setup-with-omni-node)
  - [Parachain Template Node](#parachain-template-node)
  - [Connect with the Polkadot-JS Apps Front-End](#connect-with-the-polkadot-js-apps-front-end)
  - [Takeaways](#takeaways)

- [Runtime development](#runtime-development)
- [Deploy to Paseo TestNet And See Your First Block](#deploy-to-paseo-testnet-and-see-your-first-block)
- [Contributing](#contributing)
- [Getting Help](#getting-help)

## Intro

- ⏫ This template provides a starting point to build a [parachain](https://wiki.polkadot.network/docs/learn-parachains).

- ☁️ It is based on the
  [Cumulus](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/cumulus/index.html) framework.

- 🔧 Its runtime is configured with a single custom pallet as a starting point, and a handful of ready-made pallets
  such as a [Balances pallet](https://paritytech.github.io/polkadot-sdk/master/pallet_balances/index.html).

- 👉 Learn more about parachains [here](https://wiki.polkadot.network/docs/learn-parachains)

## Template Structure

A Polkadot SDK based project such as this one consists of:

- 🧮 the [Runtime](./runtime/README.md) - the core logic of the parachain.
- 🎨 the [Pallets](./pallets/README.md) - from which the runtime is constructed.
- 💿 a [Node](./node/README.md) - the binary application, not part of the project default-members list and not compiled unless
  building the project with `--workspace` flag, which builds all workspace members, and is an alternative to
  [Omni Node](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html).

## Getting Started

- 🦀 The template is using the Rust language.

- 👉 Check the
  [Rust installation instructions](https://www.rust-lang.org/tools/install) for your system.

- 🛠️ Depending on your operating system and Rust version, there might be additional
  packages required to compile this template - please take note of the Rust compiler output.

Fetch parachain template code:

```sh
git clone https://github.com/paritytech/polkadot-sdk-parachain-template.git parachain-template

cd parachain-template
```

## Starting a Development Chain

The parachain template relies on a hardcoded parachain id which is defined in the runtime code
and referenced throughout the contents of this file as `{{PARACHAIN_ID}}`. Please replace
any command or file referencing this placeholder with the value of the `PARACHAIN_ID` constant:

```rust,ignore
pub const PARACHAIN_ID: u32 = 1000;
```

### Omni Node Prerequisites

[Omni Node](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html) can
be used to run the parachain template's runtime. `polkadot-omni-node` binary crate usage is described at a high-level
[on crates.io](https://crates.io/crates/polkadot-omni-node).

#### Install `polkadot-omni-node`

```sh
cargo install polkadot-omni-node
```

> For more advanced options, please see the installation section at [`crates.io/omni-node`](https://crates.io/crates/polkadot-omni-node).

#### Build `parachain-template-runtime`

```sh
cargo build --profile production
```

#### Install `staging-chain-spec-builder`

```sh
cargo install staging-chain-spec-builder
```

> For more advanced options, please see the installation section at [`crates.io/staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder).

#### Use `chain-spec-builder` to generate the `chain_spec.json` file

```sh
chain-spec-builder create --relay-chain "rococo-local" --runtime \
    target/release/wbuild/parachain-template-runtime/parachain_template_runtime.wasm named-preset development
```

**Note**: the `relay-chain` flag is required by Omni Node. The `relay-chain` value is set in accordance
with the relay chain ID where this instantiation of parachain-template will connect to.

#### Run Omni Node

Start Omni Node with the generated chain spec. We'll start it in development mode (without a relay chain config), producing
and finalizing blocks based on manual seal, configured below to seal a block with each second.

```bash
polkadot-omni-node --chain <path/to/chain_spec.json> --dev --dev-block-time 1000
```

However, such a setup is not close to what would run in production, and for that we need to setup a local
relay chain network that will help with the block finalization. In this guide we'll setup a local relay chain
as well. We'll not do it manually, by starting one node at a time, but we'll use [zombienet](https://paritytech.github.io/zombienet/intro.html).

Follow through the next section for more details on how to do it.

### Zombienet setup with Omni Node

Assuming we continue from the last step of the previous section, we have a chain spec and we need to setup a relay chain.
We can install `zombienet` as described [here](https://paritytech.github.io/zombienet/install.html#installation), and
`zombienet-omni-node.toml` contains the network specification we want to start.

#### Relay chain prerequisites

Download the `polkadot` (and the accompanying `polkadot-prepare-worker` and `polkadot-execute-worker`) binaries from
[Polkadot SDK releases](https://github.com/paritytech/polkadot-sdk/releases). Then expose them on `PATH` like so:

```sh
export PATH="$PATH:<path/to/binaries>"
```

#### Update `zombienet-omni-node.toml` with a valid chain spec path

To simplify the process of using the parachain-template with zombienet and Omni Node, we've added a pre-configured
development chain spec (dev_chain_spec.json) to the parachain template. The zombienet-omni-node.toml file of this
template points to it, but you can update it to an updated chain spec generated on your machine. To generate a
chain spec refer to [staging-chain-spec-builder](https://crates.io/crates/staging-chain-spec-builder)

Then make the changes in the network specification like so:

```toml
# ...
[[parachains]]
id = "<PARACHAIN_ID>"
chain_spec_path = "<TO BE UPDATED WITH A VALID PATH>"
# ...
```

#### Start the network

```sh
zombienet --provider native spawn zombienet-omni-node.toml
```

### Parachain Template Node

As mentioned in the `Template Structure` section, the `node` crate is optionally compiled and it is an alternative
to `Omni Node`. Similarly, it requires setting up a relay chain, and we'll use `zombienet` once more.

#### Install the `parachain-template-node`

```sh
cargo install --path node --locked
```

#### Setup and start the network

For setup, please consider the instructions for `zombienet` installation [here](https://paritytech.github.io/zombienet/install.html#installation)
and [relay chain prerequisites](#relay-chain-prerequisites).

We're left just with starting the network:

```sh
zombienet --provider native spawn zombienet.toml
```

### Connect with the Polkadot-JS Apps Front-End

- 🌐 You can interact with your local node using the
  hosted version of the Polkadot/Substrate Portal:
  [relay chain](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944)
  and [parachain](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9988).

- 🪐 A hosted version is also
  available on [IPFS](https://dotapps.io/).

- 🧑‍🔧 You can also find the source code and instructions for hosting your own instance in the
  [`polkadot-js/apps`](https://github.com/polkadot-js/apps) repository.

### Takeaways

Development parachains:

- 🔗 Connect to relay chains, and we showcased how to connect to a local one.
- 🧹 Do not persist the state.
- 💰 Are preconfigured with a genesis state that includes several prefunded development accounts.
- 🧑‍⚖️ Development accounts are used as validators, collators, and `sudo` accounts.

## Runtime development

We recommend using [`chopsticks`](https://github.com/AcalaNetwork/chopsticks) when the focus is more on the runtime
development and `OmniNode` is enough as is.

### Install chopsticks

To use `chopsticks`, please install the latest version according to the installation [guide](https://github.com/AcalaNetwork/chopsticks?tab=readme-ov-file#install).

### Build a raw chain spec

Build the `parachain-template-runtime` as mentioned before in this guide and use `chain-spec-builder`
again but this time by passing `--raw-storage` flag:

```sh
chain-spec-builder create --raw-storage --relay-chain "rococo-local" --runtime \
    target/release/wbuild/parachain-template-runtime/parachain_template_runtime.wasm named-preset development
```

### Start `chopsticks` with the chain spec

```sh
npx @acala-network/chopsticks@latest --chain-spec <path/to/chain_spec.json>
```

### Alternatives

`OmniNode` can be still used for runtime development if using the `--dev` flag, while `parachain-template-node` doesn't
support it at this moment. It can still be used to test a runtime in a full setup where it is started alongside a
relay chain network (see [Parachain Template node](#parachain-template-node) setup).

## Deploy to Paseo TestNet And See Your First Block

After setting up your parachain locally, you can deploy it to the Paseo public testnet to observe it producing blocks in a realistic environment.

### Steps to Deploy

Note: The following instructions are also available at the [Zero to Hero Tutorial](https://docs.polkadot.com/tutorials/polkadot-sdk/parachains/zero-to-hero/deploy-to-testnet/) where this is available in a more detailed manner, we recommend you to try that out as well if something here is unclear.

### Obtain PAS Tokens

Visit the [Polkadot Faucet](https://faucet.polkadot.network/) and request PAS tokens for your account. Ensure you're connected to the Paseo network on [Polkadot.js Apps](https://polkadot.js.org/apps/#/explorer).

### Reserve & Register a ParaID

Note: We recommend Google Chrome for the following steps. 

1. Open [Polkadot.js Apps](https://polkadot.js.org/apps/#/explorer) and switch the network selector to Paseo.

2. Go to Network → Parachains → Parathreads, click `+ ParaId`, then Submit. You’ll be assigned the next free ID (e.g. 4508).

3. In Explorer, confirm you see a `registrar.Reserved` event.

### Register Your Parachain

1. Ensure your parachain's genesis state and runtime .wasm file are ready.

2. In Polkadot.js Apps, navigate to Network > Parachains, then to the Parathreads tab.

3. Click on `+ Register` and provide:
1. Your reserved ParaID
2. The compiled .wasm runtime file
3. The genesis state file

4. Submit the transaction to register your parachain.


### Acquire Coretime

To enable your parachain to produce and finalize blocks, you need to obtain coretime.

1. In Polkadot.js Apps, go to Developer > Extrinsics.

2. Select your account and choose the onDemand.placeOrderAllowDeath extrinsic.

3. Provide the following parameters:

1. paraId: Your reserved ParaID
2. maxAmount: An appropriate amount of PAS tokens (e.g., 1000000000000)

4. Submit the extrinsic.

Upon success, your parachain will start producing blocks.

### Generate Customs Keys for Collator Node

1. To perform this step, you can use subkey, a command-line tool for generating and managing keys:
```
docker run -it parity/subkey:latest generate --scheme sr25519
```

### Build the Chain Specification
Generate the plain chain specification:
```
polkadot-omni-node chain-spec-builder \
  --chain-spec-path ./plain_chain_spec.json \
  create \
  --relay-chain paseo \
  --para-id <YOUR_PARA_ID> \
  --runtime target/release/wbuild/parachain-template-runtime/parachain_template_runtime.compact.compressed.wasm \
  named-preset local_testnet 
```
Edit plain_chain_spec.json to update fields like name, id, protocolId, para_id, and parachainInfo.parachainId with your ParaID. Also, configure balances, collatorSelection.invulnerables, session.keys, and sudo fields accordingly.
Then, generate the raw chain specification:

``` 
polkadot-omni-node chain-spec-builder \
  --chain-spec-path ./raw_chain_spec.json \
  build \
  --chain plain_chain_spec.json
```

### Start the Collator Node

1. Before starting a collator, you need to generate a node key. This key is responsible for communicating with peers as in a p2p network:

```
polkadot-omni-node key generate-node-key \
--base-path data \
--chain raw_chain_spec.json

```
2. You can start the collator with a command similar to the following:

``` 
polkadot-omni-node --collator \
--chain raw_chain_spec.json \
--base-path data \
--port 40333 \
--rpc-port 8845 \
--force-authoring \
--node-key-file ./data/chains/custom/network/secret_ed25519 \
-- \
--sync warp \
--chain paseo \
--port 50343 \
--rpc-port 9988
```

Ensure that the paths and ports are correctly set according to your environment.


### Obtain Coretime

To produce blocks, your parachain needs coretime. You can acquire it in two ways:

1. On-Demand Coretime: Use the onDemand.placeOrderAllowDeath extrinsic on the Paseo relay chain. In Polkadot.js Apps, select the extrinsic, input your ParaID and desired amount, and submit the transaction.
2. Bulk Coretime: Purchase via the Broker pallet on the coretime system parachain. Assign the purchased core to your registered ParaID.

Once coretime is assigned, your collator should start producing blocks. Monitor the logs to confirm block production.

For a more streamlined deployment experience, consider using the [Polkadot Deployment Portal (PDP)](https://polkadot.polkassembly.io/forum/t/polkadot-deployment-portal-the-1-click-solution-for-polkadot/12176), which simplifies the process of deploying parachains and managing coretime.
For more detailed guidance, refer to the [Zero to Hero: Deploy on Paseo TestNet](https://docs.polkadot.com/tutorials/polkadot-sdk/parachains/zero-to-hero/deploy-to-testnet/) and [Obtain Coretime](https://docs.polkadot.com/tutorials/polkadot-sdk/parachains/zero-to-hero/obtain-coretime/) tutorials.

## Contributing

- 🔄 This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

- ➡️ Any pull requests should be directed to this [source](https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain).

- 😇 Please refer to the monorepo's
  [contribution guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md) and
  [Code of Conduct](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CODE_OF_CONDUCT.md).

## Getting Help

- 🧑‍🏫 To learn about Polkadot in general, [docs.Polkadot.com](https://docs.polkadot.com/) website is a good starting point.

- 🧑‍🔧 For technical introduction, [here](https://github.com/paritytech/polkadot-sdk#-documentation) are
  the Polkadot SDK documentation resources.

- 👥 Additionally, there are [GitHub issues](https://github.com/paritytech/polkadot-sdk/issues) and
  [Substrate StackExchange](https://substrate.stackexchange.com/).
- 👥You can also reach out on the [Official Polkadot discord server](https://polkadot-discord.w3f.tools/)
- 🧑Reach out on [Telegram](https://t.me/substratedevs) for more questions and discussions
