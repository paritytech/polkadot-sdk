# Parachains

This directory is the home of Parity-developed parachain runtimes. This directory is _runtime
focused_, and does not include builds of parachain _nodes_.

The general internal structure is:

- `chain-specs`: Chain specs for the runtimes contained in its sibling dir `runtimes`.
- `common`: Common configurations, `impl`s, etc. used by several parachain runtimes.
- `integration-tests`: Integration tests to test parachain interactions via XCM.
- `pallets`: FRAME pallets that are specific to parachains.
- `runtimes`: The entry point for parachain runtimes.

## Common Good Parachains

The `runtimes` directory includes many, but is not limited to,
[common good parachains](https://wiki.polkadot.network/docs/learn-common-goods). Likewise, not all
common good parachains are in this repo.

## Releases

The project maintainers generally try to release a set of parachain runtimes for each Polkadot
Relay Chain runtime release.
