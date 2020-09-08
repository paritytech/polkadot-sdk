# Cumulus :cloud:

A set of tools for writing [Substrate](https://substrate.dev/)-based
[Polkadot](https://wiki.polkadot.network/en/)
[parachains](https://wiki.polkadot.network/docs/en/learn-parachains). Refer to the included
[overview](docs/overview.md) for architectural details, and the
[Cumulus workshop](https://substrate.dev/cumulus-workshop) for a hand-holding walkthrough
of using these tools.

It's easy to write blockchains using Substrate, and the overhead of writing parachains'
distribution, p2p, database, and synchronization layers should be just as low. This project aims to
make it easy to write parachains for Polkadot by leveraging the power of Substrate.

Cumulus clouds are shaped sort of like dots; together they form a system that is intricate,
beautiful and functional.

## Consensus

[`cumulus-consensus`](consensus) is a
[consensus engine](https://substrate.dev/docs/en/knowledgebase/advanced/consensus) for Substrate
that follows a Polkadot
[relay chain](https://wiki.polkadot.network/docs/en/learn-architecture#relay-chain). This will run a
Polkadot node internally, and dictate to the client and synchronization algorithms which chain to
follow,
[finalize](https://wiki.polkadot.network/docs/en/learn-consensus#probabilistic-vs-provable-finality),
and treat as best.

## Runtime

The [`cumulus-runtime`](runtime) is wrapper around Substrate runtimes that provides parachain
validation capabilities and proof-generation routines.

## Collator

A Polkadot [collator](https://wiki.polkadot.network/docs/en/maintain-collator) for the parachain is
implemented by [`cumulus-collator`](collator).

# Rococo :crown:

[Rococo](https://polkadot.js.org/apps/?rpc=wss://rococo-rpc.polkadot.io) is the testnet for
parachains. It currently runs the parachains
[Tick](https://polkadot.js.org/apps/?rpc=wss://tick-rpc.polkadot.io),
[Trick](https://polkadot.js.org/apps/?rpc=wss://trick-rpc.polkadot.io) and
[Track](https://polkadot.js.org/apps/?rpc=wss://track-rpc.polkadot.io).

Rococo is an elaborate style of design and the name describes the painstaking effort that has gone
into this project. Tick, Trick and Track are the German names for the cartoon ducks known to English
speakers as Huey, Dewey and Louie.

## Build & Launch Rococo Collators

Collators are similar to validators in the relay chain. These nodes build the blocks that will
eventually be included by the relay chain for a parachain.

To run a Rococo collator you will need to compile the following binary:

```
cargo build --release -p rococo-collator
```

Once the executable is built, launch collators for each parachain (repeat once each for chain
`tick`, `trick`, `track`):

```
./target/release/rococo-collator --chain $CHAIN --validator
```

## Parachains

The parachains of Rococo all use the same runtime code. The only difference between them is the
parachain ID used for registration with the relay chain:

-   Tick: 100
-   Trick: 110
-   Track: 120

The network uses horizontal message passing (HRMP) to enable communication between parachains and
the relay chain and, in turn, between parachains. This means that every message is sent to the relay
chain, and from the relay chain to its destination parachain.
