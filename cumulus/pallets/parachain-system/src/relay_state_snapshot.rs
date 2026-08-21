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
};
use scale_info::TypeInfo;
#[cfg(not(feature = "std"))]
use sp_runtime::traits::HashingFor;
#[cfg(not(feature = "std"))]
use sp_state_machine::{Backend, TrieBackendBuilder};
#[cfg(not(feature = "std"))]
use sp_trie::{HashDBT, MemoryDB, ProofSizeProvider, StorageProof, EMPTY_PREFIX};

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
/// Read the raw stored bytes under `key` from the relay chain state via the
/// [`read_relay_chain_state`](sp_additional_data::additional_data::read_relay_chain_state) host
/// function. Returns `Ok(None)` for a (proven) absent key.
///
/// The host function returns the SCALE-encoding of `Option<Vec<u8>>`; a decode failure here means a
/// malformed host response (a missing `AdditionalDataExt` panics inside the host function itself).
fn host_read_raw(key: &[u8]) -> Result<Option<Vec<u8>>, ReadEntryErr> {
	let encoded = sp_additional_data::additional_data::read_relay_chain_state(key.to_vec());
	<Option<Vec<u8>>>::decode(&mut &encoded[..]).map_err(|_| ReadEntryErr::Proof)
}

/// Read an entry via the host function and try to decode it, falling back to `fallback` when the
/// key is (provably) absent. Mirrors the previous trie-backed `read_entry`.
fn host_read_entry<T: Decode>(key: &[u8], fallback: Option<T>) -> Result<T, ReadEntryErr> {
	host_read_raw(key)?
		.map(|raw| T::decode(&mut &raw[..]).map_err(|_| ReadEntryErr::Decode))
		.transpose()?
		.or(fallback)
		.ok_or(ReadEntryErr::Absent)
}

/// Read an optional entry via the host function. Returns `None` for a (provably) absent key.
fn host_read_optional_entry<T: Decode>(key: &[u8]) -> Result<Option<T>, ReadEntryErr> {
	match host_read_entry(key, None) {
		Ok(v) => Ok(Some(v)),
		Err(ReadEntryErr::Absent) => Ok(None),
		Err(err) => Err(err),
	}
}

/// A trie-backed reader over a relay-state proof.
///
/// Used on the validation/import side to *serve* the `read_relay_chain_state` host function from the
/// proof recorded in the block's additional data (it is not used by the runtime's read path, which
/// goes through the host function). Kept minimal: it only needs to answer raw reads.
#[cfg(not(feature = "std"))]
pub struct TrieRelayStateReader {
	root: relay_chain::Hash,
	db: MemoryDB<HashingFor<relay_chain::Block>>,
	recorder:
		crate::validate_block::trie_recorder::ProofRecorderProvider<HashingFor<relay_chain::Block>>,
}

#[cfg(not(feature = "std"))]
impl TrieRelayStateReader {
	/// Build from a relay-state `proof`, verifying it against the trusted `root`.
	///
	/// Returns an error if `root` is not the root of `proof`.
	pub fn new(root: relay_chain::Hash, proof: StorageProof) -> Result<Self, Error> {
		let db = proof.into_memory_db::<HashingFor<relay_chain::Block>>();
		if !db.contains(&root, EMPTY_PREFIX) {
			return Err(Error::RootMismatch);
		}
		Ok(Self {
			root,
			db,
			recorder: crate::validate_block::trie_recorder::ProofRecorderProvider::default(),
		})
	}

