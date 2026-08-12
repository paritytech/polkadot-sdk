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

//! Relay chain state proof provides means for accessing part of relay chain storage for reads.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain, AbridgedHostConfiguration, AbridgedHrmpChannel, ParaId,
	RelayHostConfigurationPrefix,
};
use scale_info::TypeInfo;
use sp_runtime::traits::HashingFor;
use sp_state_machine::{Backend, TrieBackend, TrieBackendBuilder};
use sp_trie::{HashDBT, MemoryDB, StorageProof, EMPTY_PREFIX};

/// The capacity of the upward message queue of a parachain on the relay chain.
// The field order should stay the same as the data can be found in the proof to ensure both are
// have the same encoded representation.
#[derive(Clone, Encode, Decode, TypeInfo, Default)]
pub struct RelayDispatchQueueRemainingCapacity {
	/// The number of additional messages that can be enqueued.
	pub remaining_count: u32,
	/// The total size of additional messages that can be enqueued.
	pub remaining_size: u32,
}

/// A snapshot of some messaging related state of relay chain pertaining to the current parachain.
///
/// This data is essential for making sure that the parachain is aware of current resource use on
/// the relay chain and that the candidates produced for this parachain do not exceed any of these
/// limits.
#[derive(Clone, Encode, Decode, TypeInfo)]
pub struct MessagingStateSnapshot {
	/// The current message queue chain head for downward message queue.
	///
	/// If the value is absent on the relay chain this will be set to all zeros.
	pub dmq_mqc_head: relay_chain::Hash,

	/// The current capacity of the upward message queue of the current parachain on the relay
	/// chain.
	pub relay_dispatch_queue_remaining_capacity: RelayDispatchQueueRemainingCapacity,

	/// Information about all the inbound HRMP channels.
	///
	/// These are structured as a list of tuples. The para id in the tuple specifies the sender
	/// of the channel. Obviously, the recipient is the current parachain.
	///
	/// The channels are sorted by the sender para id ascension.
	pub ingress_channels: Vec<(ParaId, AbridgedHrmpChannel)>,

	/// Information about all the outbound HRMP channels.
	///
	/// These are structured as a list of tuples. The para id in the tuple specifies the recipient
	/// of the channel. Obviously, the sender is the current parachain.
	///
	/// The channels are sorted by the recipient para id ascension.
	pub egress_channels: Vec<(ParaId, AbridgedHrmpChannel)>,
}

#[derive(Debug)]
pub enum Error {
	/// The provided proof was created against unexpected storage root.
	RootMismatch,
	/// The entry cannot be read.
	ReadEntry(ReadEntryErr),
	/// The optional entry cannot be read.
	ReadOptionalEntry(ReadEntryErr),
	/// The slot cannot be extracted.
	Slot(ReadEntryErr),
	/// The upgrade go-ahead signal cannot be read.
	UpgradeGoAhead(ReadEntryErr),
	/// The upgrade restriction signal cannot be read.
	UpgradeRestriction(ReadEntryErr),
	/// The host configuration cannot be extracted.
	Config(ReadEntryErr),
	/// The DMQ MQC head cannot be extracted.
	DmqMqcHead(ReadEntryErr),
	/// Relay dispatch queue cannot be extracted.
	RelayDispatchQueueRemainingCapacity(ReadEntryErr),
	/// The hrmp ingress channel index cannot be extracted.
	HrmpIngressChannelIndex(ReadEntryErr),
	/// The hrmp egress channel index cannot be extracted.
	HrmpEgressChannelIndex(ReadEntryErr),
	/// The channel identified by the sender and receiver cannot be extracted.
	HrmpChannel(ParaId, ParaId, ReadEntryErr),
	/// The latest included parachain head cannot be extracted.
	ParaHead(ReadEntryErr),
	/// The relay chain authorities cannot be extracted
	Authorities(ReadEntryErr),
	/// The relay chain authorities for the next epoch cannot be extracted
	NextAuthorities(ReadEntryErr),
}

