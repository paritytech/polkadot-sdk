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