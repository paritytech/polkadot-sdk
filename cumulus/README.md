# Cumulus

A set of tools for writing [Polkadot](https://github.com/paritytech/polkadot) parachains that are based on [Substrate](https://github.com/paritytech/substrate).

It's easy to write blockchains using Substrate, and the overhead of writing parachains' distribution, p2p, database, and synchronization layers is generally high and should be reusable. This project aims to make it easy to write parachains for Polkadot by leveraging the power of Substrate.

Cumulus clouds are shaped sort of like dots and are up in the air, like this project (as it is an initial prototype -- expect a rename when it gets cooler.)

## cumulus-consensus

For now, this is only project contained in this repo. *cumulus-consensus* is a consensus engine for Substrate which follows a Polkadot relay chain. This will run a Polkadot node internally, and dictate to the client and synchronization algorithms which chain to follow, finalize, and treat as best.

## cumulus-runtime

A planned wrapper around substrate runtimes to turn them into parachain validation code and to provide proof-generation routines.

## cumulus-collator

A planned Polkadot collator for the parachain.

## Rococo

Rococo is the testnet for parachains. It currently runs the parachains `Tick`, `Trick` and `Track`.

### Running a collator

Collators are similar to validators in the relay chain. These nodes build the blocks that will eventually be included by the relay chain for a parachain.

To run a collator on this test network you will need to compile the following binary:

```
cargo build --release -p rococo-collator
```

After the build is finished you can use the binary to run a collator for all three parachains:

```
./target/release/rococo-collator --chain tick --validator
```

This will run the collator for the `Tick` parachain. To run a collator for one of the other nodes, the chain argument needs to be changed.

### Running a full node

To run a full node that should sync one of the parachains, you need to compile the following binary:

```
cargo build --release -p rococo-collator
```

After the build is finished you can use the binary to run a collator for all three parachains:

```
./target/release/rococo-collator --chain tick
```

### Tick, Trick and Track

These are the parachains of Rococo, essentially all run the exact same runtime code. The only difference is the parachain ID they are registered
with on the relay chain. `Tick` is using `100`, `Trick` `110` and `Track` `120`. The parachains demonstrate message
passing between themselves and the relay chain. The message passing is currently implemented as a
HRMP (Horizontal Message Passing). This means that every message is send to the relay chain and from the relay
chain to its destination parachain.
