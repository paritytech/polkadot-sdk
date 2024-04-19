//! This guide will teach you how to enable storage weight reclaiming for a parachain.
//!
//! # 1. Add the host function to your node
//!
//! In order to reclaim excess storage weight that was benchmarked, your parachain runtime needs to
//! be able to fetch the size of the storage proof from the runtime. To do this, it needs access to
//! the [storage_proof_size](cumulus_primitives_proof_size_hostfunction::storage_proof_size) host
//! function. For convenience, cumulus provides
//! [ParachainHostFunctions](cumulus_client_service::ParachainHostFunctions), a set of typically
//! expected hostfunctions typically expected by parachain runtimes.
//!
//! ## WasmExecutor
//! If your node is using the [WasmExecutor][`sc_executor::WasmExecutor`], add the hostfunctions
//! like this:
//! ```rust
//! 	let executor = WasmExecutor::<ParachainHostFunctions>::builder()
//! 	.with_execution_method(config.wasm_method)
//! 	.with_onchain_heap_alloc_strategy(heap_pages)
//! 	.with_offchain_heap_alloc_strategy(heap_pages)
//! 	.with_max_runtime_instances(config.max_runtime_instances)
//! 	.with_runtime_cache_size(config.runtime_cache_size)
//! 	.build();
//! ```
//!
//! # 2. Enable storage proof recording during import
//!
//! The reclaim mechanism reads the size of the currently recorded storage proof multiple times
//! during block execution. This entails that the host function to query the storage proof size will
//! also be called during block import. Therefore we need to make sure that storage proof recording
//! is enabled during block import. In your project find the place where your build node components
//! by calling [new_full_parts](sc_service::new_full_parts). Replace this by
//! [new_full_parts_record_import](sc_service::new_full_parts_record_import) and make sure to pass
//! `true` as the last parameter to enable import recording.
//!
//! # 3. Add the SignedExtension to your runtime
//!
//! In your runtime, you will find a list of SignedExtensions.
//! ```rust
//! pub type SignedExtra = (
//! frame_system::CheckNonZeroSender<Runtime>,
//! frame_system::CheckSpecVersion<Runtime>,
//! frame_system::CheckTxVersion<Runtime>,
//! frame_system::CheckGenesis<Runtime>,
//! frame_system::CheckEra<Runtime>,
//! frame_system::CheckNonce<Runtime>,
//! frame_system::CheckWeight<Runtime>,
//! pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
//! cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim<Runtime>,
//! ;
//! ```
//! To enable reclaim,
//! just add [StorageWeightReclaim](cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim)
//! to that list.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
