//! # Templates
//!
//! This document enumerates a non-exhaustive list of templates that one can use to get started with
//! polkadot-sdk.
//!
//! > Know more tools/templates that are not listed here? please contribute them by opening a PR.
//!
//! ## Internal
//!
//! The following templates are maintained as a part of the `polkadot-sdk` repository:
//!
//! - [`minimal-template`](https://github.com/paritytech/polkadot-sdk-minimal-template): 
//!   A minimal template that contains the least amount of features to be a functioning blockchain. 
//!   Suitable for learning and testing.
//! - [`parachain-template`](https://github.com/paritytech/polkadot-sdk-solochain-template): 
//!   Formerly known as "substrate-node-template", is a white-labeled substrate-based blockchain 
//!   (aka. solochain) that contains moderate features, such as a basic consensus engine and some 
//!   FRAME pallets. This template can act as a good starting point for those who want to launch 
//!   a solochain.
//! - [`parachain-template`](https://github.com/paritytech/polkadot-sdk-solochain-template): 
// A parachain template ready to be connected to a relay-chain, such as [Paseo]
//! (https://github.com/paseo-network/.github), Kusama  or Polkadot.
//!
//! Note that these templates are mirrored automatically from [this]
//! (https://github.com/paritytech/polkadot-sdk/blob/master/templates) directory of polkadot-sdk,
//! therefore any changes to them should be made as a PR to this repo. 
//!
//! ## OpenZeppelin
//!
//! In June 2023, OpenZeppelin was awarded a grant from the [Polkadot
//! treasury](https://polkadot.polkassembly.io/treasury/406) for building a number of Polkadot-sdk
//! based templates. These templates are expected to be a great starting point for developers.
//!
//! - <https://github.com/OpenZeppelin/polkadot-runtime-template/>
//!
//! ## POP-CLI
//!
//! Is a CLI tool capable of scaffolding a new polkadot-sdk-based project, possibly removing the
//! need for templates.
//!
//! - <https://pop.r0gue.io/cli/>
