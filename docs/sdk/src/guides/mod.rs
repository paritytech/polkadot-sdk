//! # Polkadot SDK Docs Guides
//!
//! This crate contains a collection of guides that are foundational to the developers of
//! Polkadot SDK. They are common user-journeys that are traversed in the Polkadot ecosystem.

/// Write your first simple pallet, learning the most most basic features of FRAME along the way.
pub mod your_first_pallet;

/// Writing your first real [runtime](`crate::reference_docs::wasm_meta_protocol`), and successfully
/// compiling it to [WASM](crate::polkadot_sdk::substrate#wasm-build).
pub mod your_first_runtime;

/// Running the given runtime with a node. No specific consensus mechanism is used at this stage.
pub mod your_first_node;

/// How to change the consensus engine of both the node and the runtime.
pub mod changing_consensus;

/// How to enhance a given runtime and node to be cumulus-enabled, run it as a parachain and connect
/// it to a relay-chain.
pub mod cumulus_enabled_parachain;

/// How to make a given runtime XCM-enabled, capable of sending messages (`Transact`) between itself
/// and the relay chain to which it is connected.
pub mod xcm_enabled_parachain;