#[derive(Debug)]
pub enum ReadEntryErr {
	/// The value cannot be extracted from the proof.
	Proof,
	/// The value cannot be decoded.
	Decode,
	/// The value is expected to be present on the relay chain, but it doesn't exist.
	Absent,
}

/// Read an entry given by the key and try to decode it. If the value specified by the key according
/// to the proof is empty, the `fallback` value will be returned.
///
/// Returns `Err` in case the backend can't return the value under the specific key (likely due to
/// a malformed proof), in case the decoding fails, or in case where the value is empty in the relay
/// chain state and no fallback was provided.
fn read_entry<T, B>(backend: &B, key: &[u8], fallback: Option<T>) -> Result<T, ReadEntryErr>
where
	T: Decode,
	B: Backend<HashingFor<relay_chain::Block>>,
{
	backend
		.storage(key)
		.map_err(|_| ReadEntryErr::Proof)?
		.map(|raw_entry| T::decode(&mut &raw_entry[..]).map_err(|_| ReadEntryErr::Decode))
		.transpose()?
		.or(fallback)
		.ok_or(ReadEntryErr::Absent)
}

/// Read an optional entry given by the key and try to decode it.
/// Returns `None` if the value specified by the key according to the proof is empty.
///
/// Returns `Err` in case the backend can't return the value under the specific key (likely due to
/// a malformed proof) or if the value couldn't be decoded.
fn read_optional_entry<T, B>(backend: &B, key: &[u8]) -> Result<Option<T>, ReadEntryErr>
where
	T: Decode,
	B: Backend<HashingFor<relay_chain::Block>>,
{
	match read_entry(backend, key, None) {
		Ok(v) => Ok(Some(v)),
		Err(ReadEntryErr::Absent) => Ok(None),
		Err(err) => Err(err),
	}
}

/// A state proof extracted from the relay chain.
///
/// This state proof is extracted from the relay chain block we are building on top of.
pub struct RelayChainStateProof {
	para_id: ParaId,
	trie_backend:
		TrieBackend<MemoryDB<HashingFor<relay_chain::Block>>, HashingFor<relay_chain::Block>>,
}

impl RelayChainStateProof {
	/// Create a new instance of `Self`.
	///
	/// Returns an error if the given `relay_parent_storage_root` is not the root of the given
	/// `proof`.
	pub fn new(
		para_id: ParaId,
		relay_parent_storage_root: relay_chain::Hash,
		proof: StorageProof,
	) -> Result<Self, Error> {
		let db = proof.into_memory_db::<HashingFor<relay_chain::Block>>();
		if !db.contains(&relay_parent_storage_root, EMPTY_PREFIX) {
			return Err(Error::RootMismatch);
		}
		let trie_backend = TrieBackendBuilder::new(db, relay_parent_storage_root).build();

		Ok(Self { para_id, trie_backend })
	}

