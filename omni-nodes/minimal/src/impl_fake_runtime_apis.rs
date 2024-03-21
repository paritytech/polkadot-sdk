#![allow(unused_variables)]

pub struct FakeRuntime;
use crate::types::{AccountId, Header, Nonce, OpaqueBlock as Block};

sp_api::impl_runtime_apis! {
	// same block of code as in `minimal/runtime/src/lib.rs`, but replace all functions to be
	// `todo!()`. This is only ti provide a declaration of apis to the client, code and should
	// someday be removed. Since we only use the wasm executor, this is fine for now. In fact, it
	// need not be "complete", it should only require the impls that this node crate needs 🤡 also,
	// in general, see https://github.com/paritytech/polkadot-sdk/issues/27. In some sense, this is
	// the node software fooling itself.
	impl sp_api::Core<Block> for FakeRuntime {
		fn version() -> sp_version::RuntimeVersion {
			todo!()
		}

		fn execute_block(block: Block) {
			todo!()
		}

		fn initialize_block(header: &Header) -> sp_runtime::ExtrinsicInclusionMode {
			todo!()
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for FakeRuntime {
		fn validate_transaction(
			_source: sp_runtime::transaction_validity::TransactionSource,
			_tx: <Block as sp_runtime::traits::Block>::Extrinsic,
			_hash: <Block as sp_runtime::traits::Block>::Hash,
		) -> sp_runtime::transaction_validity::TransactionValidity {
			todo!()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for FakeRuntime {
		fn apply_extrinsic(
			extrinsic: <Block as sp_runtime::traits::Block>::Extrinsic
		) -> sp_runtime::ApplyExtrinsicResult {
			todo!()
		}

		fn finalize_block() -> <Block as sp_runtime::traits::Block>::Header {
			todo!()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as sp_runtime::traits::Block>::Extrinsic> {
			todo!()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			todo!()
		}
	}

	impl sp_api::Metadata<Block> for FakeRuntime {
		fn metadata() -> sp_core::OpaqueMetadata {
			todo!()
		}

		fn metadata_at_version(version: u32) -> Option<sp_core::OpaqueMetadata> {
			todo!()
		}

		fn metadata_versions() -> Vec<u32> {
			todo!()
		}
	}

	impl substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce> for FakeRuntime {
		fn account_nonce(account: AccountId) -> Nonce {
			todo!();
		}
	}

	impl sp_session::SessionKeys<Block> for FakeRuntime {
		fn generate_session_keys(_seed: Option<Vec<u8>>) -> Vec<u8> {
			Default::default()
		}

		fn decode_session_keys(
			_encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, sp_session::KeyTypeId)>> {
			Default::default()
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for FakeRuntime {
		fn offchain_worker(header: &<Block as sp_runtime::traits::Block>::Header) {
			todo!();
		}
	}

}
