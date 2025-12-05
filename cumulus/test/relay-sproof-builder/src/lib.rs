// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate alloc;

use alloc::collections::btree_map::BTreeMap;
use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain, AbridgedHostConfiguration, AbridgedHrmpChannel, ParaId,
};
use polkadot_primitives::UpgradeGoAhead;
use sp_consensus_babe::{
	digests::{CompatibleDigestItem, PreDigest, PrimaryPreDigest},
	AuthorityId, AuthorityPair, BabeAuthorityWeight,
};
use sp_core::{
	sr25519::vrf::{VrfPreOutput, VrfProof, VrfSignature},
	Pair, H256,
};
use sp_runtime::{
	traits::{HashingFor, Header},
	Digest, DigestItem,
};
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
	pub num_authorities: u64,
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
			num_authorities: 0,
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

	/// Build sproof and generate relay parent descendants with the configured authorities.
	///
	/// Returns a tuple of (state_root, storage_proof, relay_parent_descendants).
	pub fn into_state_root_proof_and_descendants(
		self,
		num_descendants: u64,
	) -> (polkadot_primitives::Hash, sp_state_machine::StorageProof, Vec<TestHeader>) {
		let authorities = generate_authority_pairs(self.num_authorities);
		let (state_root, proof) = self.into_state_root_and_proof();
		let descendants =
			build_relay_parent_descendants(num_descendants, state_root.into(), authorities);
		(state_root, proof, descendants)
	}

	pub fn into_state_root_and_proof(
		mut self,
	) -> (polkadot_primitives::Hash, sp_state_machine::StorageProof) {
		// Generate and add authorities if num_authorities is set
		if self.num_authorities > 0 {
			let authorities = generate_authority_pairs(self.num_authorities);
			let auth_pair = convert_to_authority_weight_pair(&authorities);

			// Add authorities to the sproof builder
			self.additional_key_values.push((
				relay_chain::well_known_keys::AUTHORITIES.to_vec(),
				auth_pair.clone().encode(),
			));
			self.additional_key_values.push((
				relay_chain::well_known_keys::NEXT_AUTHORITIES.to_vec(),
				auth_pair.encode(),
			));
		}

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

/// Block Header type for testing
pub type TestHeader = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;

/// Generate a vector of AuthorityPairs
pub fn generate_authority_pairs(num_authorities: u64) -> Vec<AuthorityPair> {
	(0..num_authorities).map(|i| AuthorityPair::from_seed(&[i as u8; 32])).collect()
}

/// Convert AuthorityPair to (AuthorityId, BabeAuthorityWeight)
pub fn convert_to_authority_weight_pair(
	authorities: &[AuthorityPair],
) -> Vec<(AuthorityId, BabeAuthorityWeight)> {
	authorities
		.iter()
		.map(|auth| (auth.public().into(), Default::default()))
		.collect()
}

/// Add a BABE pre-digest to a generic header
pub fn add_pre_digest<Header: sp_runtime::traits::Header>(
	header: &mut Header,
	authority_index: u32,
	block_number: u64,
) {
	/// This method generates some vrf data, but only to make the compiler happy
	fn generate_testing_vrf() -> VrfSignature {
		let vrf_proof_bytes = [0u8; 64];
		let proof: VrfProof = VrfProof::decode(&mut vrf_proof_bytes.as_slice()).unwrap();
		let vrf_pre_out_bytes = [0u8; 32];
		let pre_output: VrfPreOutput =
			VrfPreOutput::decode(&mut vrf_pre_out_bytes.as_slice()).unwrap();
		VrfSignature { pre_output, proof }
	}

	let pre_digest = PrimaryPreDigest {
		authority_index,
		slot: block_number.into(),
		vrf_signature: generate_testing_vrf(),
	};

	header
		.digest_mut()
		.push(DigestItem::babe_pre_digest(PreDigest::Primary(pre_digest)));
}

/// Create a mock chain of relay headers as descendants of the relay parent
pub fn build_relay_parent_descendants(
	num_headers: u64,
	state_root: H256,
	authorities: Vec<AuthorityPair>,
) -> Vec<TestHeader> {
	let mut headers = Vec::with_capacity(num_headers as usize);

	let mut previous_hash = None;

	for block_number in 0..=num_headers as u32 - 1 {
		let mut header = TestHeader {
			number: block_number,
			parent_hash: previous_hash.unwrap_or_default(),
			state_root,
			extrinsics_root: H256::default(),
			digest: Digest::default(),
		};
		let authority_index = block_number % (authorities.len() as u32);

		// Add pre-digest
		add_pre_digest(&mut header, authority_index, block_number as u64);

		// Sign and seal the header
		let signature = authorities[authority_index as usize].sign(header.hash().as_bytes());
		header.digest_mut().push(DigestItem::babe_seal(signature.into()));

		previous_hash = Some(header.hash());
		headers.push(header);
	}

	headers
}