	/// Read the [`MessagingStateSnapshot`] from the relay chain state proof.
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_messaging_state_snapshot(
		&self,
		host_config: &AbridgedHostConfiguration,
	) -> Result<MessagingStateSnapshot, Error> {
		let dmq_mqc_head: relay_chain::Hash = read_entry(
			&self.trie_backend,
			&relay_chain::well_known_keys::dmq_mqc_head(self.para_id),
			Some(Default::default()),
		)
		.map_err(Error::DmqMqcHead)?;

		let relay_dispatch_queue_remaining_capacity = read_optional_entry::<
			RelayDispatchQueueRemainingCapacity,
			_,
		>(
			&self.trie_backend,
			&relay_chain::well_known_keys::relay_dispatch_queue_remaining_capacity(self.para_id)
				.key,
		);

		// TODO paritytech/polkadot#6283: Remove all usages of `relay_dispatch_queue_size`
		//
		// When the relay chain and all parachains support
		// `relay_dispatch_queue_remaining_capacity`, this code here needs to be removed and above
		// needs to be changed to `read_entry` that returns an error if
		// `relay_dispatch_queue_remaining_capacity` can not be found/decoded.
		//
		// For now we just fallback to the old dispatch queue size on `ReadEntryErr::Absent`.
		// `ReadEntryErr::Decode` and `ReadEntryErr::Proof` are potentially subject to meddling
		// by malicious collators, so we reject the block in those cases.
		let relay_dispatch_queue_remaining_capacity = match relay_dispatch_queue_remaining_capacity
		{
			Ok(Some(r)) => r,
			Ok(None) => {
				let res = read_entry::<(u32, u32), _>(
					&self.trie_backend,
					#[allow(deprecated)]
					&relay_chain::well_known_keys::relay_dispatch_queue_size(self.para_id),
					Some((0, 0)),
				)
				.map_err(Error::RelayDispatchQueueRemainingCapacity)?;

				let remaining_count = host_config.max_upward_queue_count.saturating_sub(res.0);
				let remaining_size = host_config.max_upward_queue_size.saturating_sub(res.1);
				RelayDispatchQueueRemainingCapacity { remaining_count, remaining_size }
			},
			Err(e) => return Err(Error::RelayDispatchQueueRemainingCapacity(e)),
		};

		let ingress_channel_index: Vec<ParaId> = read_entry(
			&self.trie_backend,
			&relay_chain::well_known_keys::hrmp_ingress_channel_index(self.para_id),
			Some(Vec::new()),
		)
		.map_err(Error::HrmpIngressChannelIndex)?;

		let egress_channel_index: Vec<ParaId> = read_entry(
			&self.trie_backend,
			&relay_chain::well_known_keys::hrmp_egress_channel_index(self.para_id),
			Some(Vec::new()),
		)
		.map_err(Error::HrmpEgressChannelIndex)?;

		let mut ingress_channels = Vec::with_capacity(ingress_channel_index.len());
		for sender in ingress_channel_index {
			let channel_id = relay_chain::HrmpChannelId { sender, recipient: self.para_id };
			let hrmp_channel: AbridgedHrmpChannel = read_entry(
				&self.trie_backend,
				&relay_chain::well_known_keys::hrmp_channels(channel_id),
				None,
			)
			.map_err(|read_err| Error::HrmpChannel(sender, self.para_id, read_err))?;
			ingress_channels.push((sender, hrmp_channel));
		}

		let mut egress_channels = Vec::with_capacity(egress_channel_index.len());
		for recipient in egress_channel_index {
			let channel_id = relay_chain::HrmpChannelId { sender: self.para_id, recipient };
			let hrmp_channel: AbridgedHrmpChannel = read_entry(
				&self.trie_backend,
				&relay_chain::well_known_keys::hrmp_channels(channel_id),
				None,
			)
			.map_err(|read_err| Error::HrmpChannel(self.para_id, recipient, read_err))?;
			egress_channels.push((recipient, hrmp_channel));
		}

		// NOTE that ingress_channels and egress_channels promise to be sorted. We satisfy this
		// property by relying on the fact that `ingress_channel_index` and `egress_channel_index`
		// are themselves sorted.
		Ok(MessagingStateSnapshot {
			dmq_mqc_head,
			relay_dispatch_queue_remaining_capacity,
			ingress_channels,
			egress_channels,
		})
	}

	/// Read the [`AbridgedHostConfiguration`] from the relay chain state proof.
	///
	/// Decodes only the abridged prefix, deliberately *not* via
	/// `read_host_configuration_prefix` with the remainder discarded: this is a mandatory read on
	/// every block for every parachain, so it must not depend on the layout of relay fields past
	/// the ones parachains actually persist. Only v3-capable runtimes take on that wider
	/// dependency, and only via [`Self::read_relay_v3_feature_enabled`].
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_abridged_host_configuration(&self) -> Result<AbridgedHostConfiguration, Error> {
		read_entry(&self.trie_backend, relay_chain::well_known_keys::ACTIVE_CONFIG, None)
			.map_err(Error::Config)
	}

