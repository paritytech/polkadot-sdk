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

use codec::{Encode, Decode};
use cumulus_primitives::{relay_chain, AbridgedHostConfiguration, AbridgedHrmpChannel, ParaId};
use hash_db::{HashDB, EMPTY_PREFIX};
use sp_runtime::traits::HashFor;
use sp_state_machine::{Backend, TrieBackend};
use sp_trie::StorageProof;
use sp_std::vec::Vec;

/// A snapshot of some messaging related state of relay chain pertaining to the current parachain.
///
/// This data is essential for making sure that the parachain is aware of current resource use on
/// the relay chain and that the candidates produced for this parachain do not exceed any of these
/// limits.
#[derive(Encode, Decode)]
pub struct MessagingStateSnapshot {
	/// The current capacity of the upward message queue of the current parachain on the relay chain.
	///
	/// The capacity is represented by a tuple that consist of the `count` of the messages and the
	/// `total_size` expressed as the sum of byte sizes of all messages in the queue.
	pub relay_dispatch_queue_size: (u32, u32),

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
	/// The host configuration cannot be extracted.
	Config(ReadEntryErr),
	/// Relay dispatch queue cannot be extracted.
	RelayDispatchQueueSize(ReadEntryErr),
	/// The hrmp egress channel index cannot be extracted.
	HrmpEgressChannelIndex(ReadEntryErr),
	/// The hrmp channel for the given recipient cannot be extracted.
	HrmpChannel(ParaId, ReadEntryErr),
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
	B: Backend<HashFor<relay_chain::Block>>,
{
	backend
		.storage(key)
		.map_err(|_| ReadEntryErr::Proof)?
		.map(|raw_entry| T::decode(&mut &raw_entry[..]).map_err(|_| ReadEntryErr::Decode))
		.transpose()?
		.or(fallback)
		.ok_or(ReadEntryErr::Absent)
}

/// Extract the relay chain state from the given storage proof. This function accepts the `para_id`
/// of the current parachain and the expected storage root the proof should stem from.
pub fn extract_from_proof(
	para_id: ParaId,
	relay_storage_root: relay_chain::v1::Hash,
	proof: StorageProof,
) -> Result<(AbridgedHostConfiguration, MessagingStateSnapshot), Error> {
	let db = proof.into_memory_db::<HashFor<relay_chain::Block>>();
	if !db.contains(&relay_storage_root, EMPTY_PREFIX) {
		return Err(Error::RootMismatch);
	}
	let backend = TrieBackend::new(db, relay_storage_root);

	let host_config: AbridgedHostConfiguration = read_entry(
		&backend,
		relay_chain::well_known_keys::ACTIVE_CONFIG,
		None,
	)
	.map_err(Error::Config)?;

	let relay_dispatch_queue_size: (u32, u32) = read_entry(
		&backend,
		&relay_chain::well_known_keys::relay_dispatch_queue_size(para_id),
		Some((0, 0)),
	)
	.map_err(Error::RelayDispatchQueueSize)?;

	let egress_channel_index: Vec<ParaId> = read_entry(
		&backend,
		&relay_chain::well_known_keys::hrmp_egress_channel_index(para_id),
		Some(Vec::new()),
	)
	.map_err(Error::HrmpEgressChannelIndex)?;

	let mut egress_channels = Vec::with_capacity(egress_channel_index.len());
	for recipient in egress_channel_index {
		let channel_id = relay_chain::v1::HrmpChannelId {
			sender: para_id,
			recipient,
		};
		let hrmp_channel: AbridgedHrmpChannel = read_entry(
			&backend,
			&relay_chain::well_known_keys::hrmp_channels(channel_id),
			None,
		)
		.map_err(|read_err| Error::HrmpChannel(recipient, read_err))?;
		egress_channels.push((recipient, hrmp_channel));
	}

	// NOTE that egress_channels promises to be sorted. We satisfy this property by relying on
	// the fact that `egress_channel_index` is itself sorted.

	Ok((
		host_config,
		MessagingStateSnapshot {
			relay_dispatch_queue_size,
			egress_channels,
		},
	))
}
