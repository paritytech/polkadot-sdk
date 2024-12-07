//! # Enable storage weight reclaiming
//!
//! This guide will teach you how to enable storage weight reclaiming for a parachain. The
//! explanations in this guide assume a project structure similar to the one detailed in
//! the [substrate documentation](crate::polkadot_sdk::substrate#anatomy-of-a-binary-crate). Full
//! technical details are available in the original [pull request](https://github.com/paritytech/polkadot-sdk/pull/3002).
//!
//! # What is PoV reclaim?
//! When a parachain submits a block to a relay chain like Polkadot or Kusama, it sends the block
//! itself and a storage proof. Together they form the Proof-of-Validity (PoV). The PoV allows the
//! relay chain to validate the parachain block by re-executing it. Relay chain
//! validators distribute this PoV among themselves over the network. This distribution is costly
//! and limits the size of the storage proof. The storage weight dimension of FRAME weights reflects
//! this cost and limits the size of the storage proof. However, the storage weight determined
//! during [benchmarking](crate::reference_docs::frame_benchmarking_weight) represents the worst
//! case. In reality, runtime operations often consume less space in the storage proof. PoV reclaim
//! offers a mechanism to reclaim the difference between the benchmarked worst-case and the real
//! proof-size consumption.
//!
//!
//! # How to enable PoV reclaim
//! ## 1. Add the host function to your node
//!
//! To reclaim excess storage weight, a parachain runtime needs the
//! ability to fetch the size of the storage proof from the node. The reclaim
//! mechanism uses the
//! [`storage_proof_size`](cumulus_primitives_proof_size_hostfunction::storage_proof_size)
//! host function for this purpose. For convenience, cumulus provides
//! [`ParachainHostFunctions`](cumulus_client_service::ParachainHostFunctions), a set of
//! host functions typically used by cumulus-based parachains. In the binary crate of your
//! parachain, find the instantiation of the [`WasmExecutor`](sc_executor::WasmExecutor) and set the
//! correct generic type.
//!
//! This example from the parachain-template shows a type definition that includes the correct
//! host functions.
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", wasm_executor)]
//!
//! > **Note:**
//! >
//! > If you see error `runtime requires function imports which are not present on the host:
//! > 'env:ext_storage_proof_size_storage_proof_size_version_1'`, it is likely
//! > that this step in the guide was not set up correctly.
//!
//! ## 2. Enable storage proof recording during import
//!
//! The reclaim mechanism reads the size of the currently recorded storage proof multiple times
//! during block authoring and block import. Proof recording during authoring is already enabled on
//! parachains. You must also ensure that storage proof recording is enabled during block import.
//! Find where your node builds the fundamental substrate components by calling
//! [`new_full_parts`](sc_service::new_full_parts). Replace this
//! with [`new_full_parts_record_import`](sc_service::new_full_parts_record_import) and
//! pass `true` as the last parameter to enable import recording.
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", component_instantiation)]
//!
//! > **Note:**
//! >
//! > If you see error `Storage root must match that calculated.` during block import, it is likely
//! > that this step in the guide was not
//! > set up correctly.
//!
//! ## 3. Add the TransactionExtension to your runtime
//!
//! In your runtime, you will find a list of TransactionExtensions.
//! To enable the reclaiming,
//! add [`StorageWeightReclaim`](cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim)
//! to that list. For maximum efficiency, make sure that `StorageWeightReclaim` is last in the list.
//! The extension will check the size of the storage proof before and after an extrinsic execution.
//! It reclaims the difference between the calculated size and the benchmarked size.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", template_signed_extra)]
//!
//! ## Optional: Verify that reclaim works
//!
//! Start your node with the log target `runtime::storage_reclaim` set to `trace` to enable full
//! logging for `StorageWeightReclaim`. The following log is an example from a local testnet. To
//! trigger the log, execute any extrinsic on the network.
//!
//! ```ignore
//! ...
//! 2024-04-22 17:31:48.014 TRACE runtime::storage_reclaim: [ferdie] Reclaiming storage weight. benchmarked: 3593, consumed: 265 unspent: 0
//! ...
//! ```
//!
//! In the above example we see a benchmarked size of 3593 bytes, while the extrinsic only consumed
//! 265 bytes of proof size. This results in 3328 bytes of reclaim.
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
