//! # Substrate
//!
//! Substrate is a Rust framework for building blockchains in a modular and extensible way. While in
//! itself un-opinionated, it is the main engine behind the Polkadot ecosystem.
//!
//! ## Overview, Philosophy
//!
//! Substrate approaches blockchain development with an acknowledgement of a few self-evident
//! truths:
//!
//! 1. Society and technology evolves.
//! 2. Humans are fallible.
//!
//! This, makes the task of designing a correct, safe and long-lasting blockchain system hard.
//!
//! Nonetheless, in strive towards achieve this goal, Substrate embraces the following:
//!
//! 1. Use of **Rust** as a modern and safe programming language, which limits human error through
//!    various means, most notably memory and type safety.
//! 2. Substrate is written from the ground-up with a *generic, modular and extensible* design. This
//!    ensures that software components can be easily swapped and upgraded. Examples of this is
//!    multiple consensus mechanisms provided by Substrate, as listed below.
//! 3. Lastly, the final blockchain system created with the above properties needs to be
//!    upgradeable. In order to achieve this, Substrate is designed as a meta-protocol, whereby the
//!    application logic of the blockchain (called "Runtime") is encoded as a WASM blob, and is
//!    stored in the state. The rest of the system (called "node") acts as the executor of the WASM
//!    blob.
//!
//! In essence, the meta-protocol of all Substrate based chains is the "Runtime as WASM blob"
//! accord. This enables the Runtime to become inherently upgradeable, crucially without forks. The
//! upgrade is merely a matter of the WASM blob being changed in the state, which is, in principle,
//! same as updating an account's balance. Learn more about this in detail in
//! [`crate::reference_docs::wasm_meta_protocol`].
//!
//! > A great analogy for substrate is the following: Substrate node is a gaming console, and a WASM
//! > runtime, possibly created with FRAME is the game being inserted into the console.
//!
//! [`frame`], Substrate's default runtime development library, takes the above safety practices
//! even further by embracing a declarative programming model whereby correctness is enhanced and
//! the system is highly configurable through parameterization. Learn more about this in
//! [`crate::reference_docs::trait_based_programming`].
//!
//! ## How to Get Started
//!
//! Substrate offers different options at the spectrum of technical freedom <-> development ease.
//!
//! * The easiest way to use Substrate is to use one of the templates (some of which listed at
//!   [`crate::polkadot_sdk::templates`]) and only tweak the parameters of the runtime or node. This
//!   allows you to launch a blockchain in minutes, but is limited in technical freedom.
//! * Next, most developers wish to develop their custom runtime modules, for which the de-facto way
//! is [`frame`](crate::polkadot_sdk::frame_runtime).
//! * Finally, Substrate is highly configurable at the node side as well, but this is the most
//!   technically demanding.
//!
//! > A notable Substrate-based blockchain that has built both custom FRAME pallets and custom
//! > node-side components is <https://github.com/Cardinal-Cryptography/aleph-node>.
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_dev.mmd")]
//!
//! ## Structure
//!
//! Substrate contains a large number of crates, therefore it is useful to have an overview of what
//! they are, and how they are organized. In broad terms, these crates are divided into three
//! categories:
//!
//! * `sc-*` (short for *Substrate-client*) crates, located under `./client` folder. These are all
//!   the crates that lead to the node software. Notable examples [`sc_network`], various consensus
//!   crates, RPC ([`sc_rpc_api`]) and database ([`sc_client_db`]), all of which are expected to
//!   reside in the node side.
//! * `sp-*` (short for *substrate-primitives*) crates, located under `./primitives` folder. These
//!   are crates that facilitate both the node and the runtime, but are not opinionated about what
//!   framework is using for building the runtime. Notable examples are [`sp_api`] and [`sp_io`],
//!   which form the communication bridge between the node and runtime.
//! * `pallet-*` and `frame-*` crates, located under `./frame` folder. These are the crates related
//!   to FRAME. See [`frame`] for more information.
//!
//! ### WASM Build
//!
//! Many of the Substrate crates, such as entire `sp-*`, need to compile to both WASM (when a WASM
//! runtime is being generated) and native (for example, when testing). To achieve this, Substrate
//! follows the convention of the Rust community, and uses a `feature = "std"` to signify that a
//!  crate is being built with the standard library, and is built for native. Otherwise, it is built
//!  for `no_std`.
//!
//! This can be summarized in `#![cfg_attr(not(feature = "std"), no_std)]`, which you can often find
//! in any Substrate-based runtime.
//!
//! Substrate-based runtimes use [`substrate_wasm_builder`] in their `build.rs` to automatically
//! build their WASM files as a part of normal build command (e.g. `cargo build`). Once built, the
//! wasm file is placed in `./target/{debug|release}/wbuild/{runtime_name}.wasm`.
//!
//! ### Binaries
//!
//! Multiple binaries are shipped with substrate, the most important of which are located in the
//! [`./bin`](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/bin) folder.
//!
//! * [`node_cli`] is an extensive substrate node that contains the superset of all runtime and node
//!   side features. The corresponding runtime, called [`kitchensink_runtime`] contains all of the
//!   modules that are provided with `FRAME`. This node and runtime is only used for testing and
//!   demonstration.
//!     * [`chain_spec_builder`]: Utility to build more detailed chain-specs for the aforementioned
//!       node. Other projects typically contain a `build-spec` subcommand that does the same.
//! * [`node_template`](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/bin/node-template):
//!   a template node that contains a minimal set of features and can act as a starting point of a
//!   project.
//! * [`subkey`]: Substrate's key management utility.
//!
//! ### Anatomy of a Binary Crate
//!
//! From the above, [`node_cli`]/[`kitchensink_runtime`] and `node-template` are essentially
//! blueprints of a Substrate-based project, as the name of the latter is implying. Each
//! Substrate-based project typically contains the following:
//!
//! * Under `./runtime`, a `./runtime/src/lib.rs` which is the top level runtime amalgamator file.
//!   This file typically contains the [`frame::runtime::prelude::construct_runtime`] and
//!   [`frame::runtime::prelude::impl_runtime_apis`] macro calls, which is the final definition of a
//!   runtime.
//!
//! * Under `./node`, a `main.rs`, which is the starting point, and a `./service.rs`, which contains
//!   all the node side components. Skimming this file yields an overview of the networking,
//!   database, consensus and similar node side components.
//!
//! > The above two are conventions, not rules.
//!
//! > See <https://github.com/paritytech/polkadot-sdk/issues/5> for an update on how the node side
//! > components are being amalgamated.
//!
//! ## Parachain?
//!
//! As noted above, Substrate is the main engine behind the Polkadot ecosystem. One of the ways
//! through which Polkadot can be utilized is by building "parachains", blockchains that are
//! connected to Polkadot's shared security.
//!
//! To build a parachain, one could use [Cumulus](crate::polkadot_sdk::cumulus), the library on
//! top of Substrate, empowering any substrate-based chain to be a Polkadot parachain.
//!
//! ## Where To Go Next?
//!
//! Additional noteworthy crates within substrate:
//!
//! - RPC APIs of a Substrate node: [`sc_rpc_api`]/[`sc_rpc`]
//! - CLI Options of a Substrate node: [`sc_cli`]
//! - All of the consensus related crates provided by Substrate:
//!     - [`sc_consensus_aura`]
//!     - [`sc_consensus_babe`]
//!     - [`sc_consensus_grandpa`]
//!     - [`sc_consensus_beefy`] (TODO: @adrian, add some high level docs <https://github.com/paritytech/polkadot-sdk-docs/issues/57>)
//!     - [`sc_consensus_manual_seal`]
//!     - [`sc_consensus_pow`]

#[doc(hidden)]
pub use crate::polkadot_sdk;
