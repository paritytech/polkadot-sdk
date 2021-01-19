// Copyright 2020 Parity Technologies (UK) Ltd.
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

use crate::{Backend, Client};
use cumulus_primitives::{
	inherents::{ValidationDataType, VALIDATION_DATA_IDENTIFIER}, PersistedValidationData,
};
use cumulus_test_runtime::{Block, GetLastTimestamp};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use polkadot_primitives::v1::BlockNumber as PBlockNumber;
use sc_block_builder::{BlockBuilder, BlockBuilderProvider};
use sp_api::ProvideRuntimeApi;
use sp_runtime::generic::BlockId;

/// An extension for the Cumulus test client to init a block builder.
pub trait InitBlockBuilder {
	/// Init a specific block builder that works for the test runtime.
	///
	/// This will automatically create and push the inherents for you to make the block
	/// valid for the test runtime.
	///
	/// You can use the relay chain state sproof builder to arrange required relay chain state or
	/// just use a default one.
	fn init_block_builder(
		&self,
		validation_data: Option<PersistedValidationData<PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> sc_block_builder::BlockBuilder<Block, Client, Backend>;

	/// Init a specific block builder at a specific block that works for the test runtime.
	///
	/// Same as [`InitBlockBuilder::init_block_builder`] besides that it takes a
	/// [`BlockId`] to say which should be the parent block of the block that is being build.
	fn init_block_builder_at(
		&self,
		at: &BlockId<Block>,
		validation_data: Option<PersistedValidationData<PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> sc_block_builder::BlockBuilder<Block, Client, Backend>;
}

impl InitBlockBuilder for Client {
	fn init_block_builder(
		&self,
		validation_data: Option<PersistedValidationData<PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> BlockBuilder<Block, Client, Backend> {
		let chain_info = self.chain_info();
		self.init_block_builder_at(
			&BlockId::Hash(chain_info.best_hash),
			validation_data,
			relay_sproof_builder,
		)
	}

	fn init_block_builder_at(
		&self,
		at: &BlockId<Block>,
		validation_data: Option<PersistedValidationData<PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> BlockBuilder<Block, Client, Backend> {
		let mut block_builder = self
			.new_block_at(at, Default::default(), true)
			.expect("Creates new block builder for test runtime");

		let mut inherent_data = sp_inherents::InherentData::new();
		let last_timestamp = self
			.runtime_api()
			.get_last_timestamp(&at)
			.expect("Get last timestamp");

		let timestamp = last_timestamp + cumulus_test_runtime::MinimumPeriod::get();

		inherent_data
			.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp)
			.expect("Put timestamp failed");

		let (relay_storage_root, relay_chain_state) =
			relay_sproof_builder.into_state_root_and_proof();

		let mut validation_data = validation_data.unwrap_or_default();
		assert_eq!(
			validation_data.relay_storage_root,
			Default::default(),
			"Overriding the relay storage root is not implemented",
		);
		validation_data.relay_storage_root = relay_storage_root;

		inherent_data
			.put_data(
				VALIDATION_DATA_IDENTIFIER,
				&ValidationDataType {
					validation_data,
					relay_chain_state,
				},
			)
			.expect("Put validation function params failed");

		let inherents = block_builder
			.create_inherents(inherent_data)
			.expect("Creates inherents");

		inherents
			.into_iter()
			.for_each(|ext| block_builder.push(ext).expect("Pushes inherent"));

		block_builder
	}
}