	/// Read the [`RelayHostConfigurationPrefix`] from the relay chain state proof.
	///
	/// This decodes further into the `ACTIVE_CONFIG` blob than [`AbridgedHostConfiguration`] does
	/// in order to reach `node_features`, and so additionally depends on the layout of every relay
	/// field in between. Only the abridged part is ever persisted on-chain.
	///
	/// Returns an error if anything failed at reading or decoding.
	fn read_host_configuration_prefix(&self) -> Result<RelayHostConfigurationPrefix, Error> {
		read_entry(&self.trie_backend, relay_chain::well_known_keys::ACTIVE_CONFIG, None)
			.map_err(Error::Config)
	}

	/// Read the relay `CandidateReceiptV3` node feature from the `ACTIVE_CONFIG` entry — the
	/// relay-side half of the two-sided V3 scheduling decision. Errors if the entry is missing or
	/// undecodable (the same entry the mandatory messaging path already requires).
	pub fn read_relay_v3_feature_enabled(&self) -> Result<bool, Error> {
		Ok(relay_chain::node_features::FeatureIndex::CandidateReceiptV3
			.is_set(&self.read_host_configuration_prefix()?.node_features))
	}

	/// Read latest included parachain [head data](`relay_chain::HeadData`) from the relay chain
	/// state proof.
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_included_para_head(&self) -> Result<relay_chain::HeadData, Error> {
		read_entry(&self.trie_backend, &relay_chain::well_known_keys::para_head(self.para_id), None)
			.map_err(Error::ParaHead)
	}

	/// Read relay chain authorities.
	pub fn read_authorities(
		&self,
	) -> Result<Vec<(sp_consensus_babe::AuthorityId, sp_consensus_babe::BabeAuthorityWeight)>, Error>
	{
		read_entry(&self.trie_backend, &relay_chain::well_known_keys::AUTHORITIES, None)
			.map_err(Error::Authorities)
	}

	/// Read relay chain authorities for the next epoch.
	pub fn read_next_authorities(
		&self,
	) -> Result<
		Option<Vec<(sp_consensus_babe::AuthorityId, sp_consensus_babe::BabeAuthorityWeight)>>,
		Error,
	> {
		read_optional_entry(&self.trie_backend, &relay_chain::well_known_keys::NEXT_AUTHORITIES)
			.map_err(Error::NextAuthorities)
	}

	/// Read the [`Slot`](relay_chain::Slot) from the relay chain state proof.
	///
	/// The slot is slot of the relay chain block this state proof was extracted from.
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_slot(&self) -> Result<relay_chain::Slot, Error> {
		read_entry(&self.trie_backend, relay_chain::well_known_keys::CURRENT_SLOT, None)
			.map_err(Error::Slot)
	}

	/// Read the go-ahead signal for the upgrade from the relay chain state proof.
	///
	/// The go-ahead specifies whether the parachain can apply the upgrade or should abort it. If
	/// the value is absent then there is either no judgment by the relay chain yet or no upgrade
	/// is pending.
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_upgrade_go_ahead_signal(
		&self,
	) -> Result<Option<relay_chain::UpgradeGoAhead>, Error> {
		read_optional_entry(
			&self.trie_backend,
			&relay_chain::well_known_keys::upgrade_go_ahead_signal(self.para_id),
		)
		.map_err(Error::UpgradeGoAhead)
	}

	/// Read the upgrade restriction signal for the upgrade from the relay chain state proof.
	///
	/// If the upgrade restriction is not `None`, then the parachain cannot signal an upgrade at
	/// this block.
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_upgrade_restriction_signal(
		&self,
	) -> Result<Option<relay_chain::UpgradeRestriction>, Error> {
		read_optional_entry(
			&self.trie_backend,
			&relay_chain::well_known_keys::upgrade_restriction_signal(self.para_id),
		)
		.map_err(Error::UpgradeRestriction)
	}

