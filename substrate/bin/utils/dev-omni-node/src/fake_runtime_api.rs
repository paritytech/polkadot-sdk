//! # Fake Runtime
//!
//! A fake runtime that implements (hopefully) all of the runtime apis defined in polkadot-sdk as a
//! stub. All implementations are fulfilled with `unreachable!()`.
//!
//! See [`FakeRuntime`]

#![allow(unused_variables)]

use sp_runtime::traits::Block as BlockT;

/// The fake runtime.
pub(crate) struct FakeRuntime;

/// Some types that we need to fulfill the trait bounds at compile-time, but in runtime they don't
/// matter at all.
mod whatever {
	pub(crate) type Block = crate::standards::OpaqueBlock;
	pub(crate) type Header = <Block as sp_runtime::traits::Block>::Header;
	pub(crate) type AccountId = sp_runtime::AccountId32;
	pub(crate) type Nonce = u32;
	pub(crate) type AuraId = sp_consensus_aura::sr25519::AuthorityId;
	pub(crate) type BlockNumber = u32;
	pub(crate) type Balance = u128;
	pub(crate) type Weight = sp_weights::Weight;
	pub(crate) type RuntimeCall = ();
}
use whatever::*;

sp_api::impl_runtime_apis! {
	impl sp_api::Core<Block> for FakeRuntime {
		fn version() -> sp_version::RuntimeVersion {
			unreachable!()
		}

		fn execute_block(block: Block) {
			unreachable!()
		}

		fn initialize_block(header: &Header) -> sp_runtime::ExtrinsicInclusionMode {
			unreachable!()
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for FakeRuntime {
		fn validate_transaction(
			_source: sp_runtime::transaction_validity::TransactionSource,
			_tx: <Block as sp_runtime::traits::Block>::Extrinsic,
			_hash: <Block as sp_runtime::traits::Block>::Hash,
		) -> sp_runtime::transaction_validity::TransactionValidity {
			unreachable!()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for FakeRuntime {
		fn apply_extrinsic(
			extrinsic: <Block as sp_runtime::traits::Block>::Extrinsic
		) -> sp_runtime::ApplyExtrinsicResult {
			unreachable!()
		}

		fn finalize_block() -> <Block as sp_runtime::traits::Block>::Header {
			unreachable!()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as sp_runtime::traits::Block>::Extrinsic> {
			unreachable!()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			unreachable!()
		}
	}

	impl sp_api::Metadata<Block> for FakeRuntime {
		fn metadata() -> sp_core::OpaqueMetadata {
			unreachable!()
		}

		fn metadata_at_version(version: u32) -> Option<sp_core::OpaqueMetadata> {
			unreachable!()
		}

		fn metadata_versions() -> Vec<u32> {
			unreachable!()
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for FakeRuntime {
		fn account_nonce(account: AccountId) -> Nonce {
			unreachable!();
		}
	}

	impl sp_session::SessionKeys<Block> for FakeRuntime {
		fn generate_session_keys(_seed: Option<Vec<u8>>) -> Vec<u8> {
			unreachable!()
		}

		fn decode_session_keys(
			_encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, sp_session::KeyTypeId)>> {
			unreachable!()
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for FakeRuntime {
		fn offchain_worker(header: &<Block as sp_runtime::traits::Block>::Header) {
			unreachable!()
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for FakeRuntime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			unreachable!();
		}

		fn authorities() -> Vec<AuraId> {
			unreachable!();
		}
	}

	impl sp_consensus_grandpa::GrandpaApi<Block> for FakeRuntime {
		fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
			unreachable!()
		}

		fn current_set_id() -> sp_consensus_grandpa::SetId {
			unreachable!()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_grandpa::EquivocationProof<
				<Block as BlockT>::Hash,
				BlockNumber,
			>,
			_key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			unreachable!()
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_grandpa::SetId,
			_authority_id: sp_consensus_grandpa::AuthorityId, // TODO: double check, but it should not even matter.
		) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
			unreachable!()
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for FakeRuntime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			unreachable!()
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			unreachable!()
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			unreachable!()
		}
		fn query_length_to_fee(length: u32) -> Balance {
			unreachable!()
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
		for FakeRuntime
	{
		fn query_call_info(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			unreachable!()
		}
		fn query_call_fee_details(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			unreachable!()
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			unreachable!()
		}
		fn query_length_to_fee(length: u32) -> Balance {
			unreachable!()
		}
	}
}
