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

use cumulus_primitives_core::{
	relay_chain, AbridgedHostConfiguration, AbridgedHrmpChannel, ParaId,
};
use polkadot_primitives::UpgradeGoAhead;
use sp_runtime::traits::HashingFor;
use sp_std::collections::btree_map::BTreeMap;
use sp_trie::PrefixedMemoryDB;

/// Builds a sproof (portmanteau of 'spoof' and 'proof') of the relay chain state.
#[derive(Clone)]
pub struct RelayStateSproofBuilder {
	/// The para id of the current parachain.
	///
	/// This doesn't get into the storage proof produced by the builder, however, it is used for
	/// generation of the storage image and by auxiliary methods.
	///
	/// It's recommended to change this value once in the very beginning of usage.
	///
	/// The default value is 200.
	pub para_id: ParaId,

	pub host_config: AbridgedHostConfiguration,
	pub dmq_mqc_head: Option<relay_chain::Hash>,
	pub upgrade_go_ahead: Option<UpgradeGoAhead>,
	pub relay_dispatch_queue_remaining_capacity: Option<(u32, u32)>,
	pub hrmp_ingress_channel_index: Option<Vec<ParaId>>,
	pub hrmp_egress_channel_index: Option<Vec<ParaId>>,
	pub hrmp_channels: BTreeMap<relay_chain::HrmpChannelId, AbridgedHrmpChannel>,
	pub current_slot: relay_chain::Slot,
	pub current_epoch: u64,
	pub randomness: relay_chain::Hash,
	pub additional_key_values: Vec<(Vec<u8>, Vec<u8>)>,
	pub included_para_head: Option<relay_chain::HeadData>,
}

impl Default for RelayStateSproofBuilder {
	fn default() -> Self {
		RelayStateSproofBuilder {
			para_id: ParaId::from(200),
			host_config: cumulus_primitives_core::AbridgedHostConfiguration {
				max_code_size: 2 * 1024 * 1024,
				max_head_data_size: 1024 * 1024,
				max_upward_queue_count: 8,
				max_upward_queue_size: 1024,
				max_upward_message_size: 256,
				max_upward_message_num_per_candidate: 5,
				hrmp_max_message_num_per_candidate: 5,
				validation_upgrade_cooldown: 6,
				validation_upgrade_delay: 6,
				async_backing_params: relay_chain::AsyncBackingParams {
					allowed_ancestry_len: 0,
					max_candidate_depth: 0,
				},
			},
			dmq_mqc_head: None,
			upgrade_go_ahead: None,
			relay_dispatch_queue_remaining_capacity: None,
			hrmp_ingress_channel_index: None,
			hrmp_egress_channel_index: None,
			hrmp_channels: BTreeMap::new(),
			current_slot: 0.into(),
			current_epoch: 0u64,
			randomness: relay_chain::Hash::default(),
			additional_key_values: vec![],
			included_para_head: None,
		}
	}
}

impl RelayStateSproofBuilder {
	/// Returns a mutable reference to HRMP channel metadata for a channel (`sender`,
	/// `self.para_id`).
	///
	/// If there is no channel, a new default one is created.
	///
	/// It also updates the `hrmp_ingress_channel_index`, creating it if needed.
	pub fn upsert_inbound_channel(&mut self, sender: ParaId) -> &mut AbridgedHrmpChannel {
		let in_index = self.hrmp_ingress_channel_index.get_or_insert_with(Vec::new);
		if let Err(idx) = in_index.binary_search(&sender) {
			in_index.insert(idx, sender);
		}

		self.upsert_channel(relay_chain::HrmpChannelId { sender, recipient: self.para_id })
	}

	/// Returns a mutable reference to HRMP channel metadata for a channel (`self.para_id`,
	/// `recipient`).
	///
	/// If there is no channel, a new default one is created.
	///
	/// It also updates the `hrmp_egress_channel_index`, creating it if needed.
	pub fn upsert_outbound_channel(&mut self, recipient: ParaId) -> &mut AbridgedHrmpChannel {
		let in_index = self.hrmp_egress_channel_index.get_or_insert_with(Vec::new);
		if let Err(idx) = in_index.binary_search(&recipient) {
			in_index.insert(idx, recipient);
		}

		self.upsert_channel(relay_chain::HrmpChannelId { sender: self.para_id, recipient })
	}

