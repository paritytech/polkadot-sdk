//! # Polkadot SDK
//!
//! The [Polkadot SDK repository](https://github.com/paritytech/polkadot-sdk) provides all the
//! resources needed to start building on the [Polkadot network](https://polkadot.network), a
//! multi-chain blockchain platform that enables different blockchains to interoperate and share
//! information in a secure and scalable way.
//!
//! [![StackExchange](https://img.shields.io/badge/StackExchange-Polkadot%20and%20Substrate-222222?logo=stackexchange)](https://substrate.stackexchange.com/)
//!
//! [![awesomeDot](https://img.shields.io/badge/polkadot-awesome-e6007a?logo=polkadot)](https://github.com/Awsmdot/awesome-dot)
//! [![awesomeDot](https://img.shields.io/badge/polkadot-wiki-e6007a?logo=polkadot)](https://wiki.polkadot.network/)
//! [![awesomeDot](https://img.shields.io/badge/polkadot-forum-e6007a?logo=polkadot)](https://forum.polkadot.network/)
//!
//! [![RFCs](https://img.shields.io/badge/fellowship-RFCs-e6007a?logo=polkadot)](https://github.com/polkadot-fellows/rfcs)
//! [![Runtime](https://img.shields.io/badge/fellowship-runtimes-e6007a?logo=polkadot)](https://github.com/polkadot-fellows/runtimes)
//! [![Manifesto](https://img.shields.io/badge/fellowship-manifesto-e6007a?logo=polkadot)](https://github.com/polkadot-fellows/manifesto)
//!
//!
//! ## Getting Started
//!
//! The primary way to get started with the Polkadot SDK is to start writing a Substrate-based
//! runtime using FRAME. See:
//!
//! 1. [`substrate`], for an overview of what Substrate is.
//! 2. Jump right into [`frame`] to learn about how to write a FRAME-based blockchain runtime.
//! 3. Continue with the [`developer_hub`'s "getting started"](crate#getting-started).
//!
//! ## Structure
//!
//! This repository is a nested
//! [workspace](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html), containing the
//! following major software artifacts:
//!
//! #### Substrate
//!
//! [![Substrate-license](https://img.shields.io/badge/License-GPL3%2FApache2.0-blue)](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/LICENSE-APACHE2)
//! [![GitHub Repo](https://img.shields.io/badge/github-substrate-2324CC85)](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame)
//!
//! [`substrate`] is the base blockchain framework used to power the Polkadot SDK. It is a full
//! toolkit to create sovereign blockchains, including but not limited to those who connect to
//! Polkadot as parachains.
//!
//! #### FRAME
//!
//! [![Substrate-license](https://img.shields.io/badge/License-Apache2.0-blue)](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/LICENSE-APACHE2)
//! [![GitHub Repo](https://img.shields.io/badge/github-frame-2324CC85)](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame)
//!
//! [`frame`] is the framework used to create Substrate-based runtimes. Learn more
//!
//! #### Cumulus
//!
//! [![Cumulus-license](https://img.shields.io/badge/License-GPL3-blue)](https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/LICENSE)
//! [![GitHub Repo](https://img.shields.io/badge/github-cumulus-white)](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/cumulus)
//!
//! [`cumulus`] transforms FRAME-based runtimes into Polkadot-compatible parachain runtimes, and
//! substrate-based client into Polkadot-compatible collators.
//!
//! #### Polkadot
//!
//! [![Polkadot-license](https://img.shields.io/badge/License-GPL3-blue)](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/LICENSE)
//! [![GitHub Repo](https://img.shields.io/badge/github-polkadot-e6007a?logo=polkadot)](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/polkadot)
//!
//! Recall from [Substrate's architecture](`polkadot_sdk::substrate#architecture`) that any
//! substrate-based chain is composed of two parts: A client, and a runtime.
//!
//! [`polkadot`] is an implementation of a Polkadot client in Rust, by `@paritytech`. The Polkadot
//! runtimes are located under the `fellowship/runtime` repository.
//!
//! > [`polkadot`] contains useful links to further learn about Polkadot, **but is in general not
//! > part of the SDK**, as it is rarely used by developers who wish to build on top of Polkadot.
//!
//! ### Summary
//!
//! The following diagram summarizes how components of the Polkadot-SDK work together:
#![doc = simple_mermaid::mermaid!("../../../docs/mermaid/polkadot_sdk.mmd")]
//!
//! 1. A Substrate-based chain is a blockchain composed of a "Runtime" and a "Client". As noted
//!    above, the "Runtime" is the application logic of the blockchain, and the "Client" is
//!    everything else. See [`reference_docs::wasm_meta_protocol`] for an in-depth explanation of
//!    this. The former is built with [`frame`], and the latter is built with Substrate client
//!    libraries.
//! 2. Polkadot is itself a Substrate-based chain, composed of the the exact same two components.
//!    The Polkadot client code is in [`polkadot`], and the Polkadot runtimes are controlled by the
//!    Polkadot Fellowship.
//! 3. A parachain is a "special" Substrate based chain, whereby both the client and the runtime
//!    components have became "Polkadot-aware" using Cumulus.
//!
//! ## History
//!
//! Substrate, Polkadot and Cumulus used to each have their own repository, each of which is now
//! archived. For historical context about how they merged into this mono-repo, see:
//!
//! - <https://polkadot-public.notion.site/Polkadot-SDK-FAQ-fbc4cecc2c46443fb37b9eeec2f0d85f>
//! - <https://forum.polkadot.network/t/psa-parity-is-currently-working-on-merging-the-polkadot-stack-repositories-into-one-single-repository/2883>
//!
//! [`substrate`]: crate::polkadot_sdk::substrate
//! [`frame`]: crate::polkadot_sdk::frame_runtime
//! [`cumulus`]: crate::polkadot_sdk::cumulus
//! [`polkadot`]: crate::polkadot_sdk::polkadot
//!
//! ## Notable Upstream Crates:
//!
//! - [`parity-scale-codec`](https://github.com/paritytech/parity-scale-codec)
//! - [`parity-db`](https://github.com/paritytech/parity-db)
//! - [`trie`](https://github.com/paritytech/trie)
//! - [`parity-common`](https://github.com/paritytech/parity-common)

/// Lean about Cumulus, the framework that transforms [`substrate`]-based chains into
/// [`polkadot`]-enabled parachains.
pub mod cumulus;
/// Learn about FRAME, the framework used to build Substrate runtimes.
pub mod frame_runtime;
/// Learn about Polkadot as a platform.
pub mod polkadot;
/// Learn about different ways through which smart contracts can be utilized on top of Substrate,
/// and in the Polkadot ecosystem.
pub mod smart_contracts;
/// Learn about Substrate, the main blockchain framework used in the Polkadot ecosystem.
pub mod substrate;
/// Index of all the templates that can act as an initial scaffold for a new project.
pub mod templates;
