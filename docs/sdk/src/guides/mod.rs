//! # Polkadot SDK Docs Guides
//!
//! This crate contains a collection of guides that are foundational to the developers of Polkadot
//! SDK. They are common user-journeys that are traversed in the Polkadot ecosystem.
//!
//! 1. [`crate::guides::your_first_pallet`] is your starting point with Polkadot SDK. It contains
//!    the basics of
//! building a simple crypto currency with FRAME.
//! 2. [`crate::guides::your_first_runtime`] is the next step in your journey. It contains the
//!    basics of building a runtime that contains this pallet, plus a few common pallets from FRAME.
//!
//!
//! Other guides are related to other miscellaneous topics and are listed as modules below.

/// Write your first simple pallet, learning the most most basic features of FRAME along the way.
pub mod your_first_pallet;

/// Write your first real [runtime](`crate::reference_docs::wasm_meta_protocol`),
/// compiling it to [WASM](crate::polkadot_sdk::substrate#wasm-build).
pub mod your_first_runtime;

// /// Running the given runtime with a node. No specific consensus mechanism is used at this stage.
// TODO
// pub mod your_first_node;

// /// How to enhance a given runtime and node to be cumulus-enabled, run it as a parachain
// /// and connect it to a relay-chain.
// TODO
// pub mod cumulus_enabled_parachain;

// /// How to make a given runtime XCM-enabled, capable of sending messages (`Transact`) between
// /// itself and the relay chain to which it is connected.
// TODO
// pub mod xcm_enabled_parachain;

/// How to enable storage weight reclaiming in a parachain node and runtime.
pub mod enable_pov_reclaim;

/// How to enable Async Backing on parachain projects that started in 2023 or before.
pub mod async_backing_guide;

/// How to enable metadata hash verification in the runtime.
pub mod enable_metadata_hash;

/// How to enable elastic scaling MVP on a parachain.
pub mod enable_elastic_scaling_mvp;
