//! # Substrate
//!
//! itself un-opinionated, it is the main engine behind the Polkadot ecosystem.
//!
//!
//! truths:
//!
//! 2. Humans are fallible.
//!
//!
//!
//!    various means, most notably memory and type safety.
//!    ensures that software components can be easily swapped and upgraded. Examples of this is
//! 3. Lastly, the final blockchain system created with the above properties needs to be
//!    application logic of the blockchain (called "Runtime") is encoded as a WASM blob, and is
//!    blob.
//!
//! accord. This enables the Runtime to become inherently upgradeable, crucially without [`forks`]). The
//! same as updating an account's balance. Learn more about this in detail in
//!
//! > runtime, possibly created with FRAME is the game being inserted into the console.
//!
//! even further by embracing a declarative programming model whereby correctness is enhanced and
//! [`trait_based_programming`].
//!
//!
//!
//!   [`templates`]) and only tweak the parameters of the runtime or node. This
//! * Next, most developers wish to develop their custom runtime modules, for which the de-facto way
//! * Finally, Substrate is highly configurable at the node side as well, but this is the most
//!
//! > node-side components is <https://github.com/Cardinal-Cryptography/aleph-node>.
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_dev.mmd")]
//!
//!
//! they are, and how they are organized. In broad terms, these crates are divided into three
//!
//!   the crates that lead to the node software. Notable examples are [`sc_network`], various
//!   expected to reside in the node side.
//!   are crates that facilitate both the node and the runtime, but are not opinionated about what
//!   which form the communication bridge between the node and runtime.
//!   to FRAME. See [`frame`] for more information.
//!
//!
//! runtime is being generated) and native (for example, when testing). To achieve this, Substrate
//!  crate is being built with the standard library, and is built for native. Otherwise, it is built
//!
//! in any Substrate-based runtime.
//!
//! build their WASM files as a part of normal build command (e.g. `cargo build`). Once built, the
//!
//!
//!
//! blueprints of a Substrate-based project, as the name of the latter is implying. Each
//!
//!   This file typically contains the [`construct_runtime`] and
//!   runtime.
//!
//!   all the node side components. Skimming this file yields an overview of the networking,
//!
//!
//! > components are being amalgamated.
//!
//!
//! through which Polkadot can be utilized is by building "parachains", blockchains that are
//!
//! top of Substrate, empowering any substrate-based chain to be a Polkadot parachain.
//!
//!
//!
//! - CLI Options of a Substrate node: [`sc_cli`]
//!     - [`sc_consensus_aura`]
//!     - [`sc_consensus_grandpa`]
//!     - [`sc_consensus_manual_seal`]

#[doc(hidden)]
pub use crate::polkadot_sdk;

// Link References
// [`trait_based_programming`]: crate::reference_docs::trait_based_programming

// Link References
// [`templates`]: crate::polkadot_sdk::templates
// [`wasm_meta_protocol`]: crate::reference_docs::wasm_meta_protocol

// [`impl_runtime_apis`]: frame::runtime::prelude::impl_runtime_apis

// [`Cumulus`]: crate::polkadot_sdk::cumulus
// [`Substrate Runtime Toolbox (srtool)`]: srtool)](https://github.com/paritytech/srtool
// [`forks`]: https://en.wikipedia.org/wiki/Fork_(blockchain