	/// Read an entry given by the key and try to decode it. If the value specified by the key
	/// according to the proof is empty, the `fallback` value will be returned.
	///
	/// Returns `Err` in case the backend can't return the value under the specific key (likely due
	/// to a malformed proof), in case the decoding fails, or in case where the value is empty in
	/// the relay chain state and no fallback was provided.
	pub fn read_entry<T>(&self, key: &[u8], fallback: Option<T>) -> Result<T, Error>
	where
		T: Decode,
	{
		read_entry(&self.trie_backend, key, fallback).map_err(Error::ReadEntry)
	}

	/// Read an optional entry given by the key and try to decode it.
	///
	/// Returns `Err` in case the backend can't return the value under the specific key (likely due
	/// to a malformed proof) or if the value couldn't be decoded.
	pub fn read_optional_entry<T>(&self, key: &[u8]) -> Result<Option<T>, Error>
	where
		T: Decode,
	{
		read_optional_entry(&self.trie_backend, key).map_err(Error::ReadOptionalEntry)
	}

	/// Read a value from a child trie in the relay chain state proof.
	///
	/// Returns `Ok(Some(value))` if the key exists in the child trie,
	/// `Ok(None)` if the key doesn't exist,
	/// or `Err` if there was a proof error.
	pub fn read_child_storage(
		&self,
		child_info: &sp_core::storage::ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Error> {
		use sp_state_machine::Backend;
		self.trie_backend
			.child_storage(child_info, key)
			.map_err(|_| Error::ReadEntry(ReadEntryErr::Proof))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_primitives::{node_features::FeatureIndex, AsyncBackingParams, NodeFeatures};
	use polkadot_runtime_parachains::configuration::HostConfiguration;

	/// Guards that [`RelayHostConfigurationPrefix`] stays a positional prefix of the relay
	/// `HostConfiguration<BlockNumber>`.
	///
	/// A FAILURE of this test means the relay-side field layout has diverged. Parachains decode
	/// this prefix straight out of the `ACTIVE_CONFIG` blob on every mandatory
	/// `set_validation_data` inherent, so divergence breaks active-config decoding for every
	/// parachain that has not yet upgraded to a matching runtime: it needs a coordinated migration
	/// and must not be shipped casually. The shorter `AbridgedHostConfiguration` prefix is guarded
	/// separately, by `verify_externally_accessible` in the relay configuration pallet.
	#[test]
	fn verify_relay_host_configuration_prefix() {
		let mut ground_truth = HostConfiguration::<u32>::default();
		// Every field gets a distinct non-default value, so swapping any two same-typed neighbours
		// on the relay side cannot decode cleanly here.
		ground_truth.max_code_size = 1;
		ground_truth.max_head_data_size = 2;
		ground_truth.max_upward_queue_count = 3;
		ground_truth.max_upward_queue_size = 4;
		ground_truth.max_upward_message_size = 5;
		ground_truth.max_upward_message_num_per_candidate = 6;
		ground_truth.hrmp_max_message_num_per_candidate = 7;
		ground_truth.validation_upgrade_cooldown = 8;
		ground_truth.validation_upgrade_delay = 9;
		ground_truth.async_backing_params =
			AsyncBackingParams { allowed_ancestry_len: 111, max_candidate_depth: 222 };
		ground_truth.max_pov_size = 12_345;
		ground_truth.max_downward_message_size = 10;
		ground_truth.hrmp_max_parachain_outbound_channels = 11;
		ground_truth.hrmp_sender_deposit = 12;
		ground_truth.hrmp_recipient_deposit = 13;
		ground_truth.hrmp_channel_max_capacity = 14;
		ground_truth.hrmp_channel_max_total_size = 15;
		ground_truth.hrmp_max_parachain_inbound_channels = 16;
		ground_truth.hrmp_channel_max_message_size = 17;
		ground_truth.code_retention_period = 18;
		ground_truth.max_validators = Some(19);
		ground_truth.dispute_period = 20;
		ground_truth.dispute_post_conclusion_acceptance_period = 21;
		ground_truth.no_show_slots = 22;
		ground_truth.n_delay_tranches = 23;
		ground_truth.zeroth_delay_tranche_width = 24;
		ground_truth.needed_approvals = 25;
		ground_truth.relay_vrf_modulo_samples = 26;
		ground_truth.pvf_voting_ttl = 27;
		ground_truth.minimum_validation_upgrade_delay = 28;
		ground_truth.minimum_backing_votes = 29;
		let mut node_features = NodeFeatures::EMPTY;
		node_features.resize(FeatureIndex::CandidateReceiptV3 as usize + 1, false);
		node_features.set(FeatureIndex::CandidateReceiptV3 as usize, true);
		ground_truth.node_features = node_features;

		let prefix = RelayHostConfigurationPrefix::decode(&mut &ground_truth.encode()[..])
			.expect("`HostConfiguration` must decode into `RelayHostConfigurationPrefix`");

		assert_eq!(
			prefix,
			RelayHostConfigurationPrefix {
				abridged: AbridgedHostConfiguration {
					max_code_size: ground_truth.max_code_size,
					max_head_data_size: ground_truth.max_head_data_size,
					max_upward_queue_count: ground_truth.max_upward_queue_count,
					max_upward_queue_size: ground_truth.max_upward_queue_size,
					max_upward_message_size: ground_truth.max_upward_message_size,
					max_upward_message_num_per_candidate: ground_truth
						.max_upward_message_num_per_candidate,
					hrmp_max_message_num_per_candidate: ground_truth
						.hrmp_max_message_num_per_candidate,
					validation_upgrade_cooldown: ground_truth.validation_upgrade_cooldown,
					validation_upgrade_delay: ground_truth.validation_upgrade_delay,
					async_backing_params: ground_truth.async_backing_params,
				},
				max_pov_size: ground_truth.max_pov_size,
				max_downward_message_size: ground_truth.max_downward_message_size,
				hrmp_max_parachain_outbound_channels: ground_truth
					.hrmp_max_parachain_outbound_channels,
				hrmp_sender_deposit: ground_truth.hrmp_sender_deposit,
				hrmp_recipient_deposit: ground_truth.hrmp_recipient_deposit,
				hrmp_channel_max_capacity: ground_truth.hrmp_channel_max_capacity,
				hrmp_channel_max_total_size: ground_truth.hrmp_channel_max_total_size,
				hrmp_max_parachain_inbound_channels: ground_truth
					.hrmp_max_parachain_inbound_channels,
				hrmp_channel_max_message_size: ground_truth.hrmp_channel_max_message_size,
				executor_params: ground_truth.executor_params.clone(),
				code_retention_period: ground_truth.code_retention_period,
				max_validators: ground_truth.max_validators,
				dispute_period: ground_truth.dispute_period,
				dispute_post_conclusion_acceptance_period: ground_truth
					.dispute_post_conclusion_acceptance_period,
				no_show_slots: ground_truth.no_show_slots,
				n_delay_tranches: ground_truth.n_delay_tranches,
				zeroth_delay_tranche_width: ground_truth.zeroth_delay_tranche_width,
				needed_approvals: ground_truth.needed_approvals,
				relay_vrf_modulo_samples: ground_truth.relay_vrf_modulo_samples,
				pvf_voting_ttl: ground_truth.pvf_voting_ttl,
				minimum_validation_upgrade_delay: ground_truth.minimum_validation_upgrade_delay,
				minimum_backing_votes: ground_truth.minimum_backing_votes,
				node_features: ground_truth.node_features.clone(),
			}
		);
	}
}
