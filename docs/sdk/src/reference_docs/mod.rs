//! # Polkadot SDK Reference Docs.
//!
//! This is the entry point for all reference documents that enhance one's learning experience in
//! the Polkadot SDK.
//!
//! Note that this module also contains the [glossary](crate::reference_docs::glossary).
//!
//! ## What is a "reference document"?
//!
//! First, see [why we use rust-docs for everything](crate::meta_contributing#why-rust-docs) and our
//! documentation [principles](crate::meta_contributing#principles). We acknowledge that as much of
//! the crucial information should be embedded in the low level rust-docs. Then, high level
//! scenarios should be covered in [`crate::guides`]. Finally, we acknowledge that there is a
//! category of information that is:
//!
//! 1. Crucial to know.
//! 2. Is too high level to be in the rust-doc of any one `type`, `trait` or `fn`.
//! 3. Is too low level to be encompassed in a [`crate::guides`].
//!
//! We call this class of documents "reference documents". Our goal should be to minimize the number
//! of "reference" docs, as they incur maintenance burden.

/// Learn how Substrate and FRAME use traits and associated types to make modules generic in a
/// type-safe manner.
pub mod trait_based_programming;

/// Learn about the way Substrate and FRAME view their blockchains as state machines.
pub mod blockchain_state_machines;

/// The glossary.
pub mod glossary;

/// Learn about the WASM meta-protocol of all Substrate-based chains.
pub mod wasm_meta_protocol;

/// Learn about the differences between smart contracts and a FRAME-based runtime. They are both
/// "code stored onchain", but how do they differ?
pub mod runtime_vs_smart_contract;

/// Learn about how extrinsics are encoded to be transmitted to a node and stored in blocks.
pub mod extrinsic_encoding;

/// Deprecated in favor of transaction extensions.
pub mod signed_extensions;

/// Learn about the transaction extensions that form a part of extrinsics.
pub mod transaction_extensions;

/// Learn about *Origins*, a topic in FRAME that enables complex account abstractions to be built.
pub mod frame_origin;

/// Learn about the details of what derives are needed for a type to be store-able in `frame`
/// storage.
pub mod frame_storage_derives;

/// Learn about how to write safe and defensive code in your FRAME runtime.
pub mod defensive_programming;

/// Learn about composite enums and other runtime level types, such as `RuntimeEvent` and
/// `RuntimeCall`.
pub mod frame_runtime_types;

/// Learn about how to make a pallet/runtime that is fee-less and instead uses another mechanism to
/// control usage and sybil attacks.
pub mod fee_less_runtime;

/// Learn about metadata, the main means through which an upgradeable runtime communicates its
/// properties to the outside world.
pub mod metadata;

/// Learn about how to add custom host functions to the node.
pub mod custom_host_functions;

/// Learn about how frame-system handles `account-ids`, nonces, consumers and providers.
pub mod frame_system_accounts;

/// Advice for configuring your development environment for Substrate development.
pub mod development_environment_advice;

/// Learn about benchmarking and weight.
pub mod frame_benchmarking_weight;

/// Learn about the token-related logic in FRAME and how to apply it to your use case.
pub mod frame_tokens;

/// Learn about chain specification file and the genesis state of the blockchain.
pub mod chain_spec_genesis;

/// Learn about Substrate's CLI, and how it can be extended.
pub mod cli;

/// Learn about Runtime Upgrades and best practices for writing Migrations.
pub mod frame_runtime_upgrades_and_migrations;

/// Learn about the offchain workers, how they function, and how to use them, as provided by the
/// [`frame`] APIs.
pub mod frame_offchain_workers;

/// Learn about the different ways through which multiple [`frame`] pallets can be combined to work
/// together.
pub mod frame_pallet_coupling;

/// Learn about how to do logging in FRAME-based runtimes.
pub mod frame_logging;

/// Learn about the Polkadot Umbrella crate that re-exports all other crates.
pub mod umbrella_crate;

/// Learn about how to create custom RPC endpoints and runtime APIs.
pub mod custom_runtime_api_rpc;

/// The [`polkadot-omni-node`](https://crates.io/crates/polkadot-omni-node) and its related binaries.
pub mod omni_node;