	/// Read the raw stored bytes under `key` (proven absence returns `Ok(None)`), recording the
	/// nodes accessed so [`Self::requested_hash`] can reassemble exactly what was read.
	///
	/// A fresh (empty) cache is used per read so the recorder observes every node on the access
	/// path. `new_with_cache` (rather than `new`) is what lets us give the backend our
	/// [`ProofRecorderProvider`] instead of the no_std default (unimplemented) recorder.
	pub fn read_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
		let cache_provider =
			crate::validate_block::trie_cache::CacheProvider::<HashingFor<relay_chain::Block>>::new();
		let recording = TrieBackendBuilder::new_with_cache(&self.db, self.root, &cache_provider)
			.with_recorder(self.recorder.clone())
			.build();
		recording.storage(key).map_err(|_| Error::ReadOptionalEntry(ReadEntryErr::Proof))
	}

	/// blake2 hash of the additional-data map reassembled from exactly the nodes read so far, or
	/// `None` if nothing was read. Mirrors what the collator committed for an honest, minimal
	/// candidate: `frame_executive`'s digest-equality rejects a candidate whose carried map differs
	/// from what its execution actually requested.
	pub fn requested_hash(&self) -> Option<[u8; 32]> {
		let proof = self.recorder.to_storage_proof();
		if proof.is_empty() {
			return None;
		}
		let mut map = sp_additional_data::AdditionalData::new();
		map.insert(sp_additional_data::RELAY_PROOF_KEY.into(), (self.root, proof).encode());
		Some(sp_additional_data::hash(&map))
	}

	/// Estimated encoded size of the relay-read proof recorded so far — the additional-data
	/// contribution to the PoV. Summed into `storage_proof_size` so the runtime budgets for it.
	/// Uses the same per-node metric as the build-side recorder (see `ProofRecorderProvider`).
	pub fn proof_size(&self) -> usize {
		self.recorder.estimate_encoded_size()
	}
}

/// Reader for the relay chain state, backed by the `read_relay_chain_state` host function.
///
/// Every read is served dynamically: on block building it reads the live relay state and records a
/// minimal proof into the block's additional data; on validation/import it reads back from — and is
/// verified against — that recorded proof. This replaces the previous fixed relay-state proof that
/// used to be carried in the parachain inherent.
///
/// All read methods require the `AdditionalDataExt` externalities extension to be registered (it is,
/// on every execution path: block building, `validate_block`, and generic import); the host function
/// panics otherwise, as the read is consensus-critical.
pub struct RelayChainStateProof {
	para_id: ParaId,
}

impl RelayChainStateProof {
	/// Create a new reader for the given `para_id`.
	pub fn new(para_id: ParaId) -> Self {
		Self { para_id }
	}

