// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::{ParachainInherentData, INHERENT_IDENTIFIER};
use codec::Decode;
use cumulus_primitives_core::{
	relay_chain, InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
};
use sc_client_api::{Backend, StorageProvider};
use sp_api::BlockId;
use sp_core::twox_128;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_runtime::traits::Block;
use std::collections::BTreeMap;

use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

/// Inherent data provider that supplies mocked validation data.
///
/// This is useful when running a node that is not actually backed by any relay chain.
/// For example when running a local node, or running integration tests.
///
/// We mock a relay chain block number as follows:
/// relay_block_number = offset + relay_blocks_per_para_block * current_para_block
/// To simulate a parachain that starts in relay block 1000 and gets a block in every other relay
/// block, use 1000 and 2
///
/// Optionally, mock XCM messages can be injected into the runtime. When mocking XCM,
/// in addition to the messages themselves, you must provide some information about
/// your parachain's configuration in order to mock the MQC heads properly.
/// See [`MockXcmConfig`] for more information
pub struct MockValidationDataInherentDataProvider {
	/// The current block number of the local block chain (the parachain)
	pub current_para_block: u32,
	/// The relay block in which this parachain appeared to start. This will be the relay block
	/// number in para block #P1
	pub relay_offset: u32,
	/// The number of relay blocks that elapses between each parablock. Probably set this to 1 or 2
	/// to simulate optimistic or realistic relay chain behavior.
	pub relay_blocks_per_para_block: u32,
	/// XCM messages and associated configuration information.
	pub xcm_config: MockXcmConfig,
	/// Inbound downward XCM messages to be injected into the block.
	pub raw_downward_messages: Vec<Vec<u8>>,
	// Inbound Horizontal messages sorted by channel
	pub raw_horizontal_messages: Vec<(ParaId, Vec<u8>)>,
}

/// Parameters for how the Mock inherent data provider should inject XCM messages.
/// In addition to the messages themselves, some information about the parachain's
/// configuration is also required so that the MQC heads can be read out of the
/// parachain's storage, and the corresponding relay data mocked.
#[derive(Default)]
pub struct MockXcmConfig {
	/// The parachain id of the parachain being mocked.
	pub para_id: ParaId,
	/// The starting state of the dmq_mqc_head.
	pub starting_dmq_mqc_head: relay_chain::Hash,
	/// The starting state of each parachain's mqc head
	pub starting_hrmp_mqc_heads: BTreeMap<ParaId, relay_chain::Hash>,
}

/// The name of the parachain system in the runtime.
///
/// This name is used by frame to prefix storage items and will be required to read data from the storage.
///
/// The `Default` implementation sets the name to `ParachainSystem`.
pub struct ParachainSystemName(pub Vec<u8>);

impl Default for ParachainSystemName {
	fn default() -> Self {
		Self(b"ParachainSystem".to_vec())
	}
}

impl MockXcmConfig {
	/// Create a MockXcmConfig by reading the mqc_heads directly
	/// from the storage of a previous block.
	pub fn new<B: Block, BE: Backend<B>, C: StorageProvider<B, BE>>(
		client: &C,
		parent_block: B::Hash,
		para_id: ParaId,
		parachain_system_name: ParachainSystemName,
	) -> Self {
		let starting_dmq_mqc_head = client
			.storage(
				&BlockId::Hash(parent_block),
				&sp_storage::StorageKey(
					[twox_128(&parachain_system_name.0), twox_128(b"LastDmqMqcHead")]
						.concat()
						.to_vec(),
				),
			)
			.expect("We should be able to read storage from the parent block.")
			.map(|ref mut raw_data| {
				Decode::decode(&mut &raw_data.0[..]).expect("Stored data should decode correctly")
			})
			.unwrap_or_default();

		let starting_hrmp_mqc_heads = client
			.storage(
				&BlockId::Hash(parent_block),
				&sp_storage::StorageKey(
					[twox_128(&parachain_system_name.0), twox_128(b"LastHrmpMqcHeads")]
						.concat()
						.to_vec(),
				),
			)
			.expect("We should be able to read storage from the parent block.")
			.map(|ref mut raw_data| {
				Decode::decode(&mut &raw_data.0[..]).expect("Stored data should decode correctly")
			})
			.unwrap_or_default();

		Self { para_id, starting_dmq_mqc_head, starting_hrmp_mqc_heads }
	}
}

#[async_trait::async_trait]
impl InherentDataProvider for MockValidationDataInherentDataProvider {
	fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		// Calculate the mocked relay block based on the current para block
		let relay_parent_number =
			self.relay_offset + self.relay_blocks_per_para_block * self.current_para_block;

		// Use the "sproof" (spoof proof) builder to build valid mock state root and proof.
		let mut sproof_builder = RelayStateSproofBuilder::default();
		sproof_builder.para_id = self.xcm_config.para_id;

		// Process the downward messages and set up the correct head
		let mut downward_messages = Vec::new();
		let mut dmq_mqc = crate::MessageQueueChain(self.xcm_config.starting_dmq_mqc_head);
		for msg in &self.raw_downward_messages {
			let wrapped = InboundDownwardMessage { sent_at: relay_parent_number, msg: msg.clone() };

			dmq_mqc.extend_downward(&wrapped);
			downward_messages.push(wrapped);
		}
		sproof_builder.dmq_mqc_head = Some(dmq_mqc.head());

		// Process the hrmp messages and set up the correct heads
		// Begin by collecting them into a Map
		let mut horizontal_messages = BTreeMap::<ParaId, Vec<InboundHrmpMessage>>::new();
		for (para_id, msg) in &self.raw_horizontal_messages {
			let wrapped = InboundHrmpMessage { sent_at: relay_parent_number, data: msg.clone() };

			horizontal_messages.entry(*para_id).or_default().push(wrapped);
		}

		// Now iterate again, updating the heads as we go
		for (para_id, messages) in &horizontal_messages {
			let mut channel_mqc = crate::MessageQueueChain(
				*self
					.xcm_config
					.starting_hrmp_mqc_heads
					.get(para_id)
					.unwrap_or(&relay_chain::Hash::default()),
			);
			for message in messages {
				channel_mqc.extend_hrmp(message);
			}
			sproof_builder.upsert_inbound_channel(*para_id).mqc_head = Some(channel_mqc.head());
		}

		let (relay_parent_storage_root, proof) = sproof_builder.into_state_root_and_proof();

		inherent_data.put_data(
			INHERENT_IDENTIFIER,
			&ParachainInherentData {
				validation_data: PersistedValidationData {
					parent_head: Default::default(),
					relay_parent_storage_root,
					relay_parent_number,
					max_pov_size: Default::default(),
				},
				downward_messages,
				horizontal_messages,
				relay_chain_state: proof,
			},
		)
	}

	// Copied from the real implementation
	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}
