# Polkadot Omni Node Library

Helper library that can be used to run a parachain node.

## Overview

This library can be used to run a parachain node while also customizing the chain specs
that are supported by default by the `--chain-spec` argument of the node's `CLI`
and the parameters of the runtime that is associated with each of these chain specs.

## API

The library exposes the possibility to provide a [`RunConfig`]. Through this structure
2 optional configurations can be provided:
- a chain spec loader (an implementation of [`chain_spec::LoadSpec`]): this can be used for
  providing the chain specs that are supported by default by the `--chain-spec` argument of the
  node's `CLI` and the actual chain config associated with each one.
- a runtime resolver (an implementation of [`runtime::RuntimeResolver`]): this can be used for
  providing the parameters of the runtime that is associated with each of the chain specs

Apart from this, a [`CliConfig`] can also be provided, that can be used to customize some
user-facing binary author, support url, etc.

## Examples

For an example, see the [`polkadot-parachain-bin`](https://crates.io/crates/polkadot-parachain-bin) crate.
