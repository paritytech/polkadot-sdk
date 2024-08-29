//! # Templates
//!
//! This document enumerates a non-exhaustive list of templates that one can use to get started with
//! polkadot-sdk.
//!
//! > Know more tools/templates that are not listed here? please contribute them by opening a PR.
//!
//! ## Internal
//!
//! The following [templates](https://github.com/paritytech/polkadot-sdk/blob/master/templates) are
//! maintained as a part of the `polkadot-sdk` repository:
//!
//! - `minimal_template_node`/[`minimal_template_runtime`]: A minimal template that contains the
//!   least amount of features to be a functioning blockchain. Suitable for learning, development
//!   and testing. This template is not meant to be used in production.
//! - `solochain_template_node`/[`solochain_template_runtime`]: Formerly known as
//!   "substrate-node-template", is a white-labeled substrate-based blockchain (aka. solochain) that
//!   contains moderate features, such as a basic consensus engine and some FRAME pallets. This
//!   template can act as a good starting point for those who want to launch a solochain.
//! - `parachain_template_node`/[`parachain_template_runtime`]: A parachain template ready to be
//!   connected to a test relay-chain.
//!
//! These templates are always kept up to date, and are mirrored to external repositories for easy
//! forking:
//!
//! - <https://github.com/paritytech/polkadot-sdk-minimal-template>
//! - <https://github.com/paritytech/polkadot-sdk-solochain-template>
//! - <https://github.com/paritytech/polkadot-sdk-parachain-template>
//!
//! ## External Templates
//!
//! Noteworthy templates outside of this repository.
//!
//! - [`extended-parachain-template`](https://github.com/paritytech/extended-parachain-template): A
//!   parachain template that contains more built-in functionality such as assets and NFTs.
//! - [`frontier-parachain-template`](https://github.com/paritytech/frontier-parachain-template): A
//!   parachain template for launching EVM-compatible parachains.
//!
//! ## OpenZeppelin
//!
//! In June 2023, OpenZeppelin was awarded a grant from the [Polkadot
//! treasury](https://polkadot.polkassembly.io/treasury/406) for building a number of Polkadot-sdk
//! based templates. These templates are a great starting point for developers and newcomers.
//! So far OpenZeppelin has released two templates, which have been fully [audited](https://github.com/OpenZeppelin/polkadot-runtime-templates/tree/main/audits):
//! - [`generic-runtime-template`](https://github.com/OpenZeppelin/polkadot-runtime-templates?tab=readme-ov-file#generic-runtime-template):
//!   A minimal template that has all the common pallets that parachains use with secure defaults.
//! - [`evm-runtime-template`](https://github.com/OpenZeppelin/polkadot-runtime-templates/tree/main?tab=readme-ov-file#evm-template):
//! This template has EVM compatibility out of the box and allows migrating your solidity contracts
//! or EVM compatible dapps easily. It also uses 20 byte addresses like Ethereum and has some
//! Account Abstraction support.
//!
//! ## POP-CLI
//!
//! Is a CLI tool capable of scaffolding a new polkadot-sdk-based project, possibly removing the
//! need for templates.
//!
//! - <https://pop.r0gue.io/cli/>