	/// Read the [`MessagingStateSnapshot`] from the relay chain state.
	///
	/// Returns an error if anything failed at reading or decoding.
	pub fn read_messaging_state_snapshot(
		&self,
		host_config: &AbridgedHostConfiguration,
	) -> Result<MessagingStateSnapshot, Error> {
		let dmq_mqc_head: relay_chain::Hash = host_read_entry(
			&relay_chain::well_known_keys::dmq_mqc_head(self.para_id),
			Some(Default::default()),
		)
		.map_err(Error::DmqMqcHead)?;

		let relay_dispatch_queue_remaining_capacity =
			host_read_optional_entry::<RelayDispatchQueueRemainingCapacity>(
				&relay_chain::well_known_keys::relay_dispatch_queue_remaining_capacity(self.para_id)
					.key,
			);

		// TODO paritytech/polkadot#6283: Remove all usages of `relay_dispatch_queue_size`
		//
		// When the relay chain and all parachains support
		// `relay_dispatch_queue_remaining_capacity`, this code here needs to be removed and above
		// needs to be changed to `host_read_entry` that returns an error if
		// `relay_dispatch_queue_remaining_capacity` can not be found/decoded.
		let relay_dispatch_queue_remaining_capacity = match relay_dispatch_queue_remaining_capacity
		{
			Ok(Some(r)) => r,
			Ok(None) => {
				let res = host_read_entry::<(u32, u32)>(
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

		let ingress_channel_index: Vec<ParaId> = host_read_entry(
			&relay_chain::well_known_keys::hrmp_ingress_channel_index(self.para_id),
			Some(Vec::new()),
		)
		.map_err(Error::HrmpIngressChannelIndex)?;

		let egress_channel_index: Vec<ParaId> = host_read_entry(
			&relay_chain::well_known_keys::hrmp_egress_channel_index(self.para_id),
			Some(Vec::new()),
		)
		.map_err(Error::HrmpEgressChannelIndex)?;

		let mut ingress_channels = Vec::with_capacity(ingress_channel_index.len());
		for sender in ingress_channel_index {
			let channel_id = relay_chain::HrmpChannelId { sender, recipient: self.para_id };
			let hrmp_channel: AbridgedHrmpChannel =
				host_read_entry(&relay_chain::well_known_keys::hrmp_channels(channel_id), None)
					.map_err(|read_err| Error::HrmpChannel(sender, self.para_id, read_err))?;
			ingress_channels.push((sender, hrmp_channel));
		}

		let mut egress_channels = Vec::with_capacity(egress_channel_index.len());
		for recipient in egress_channel_index {
			let channel_id = relay_chain::HrmpChannelId { sender: self.para_id, recipient };
			let hrmp_channel: AbridgedHrmpChannel =
				host_read_entry(&relay_chain::well_known_keys::hrmp_channels(channel_id), None)
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

	/// Read the [`AbridgedHostConfiguration`] from the relay chain state.
	pub fn read_abridged_host_configuration(&self) -> Result<AbridgedHostConfiguration, Error> {
		host_read_entry(relay_chain::well_known_keys::ACTIVE_CONFIG, None).map_err(Error::Config)
	}

	/// Read latest included parachain [head data](`relay_chain::HeadData`) from the relay chain
	/// state.
	pub fn read_included_para_head(&self) -> Result<relay_chain::HeadData, Error> {
		host_read_entry(&relay_chain::well_known_keys::para_head(self.para_id), None)
			.map_err(Error::ParaHead)
	}

	/// Read relay chain authorities.
	pub fn read_authorities(
		&self,
	) -> Result<Vec<(sp_consensus_babe::AuthorityId, sp_consensus_babe::BabeAuthorityWeight)>, Error>
	{
		host_read_entry(&relay_chain::well_known_keys::AUTHORITIES, None).map_err(Error::Authorities)
	}

	/// Read relay chain authorities for the next epoch.
	pub fn read_next_authorities(
		&self,
	) -> Result<
		Option<Vec<(sp_consensus_babe::AuthorityId, sp_consensus_babe::BabeAuthorityWeight)>>,
		Error,
	> {
		host_read_optional_entry(&relay_chain::well_known_keys::NEXT_AUTHORITIES)
			.map_err(Error::NextAuthorities)
	}

	/// Read the [`Slot`](relay_chain::Slot) of the relay chain block this state was read from.
	pub fn read_slot(&self) -> Result<relay_chain::Slot, Error> {
		host_read_entry(relay_chain::well_known_keys::CURRENT_SLOT, None).map_err(Error::Slot)
	}

	/// Read the go-ahead signal for a pending code upgrade.
	pub fn read_upgrade_go_ahead_signal(
		&self,
	) -> Result<Option<relay_chain::UpgradeGoAhead>, Error> {
		host_read_optional_entry(&relay_chain::well_known_keys::upgrade_go_ahead_signal(
			self.para_id,
		))
		.map_err(Error::UpgradeGoAhead)
	}

	/// Read the upgrade restriction signal.
	pub fn read_upgrade_restriction_signal(
		&self,
	) -> Result<Option<relay_chain::UpgradeRestriction>, Error> {
		host_read_optional_entry(&relay_chain::well_known_keys::upgrade_restriction_signal(
			self.para_id,
		))
		.map_err(Error::UpgradeRestriction)
	}

	/// Read an entry given by the key and try to decode it, falling back to `fallback` when the key
	/// is (provably) absent.
	pub fn read_entry<T>(&self, key: &[u8], fallback: Option<T>) -> Result<T, Error>
	where
		T: Decode,
	{
		host_read_entry(key, fallback).map_err(Error::ReadEntry)
	}

	/// Read an optional entry given by the key and try to decode it.
	pub fn read_optional_entry<T>(&self, key: &[u8]) -> Result<Option<T>, Error>
	where
		T: Decode,
	{
		host_read_optional_entry(key).map_err(Error::ReadOptionalEntry)
	}

	/// Read the raw stored bytes under `key` (proven absence returns `Ok(None)`).
	pub fn read_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
		host_read_raw(key).map_err(Error::ReadOptionalEntry)
	}
}
