// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

pub(crate) mod imports {
	pub use cumulus_primitives_core::{ClaimQueueOffset, CoreSelector};
	pub use parachains_common::{AccountId, Balance, Nonce};
	pub use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
	pub use sp_runtime::{
		traits::Block as BlockT,
		transaction_validity::{TransactionSource, TransactionValidity},
		ApplyExtrinsicResult,
	};
	pub use sp_weights::Weight;
}

macro_rules! impl_node_runtime_apis {
	($runtime: ty, $block: tt, $aura_id: ty) => {
		sp_api::impl_runtime_apis! {
			impl sp_api::Core<$block> for $runtime {
				fn version() -> sp_version::RuntimeVersion {
					unimplemented!()
				}

				fn execute_block(_: $block) {
					unimplemented!()
				}

				fn initialize_block(
					_: &<$block as BlockT>::Header
				) -> sp_runtime::ExtrinsicInclusionMode {
					unimplemented!()
				}
			}

			impl sp_api::Metadata<$block> for $runtime {
				fn metadata() -> OpaqueMetadata {
					unimplemented!()
				}

				fn metadata_at_version(_: u32) -> Option<OpaqueMetadata> {
					unimplemented!()
				}

				fn metadata_versions() -> Vec<u32> {
					unimplemented!()
				}
			}

			impl sp_consensus_aura::AuraApi<$block, $aura_id> for $runtime {
				fn slot_duration() -> sp_consensus_aura::SlotDuration {
					unimplemented!()
				}

				fn authorities() -> Vec<$aura_id> {
					unimplemented!()
				}
			}

			impl cumulus_primitives_aura::AuraUnincludedSegmentApi<$block> for $runtime {
				fn can_build_upon(
					_: <$block as BlockT>::Hash,
					_: cumulus_primitives_aura::Slot,
				) -> bool {
					unimplemented!()
				}
			}

			impl sp_block_builder::BlockBuilder<$block> for $runtime {
				fn apply_extrinsic(_: <$block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
					unimplemented!()
				}

				fn finalize_block() -> <$block as BlockT>::Header {
					unimplemented!()
				}

				fn inherent_extrinsics(
					_: sp_inherents::InherentData
				) -> Vec<<$block as BlockT>::Extrinsic> {
					unimplemented!()
				}

				fn check_inherents(
					_: $block,
					_: sp_inherents::InherentData
				) -> sp_inherents::CheckInherentsResult {
					unimplemented!()
				}
			}

			impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<$block> for $runtime {
				fn validate_transaction(
					_: TransactionSource,
					_: <$block as BlockT>::Extrinsic,
					_: <$block as BlockT>::Hash,
				) -> TransactionValidity {
					unimplemented!()
				}
			}

			impl sp_session::SessionKeys<$block> for $runtime {
				fn generate_session_keys(_: Option<Vec<u8>>) -> Vec<u8> {
					unimplemented!()
				}

				fn decode_session_keys(
					_: Vec<u8>,
				) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
					unimplemented!()
				}
			}

			impl
				pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<
					$block,
					Balance,
				> for $runtime
			{
				fn query_info(
					_: <$block as BlockT>::Extrinsic,
					_: u32,
				) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
					unimplemented!()
				}
				fn query_fee_details(
					_: <$block as BlockT>::Extrinsic,
					_: u32,
				) -> pallet_transaction_payment::FeeDetails<Balance> {
					unimplemented!()
				}
				fn query_weight_to_fee(_: Weight) -> Balance {
					unimplemented!()
				}
				fn query_length_to_fee(_: u32) -> Balance {
					unimplemented!()
				}
			}

			impl cumulus_primitives_core::CollectCollationInfo<$block> for $runtime {
				fn collect_collation_info(
					_: &<$block as BlockT>::Header
				) -> cumulus_primitives_core::CollationInfo {
					unimplemented!()
				}
			}

			impl cumulus_primitives_core::GetCoreSelectorApi<$block> for $runtime {
				fn core_selector() -> (CoreSelector, ClaimQueueOffset) {
					unimplemented!()
				}
			}

			#[cfg(feature = "try-runtime")]
			impl frame_try_runtime::TryRuntime<$block> for $runtime {
				fn on_runtime_upgrade(
					_: frame_try_runtime::UpgradeCheckSelect
				) -> (Weight, Weight) {
					unimplemented!()
				}

				fn execute_block(
					_: $block,
					_: bool,
					_: bool,
					_: frame_try_runtime::TryStateSelect,
				) -> Weight {
					unimplemented!()
				}
			}

			impl frame_system_rpc_runtime_api::AccountNonceApi<
				$block,
				AccountId,
				Nonce
			> for $runtime {
				fn account_nonce(_: AccountId) -> Nonce {
					unimplemented!()
				}
			}

			#[cfg(feature = "runtime-benchmarks")]
			impl frame_benchmarking::Benchmark<$block> for $runtime {
				fn benchmark_metadata(_: bool) -> (
					Vec<frame_benchmarking::BenchmarkList>,
					Vec<frame_support::traits::StorageInfo>,
				) {
					unimplemented!()
				}

				fn dispatch_benchmark(
					_: frame_benchmarking::BenchmarkConfig
				) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
					unimplemented!()
				}
			}

			impl sp_genesis_builder::GenesisBuilder<$block> for $runtime {
				fn build_state(_: Vec<u8>) -> sp_genesis_builder::Result {
					unimplemented!()
				}

				fn get_preset(_id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
					unimplemented!()
				}

				fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
					unimplemented!()
				}
			}
		}
	};
}

pub(crate) use impl_node_runtime_apis;
