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

use crate::Client;
use codec::Encode;
use cumulus_primitives_core::{ParachainBlockData, PersistedValidationData};
use cumulus_primitives_parachain_inherent::{ParachainInherentData, INHERENT_IDENTIFIER};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use cumulus_test_runtime::{Block, GetLastTimestamp, Hash, Header};
use polkadot_primitives::{BlockNumber as PBlockNumber, Hash as PHash};
use sc_block_builder::BlockBuilderBuilder;
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::Slot;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	Digest, DigestItem,
};

/// A struct containing a block builder and support data required to build test scenarios.
pub struct BlockBuilderAndSupportData<'a> {
	pub block_builder: sc_block_builder::BlockBuilder<'a, Block, Client>,
	pub persisted_validation_data: PersistedValidationData<PHash, PBlockNumber>,
	pub slot: Slot,
}

/// An extension for the Cumulus test client to init a block builder.
pub trait InitBlockBuilder {
	/// Init a specific block builder that works for the test runtime.
	///
	/// This will automatically create and push the inherents for you to make the block
	/// valid for the test runtime.
	///
	/// You can use the relay chain state sproof builder to arrange required relay chain state or
	/// just use a default one. The relay chain slot in the storage proof
	/// will be adjusted to align with the parachain slot to pass validation.
	///
	/// Returns the block builder and validation data for further usage.
	fn init_block_builder(
		&self,
		validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> BlockBuilderAndSupportData;

	/// Init a specific block builder at a specific block that works for the test runtime.
	///
	/// Same as [`InitBlockBuilder::init_block_builder`] besides that it takes a
	/// [`type@Hash`] to say which should be the parent block of the block that is being build.
	fn init_block_builder_at(
		&self,
		at: Hash,
		validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> BlockBuilderAndSupportData;

	/// Init a specific block builder that works for the test runtime.
	///
	/// Same as [`InitBlockBuilder::init_block_builder`] besides that it takes a
	/// [`type@Hash`] to say which should be the parent block of the block that is being build and
	/// it will use the given `timestamp` as input for the timestamp inherent.
	fn init_block_builder_with_timestamp(
		&self,
		at: Hash,
		validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
		timestamp: u64,
	) -> BlockBuilderAndSupportData;
}

fn init_block_builder(
	client: &Client,
	at: Hash,
	validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
	mut relay_sproof_builder: RelayStateSproofBuilder,
	timestamp: u64,
) -> BlockBuilderAndSupportData<'_> {
	// This slot will be used for both relay chain and parachain
	let slot: Slot = (timestamp / cumulus_test_runtime::SLOT_DURATION).into();
	relay_sproof_builder.current_slot = slot;

	let aura_pre_digest = Digest {
		logs: vec![DigestItem::PreRuntime(sp_consensus_aura::AURA_ENGINE_ID, slot.encode())],
	};

	let mut block_builder = BlockBuilderBuilder::new(client)
		.on_parent_block(at)
		.fetch_parent_block_number(client)
		.unwrap()
		.enable_proof_recording()
		.with_inherent_digests(aura_pre_digest)
		.build()
		.expect("Creates new block builder for test runtime");

	let mut inherent_data = sp_inherents::InherentData::new();

	inherent_data
		.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp)
		.expect("Put timestamp failed");

	let (relay_parent_storage_root, relay_chain_state) =
		relay_sproof_builder.into_state_root_and_proof();

	let mut validation_data = validation_data.unwrap_or_default();
	validation_data.relay_parent_storage_root = relay_parent_storage_root;

	inherent_data
		.put_data(
			INHERENT_IDENTIFIER,
			&ParachainInherentData {
				validation_data: validation_data.clone(),
				relay_chain_state,
				downward_messages: Default::default(),
				horizontal_messages: Default::default(),
			},
		)
		.expect("Put validation function params failed");

	let inherents = block_builder.create_inherents(inherent_data).expect("Creates inherents");

	inherents
		.into_iter()
		.for_each(|ext| block_builder.push(ext).expect("Pushes inherent"));

	BlockBuilderAndSupportData { block_builder, persisted_validation_data: validation_data, slot }
}

impl InitBlockBuilder for Client {
	fn init_block_builder(
		&self,
		validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> BlockBuilderAndSupportData {
		let chain_info = self.chain_info();
		self.init_block_builder_at(chain_info.best_hash, validation_data, relay_sproof_builder)
	}

	fn init_block_builder_at(
		&self,
		at: Hash,
		validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
	) -> BlockBuilderAndSupportData {
		let last_timestamp = self.runtime_api().get_last_timestamp(at).expect("Get last timestamp");

		let timestamp = if last_timestamp == 0 {
			std::time::SystemTime::now()
				.duration_since(std::time::SystemTime::UNIX_EPOCH)
				.expect("Time is always after UNIX_EPOCH; qed")
				.as_millis() as u64
		} else {
			last_timestamp + cumulus_test_runtime::SLOT_DURATION
		};

		init_block_builder(self, at, validation_data, relay_sproof_builder, timestamp)
	}

	fn init_block_builder_with_timestamp(
		&self,
		at: Hash,
		validation_data: Option<PersistedValidationData<PHash, PBlockNumber>>,
		relay_sproof_builder: RelayStateSproofBuilder,
		timestamp: u64,
	) -> BlockBuilderAndSupportData {
		init_block_builder(self, at, validation_data, relay_sproof_builder, timestamp)
	}
}

/// Extension trait for the [`BlockBuilder`](sc_block_builder::BlockBuilder) to build directly a
/// [`ParachainBlockData`].
pub trait BuildParachainBlockData {
	/// Directly build the [`ParachainBlockData`] from the block that comes out of the block
	/// builder.
	fn build_parachain_block(self, parent_state_root: Hash) -> ParachainBlockData<Block>;
}

impl<'a> BuildParachainBlockData for sc_block_builder::BlockBuilder<'a, Block, Client> {
	fn build_parachain_block(self, parent_state_root: Hash) -> ParachainBlockData<Block> {
		let built_block = self.build().expect("Builds the block");

		let storage_proof = built_block
			.proof
			.expect("We enabled proof recording before.")
			.into_compact_proof::<<Header as HeaderT>::Hashing>(parent_state_root)
			.expect("Creates the compact proof");

		let (header, extrinsics) = built_block.block.deconstruct();
		ParachainBlockData::new(header, extrinsics, storage_proof)
	}
}
