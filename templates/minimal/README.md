<div align="center">

# Polkadot SDK's Minimal Template

<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only"/>
<img height="70px" alt="Polkadot SDK Logo" src="https://github.com/paritytech/polkadot-sdk/raw/master/docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only"/>

> This is a minimal template for creating a [Substrate](https://substrate.io/) blockchain.
> 
> This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

</div>

## Getting Started

Depending on your operating system and Rust version, there might be additional
packages required to compile this template.

Check the
[Substrate Install](https://docs.substrate.io/install/) instructions for your platform for
the most common dependencies.

### Build

Use the following command to build the node without launching it:

```sh
cargo build --release
```

Alternatively, build the docker image:

```sh
docker build . -t polkadot-sdk-minimal-template
```

### Single-Node Development Chain

The following command starts a single-node development chain that doesn't
persist state:

```sh
./target/release/minimal-template-node --dev

# docker version:
docker run --rm polkadot-sdk-minimal-template --dev
```

Development chains:

- Maintain state in a `tmp` folder while the node is running.
- Are preconfigured with a genesis state (see [`chain_spec.rs`](./node/src/chain_spec.rs)) that
  includes several prefunded development accounts.
- Development accounts are used as default validator authorities and a `sudo` account.

### Multi-Node Local Testnet

If you want to see the multi-node consensus algorithm in action, see [Simulate a
network](https://docs.substrate.io/tutorials/build-a-blockchain/simulate-network/).

## Template Structure

A Polkadot SDK based project such as this one consists of:

- a [Node](./node/README.md) - the binary application.
- the [Runtime](./runtime/README.md) - the core logic of the blockchain.
- the [Pallets](./pallets/README.md) - from which the runtime is constructed.

## Contributing

🔄 This template is automatically updated after releases in the main [Polkadot SDK monorepo](https://github.com/paritytech/polkadot-sdk).

➡️ Any pull requests should be directed to this [source](https://github.com/paritytech/polkadot-sdk/tree/master/templates/minimal).

😇 Please refer to the monorepo's [contribution guidelines](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md) and [Code of Conduct](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CODE_OF_CONDUCT.md).

## Getting Help

🧑‍🏫 To learn about Polkadot in general, [Polkadot.network](https://polkadot.network/) website is a good starting point.

🧑‍🔧 For technical introduction, [here](https://github.com/paritytech/polkadot-sdk#-documentation) are the Polkadot SDK documentation resources.

👥 Additionally, there are [GitHub issues](https://github.com/paritytech/polkadot-sdk/issues) and [Substrate StackExchange](https://substrate.stackexchange.com/).
