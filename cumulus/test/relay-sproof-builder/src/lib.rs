// Copyright 2021 Parity Technologies (UK) Ltd.
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
use polkadot_primitives::v1::UpgradeGoAhead;
use sp_runtime::traits::HashFor;
use sp_state_machine::MemoryDB;
use sp_std::collections::btree_map::BTreeMap;

/// Builds a sproof (portmanteau of 'spoof' and 'proof') of the relay chain state.
#[derive(Clone)]
pub struct RelayStateSproofBuilder {
	/// The para id of the current parachain.
	///
	/// This doesn't get into the storage proof produced by the builder, however, it is used for
	/// generation of the storage image and by auxilary methods.
	///
	/// It's recommended to change this value once in the very beginning of usage.
	///
	/// The default value is 200.
	pub para_id: ParaId,

	pub host_config: AbridgedHostConfiguration,
	pub dmq_mqc_head: Option<relay_chain::Hash>,
	pub upgrade_go_ahead: Option<UpgradeGoAhead>,
	pub relay_dispatch_queue_size: Option<(u32, u32)>,
	pub hrmp_ingress_channel_index: Option<Vec<ParaId>>,
	pub hrmp_egress_channel_index: Option<Vec<ParaId>>,
	pub hrmp_channels: BTreeMap<relay_chain::v1::HrmpChannelId, AbridgedHrmpChannel>,
	pub current_slot: relay_chain::v1::Slot,
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
			},
			dmq_mqc_head: None,
			upgrade_go_ahead: None,
			relay_dispatch_queue_size: None,
			hrmp_ingress_channel_index: None,
			hrmp_egress_channel_index: None,
			hrmp_channels: BTreeMap::new(),
			current_slot: 0.into(),
		}
	}
}

impl RelayStateSproofBuilder {
	/// Returns a mutable reference to HRMP channel metadata for a channel (`sender`, `self.para_id`).
	///
	/// If there is no channel, a new default one is created.
	///
	/// It also updates the `hrmp_ingress_channel_index`, creating it if needed.
	pub fn upsert_inbound_channel(&mut self, sender: ParaId) -> &mut AbridgedHrmpChannel {
		let in_index = self.hrmp_ingress_channel_index.get_or_insert_with(Vec::new);
		if let Err(idx) = in_index.binary_search(&sender) {
			in_index.insert(idx, sender);
		}

		self.hrmp_channels
			.entry(relay_chain::v1::HrmpChannelId { sender, recipient: self.para_id })
			.or_insert_with(|| AbridgedHrmpChannel {
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
	) -> (polkadot_primitives::v1::Hash, sp_state_machine::StorageProof) {
		let (db, root) = MemoryDB::<HashFor<polkadot_primitives::v1::Block>>::default_with_root();
		let state_version = Default::default(); // for test using default.
		let mut backend = sp_state_machine::TrieBackend::new(db, root);

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
			if let Some(relay_dispatch_queue_size) = self.relay_dispatch_queue_size {
				insert(
					relay_chain::well_known_keys::relay_dispatch_queue_size(self.para_id),
					relay_dispatch_queue_size.encode(),
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

			insert(relay_chain::well_known_keys::CURRENT_SLOT.to_vec(), self.current_slot.encode());
		}

		let root = backend.root().clone();
		let proof = sp_state_machine::prove_read(backend, relevant_keys).expect("prove read");
		(root, proof)
	}
}