	/// Creates a new default entry in the hrmp channels mapping if not exists, and returns mutable
	/// reference to it.
	fn upsert_channel(&mut self, id: relay_chain::HrmpChannelId) -> &mut AbridgedHrmpChannel {
		self.hrmp_channels.entry(id).or_insert_with(|| AbridgedHrmpChannel {
			max_capacity: 0,
			max_total_size: 0,
			max_message_size: 0,
			msg_count: 0,
			total_size: 0,
			mqc_head: None,
		})
	}

	pub fn into_state_root_and_proof(
		self,
	) -> (polkadot_primitives::Hash, sp_state_machine::StorageProof) {
		let (db, root) =
			PrefixedMemoryDB::<HashingFor<polkadot_primitives::Block>>::default_with_root();
		let state_version = Default::default(); // for test using default.
		let mut backend = sp_state_machine::TrieBackendBuilder::new(db, root).build();

		let mut relevant_keys = Vec::new();
		{
			use codec::Encode as _;

			let mut insert = |key: Vec<u8>, value: Vec<u8>| {
				relevant_keys.push(key.clone());
				backend.insert(vec![(None, vec![(key, Some(value))])], state_version);
			};

			insert(relay_chain::well_known_keys::ACTIVE_CONFIG.to_vec(), self.host_config.encode());
			if let Some(dmq_mqc_head) = self.dmq_mqc_head {
				insert(
					relay_chain::well_known_keys::dmq_mqc_head(self.para_id),
					dmq_mqc_head.encode(),
				);
			}
			if let Some(para_head) = self.included_para_head {
				insert(relay_chain::well_known_keys::para_head(self.para_id), para_head.encode());
			}
			if let Some(relay_dispatch_queue_remaining_capacity) =
				self.relay_dispatch_queue_remaining_capacity
			{
				insert(
					relay_chain::well_known_keys::relay_dispatch_queue_remaining_capacity(
						self.para_id,
					)
					.key,
					relay_dispatch_queue_remaining_capacity.encode(),
				);
			}
			if let Some(upgrade_go_ahead) = self.upgrade_go_ahead {
				insert(
					relay_chain::well_known_keys::upgrade_go_ahead_signal(self.para_id),
					upgrade_go_ahead.encode(),
				);
			}
			if let Some(hrmp_ingress_channel_index) = self.hrmp_ingress_channel_index {
				let mut sorted = hrmp_ingress_channel_index.clone();
				sorted.sort();
				assert_eq!(sorted, hrmp_ingress_channel_index);

				insert(
					relay_chain::well_known_keys::hrmp_ingress_channel_index(self.para_id),
					hrmp_ingress_channel_index.encode(),
				);
			}
			if let Some(hrmp_egress_channel_index) = self.hrmp_egress_channel_index {
				let mut sorted = hrmp_egress_channel_index.clone();
				sorted.sort();
				assert_eq!(sorted, hrmp_egress_channel_index);

				insert(
					relay_chain::well_known_keys::hrmp_egress_channel_index(self.para_id),
					hrmp_egress_channel_index.encode(),
				);
			}
			for (channel, metadata) in self.hrmp_channels {
				insert(relay_chain::well_known_keys::hrmp_channels(channel), metadata.encode());
			}
			insert(relay_chain::well_known_keys::EPOCH_INDEX.to_vec(), self.current_epoch.encode());
			insert(
				relay_chain::well_known_keys::ONE_EPOCH_AGO_RANDOMNESS.to_vec(),
				self.randomness.encode(),
			);
			insert(relay_chain::well_known_keys::CURRENT_SLOT.to_vec(), self.current_slot.encode());

			for (key, value) in self.additional_key_values {
				insert(key, value);
			}
		}

		let root = *backend.root();
		let proof = sp_state_machine::prove_read(backend, relevant_keys).expect("prove read");
		(root, proof)
	}
}
