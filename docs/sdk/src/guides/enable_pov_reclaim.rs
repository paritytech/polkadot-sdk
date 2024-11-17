//! # Enable storage weight reclaiming
//!
//! explanations in this guide assume a project structure similar to the one detailed in
//! technical details are available in the original [`pull request`].
//!
//! When a parachain submits a block to a relay chain like Polkadot or Kusama, it sends the block
//! relay chain to validate the parachain block by re-executing it. Relay chain
//! and limits the size of the storage proof. The storage weight dimension of FRAME weights reflects
//! during [`benchmarking`] represents the worst
//! offers a mechanism to reclaim the difference between the benchmarked worst-case and the real
//!
//!
//! ## 1. Add the host function to your node
//!
//! ability to fetch the size of the storage proof from the node. The reclaim
//! [`storage_proof_size`]
//! [`ParachainHostFunctions`], a set of
//! parachain, find the instantiation of the [`WasmExecutor`] and set the
//!
//! host functions.
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", wasm_executor)]
//!
//! >
//! > 'env:ext_storage_proof_size_storage_proof_size_version_1'`, it is likely
//!
//!
//! during block authoring and block import. Proof recording during authoring is already enabled on
//! Find where your node builds the fundamental substrate components by calling
//! with [`new_full_parts_record_import`] and
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", component_instantiation)]
//!
//! >
//! > that this step in the guide was not
//!
//!
//! To enable the reclaiming,
//! to that list. For maximum efficiency, make sure that `StorageWeightReclaim` is last in the list.
//! It reclaims the difference between the calculated size and the benchmarked size.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", template_signed_extra)]
//!
//!
//! logging for `StorageWeightReclaim`. The following log is an example from a local testnet. To
//!
//! ...
//! ...
//!
//! 265 bytes of proof size. This results in 3328 bytes of reclaim.
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

// [`ParachainHostFunctions`]: cumulus_client_service::ParachainHostFunctions
// [`StorageWeightReclaim`]: cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim
// [`WasmExecutor`]: sc_executor::WasmExecutor
// [`benchmarking`]: crate::reference_docs::frame_benchmarking_weight
// [`new_full_parts`]: sc_service::new_full_parts
// [`new_full_parts_record_import`]: sc_service::new_full_parts_record_import
// [`pull request`]: https://github.com/paritytech/polkadot-sdk/pull/3002
// [`storage_proof_size`]: cumulus_primitives_proof_size_hostfunction::storage_proof_size
// [`substrate documentation`]: crate::polkadot_sdk::substrate#anatomy-of-a-binary-crate
