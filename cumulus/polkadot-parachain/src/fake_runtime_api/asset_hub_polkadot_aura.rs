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

//! These are used to provide a type that implements these runtime APIs without requiring to import
//! the native runtimes.

use frame_support::weights::Weight;
use parachains_common::{AccountId, AssetHubPolkadotAuraId, Balance, Nonce};
use polkadot_primitives::Block;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
	traits::Block as BlockT,
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult,
};

pub struct Runtime;

sp_api::impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> sp_version::RuntimeVersion {
			unimplemented!()
		}

		fn execute_block(_: Block) {
			unimplemented!()
		}

		fn initialize_block(_: &<Block as BlockT>::Header) {
			unimplemented!()
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			unimplemented!()
		}

		fn metadata_at_version(_: u32) -> Option<OpaqueMetadata> {
			unimplemented!()
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			unimplemented!()
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AssetHubPolkadotAuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			unimplemented!()
		}

		fn authorities() -> Vec<AssetHubPolkadotAuraId> {
			unimplemented!()
		}
	}

	impl cumulus_primitives_aura::AuraUnincludedSegmentApi<Block> for Runtime {
		fn can_build_upon(
			_: <Block as BlockT>::Hash,
			_: cumulus_primitives_aura::Slot,
		) -> bool {
			unimplemented!()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(_: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			unimplemented!()
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			unimplemented!()
		}

		fn inherent_extrinsics(_: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			unimplemented!()
		}

		fn check_inherents(_: Block, _: sp_inherents::InherentData) -> sp_inherents::CheckInherentsResult {
			unimplemented!()
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			_: TransactionSource,
			_: <Block as BlockT>::Extrinsic,
			_: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			unimplemented!()
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(_: &<Block as BlockT>::Header) {
			unimplemented!()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(_: Option<Vec<u8>>) -> Vec<u8> {
			unimplemented!()
		}

		fn decode_session_keys(
			_: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			unimplemented!()
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			_: <Block as BlockT>::Extrinsic,
			_: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			unimplemented!()
		}
		fn query_fee_details(
			_: <Block as BlockT>::Extrinsic,
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

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info(_: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
			unimplemented!()
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(_: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			unimplemented!()
		}

		fn execute_block(
			_: Block,
			_: bool,
			_: bool,
			_: frame_try_runtime::TryStateSelect,
		) -> Weight {
			unimplemented!()
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(_: AccountId) -> Nonce {
			unimplemented!()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
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

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn create_default_config() -> Vec<u8> {
			unimplemented!()
		}

		fn build_config(_: Vec<u8>) -> sp_genesis_builder::Result {
			unimplemented!()
		}
	}
}
