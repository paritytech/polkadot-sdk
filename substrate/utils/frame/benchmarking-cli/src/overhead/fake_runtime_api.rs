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

//! A fake runtime struct that allows us to instantiate a client.
//! Has all the required runtime APIs implemented to satisfy trait bounds,
//! but the methods are never called since we use WASM exclusively.

use sp_core::OpaqueMetadata;
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, Block as BlockT},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, OpaqueExtrinsic,
};

/// Block number
type BlockNumber = u32;
/// Opaque block header type.
type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Opaque block type.
type Block = generic::Block<Header, OpaqueExtrinsic>;

#[allow(unused)]
pub struct Runtime;

sp_api::impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> sp_version::RuntimeVersion {
			unimplemented!()
		}

		fn execute_block(_: Block) {
			unimplemented!()
		}

		fn initialize_block(_: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
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

		fn metadata_versions() -> Vec<u32> {
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

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
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
