// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Serialization types for disk persistence of collator reputation data.

use codec::{Decode, Encode};
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::{BlockNumber, Id as ParaId};
use std::collections::HashMap;

use super::db::ScoreEntry;

/// Key prefix for per-para reputation data.
pub const REPUTATION_PARA_PREFIX: &[u8; 12] = b"Rep_per_para";
/// Key for metadata.
pub const REPUTATION_META_KEY: &[u8; 8] = b"Rep_meta";

/// Serializable PeerId wrapper.
/// PeerId is a Multihash which can be converted to/from bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializablePeerId(pub PeerId);

impl Encode for SerializablePeerId {
	fn encode(&self) -> Vec<u8> {
		self.0.to_bytes().encode()
	}

	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		self.0.to_bytes().encode_to(dest)
	}
}

impl Decode for SerializablePeerId {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let bytes = Vec::<u8>::decode(input)?;
		PeerId::from_bytes(&bytes)
			.map(SerializablePeerId)
			.map_err(|_| codec::Error::from("Invalid PeerId bytes"))
	}
}

/// Stored reputations for a single para.
/// This is the VALUE stored in the DB, keyed by ParaId.
#[derive(Debug, Clone, Encode, Decode, Default)]
pub struct StoredParaReputations {
	/// Vec of (peer_id, score_entry) pairs.
	pub entries: Vec<(SerializablePeerId, ScoreEntry)>,
}

impl StoredParaReputations {
	/// Convert from in-memory HashMap to storable format.
	pub fn from_hashmap(map: &HashMap<PeerId, ScoreEntry>) -> Self {
		let entries = map
			.iter()
			.map(|(peer_id, entry)| (SerializablePeerId(*peer_id), *entry))
			.collect();
		StoredParaReputations { entries }
	}

	/// Convert to in-memory HashMap.
	pub fn to_hashmap(&self) -> HashMap<PeerId, ScoreEntry> {
		self.entries.iter().map(|(peer_id, entry)| (peer_id.0, *entry)).collect()
	}
}

/// Metadata stored separately from per-para data.
#[derive(Debug, Clone, Encode, Decode)]
pub struct StoredMetadata {
	/// The last finalized block number that was processed.
	pub last_finalized: Option<BlockNumber>,
}

/// Generate key for a para's reputation data.
/// Key format: "Rep_per_para" (12 bytes) + ParaId (4 bytes, big-endian)
/// Using big-endian for lexicographic ordering when iterating.
pub fn para_reputation_key(para_id: ParaId) -> [u8; 16] {
	let mut key = [0u8; 12 + 4];
	key[..12].copy_from_slice(REPUTATION_PARA_PREFIX);
	// Use big-endian for lexicographic ordering
	key[12..].copy_from_slice(&u32::from(para_id).to_be_bytes());
	key
}

/// Returns the metadata key.
pub fn metadata_key() -> &'static [u8] {
	REPUTATION_META_KEY
}

/// Decode a para key to extract the ParaId.
/// Returns None if the key doesn't match the expected format.
pub fn decode_para_key(key: &[u8]) -> Option<ParaId> {
	if key.len() != 16 || !key.starts_with(REPUTATION_PARA_PREFIX) {
		return None
	}
	let mut bytes = [0u8; 4];
	bytes.copy_from_slice(&key[12..16]);
	Some(ParaId::from(u32::from_be_bytes(bytes)))
}

/// Errors during persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("Codec error: {0}")]
	Codec(#[from] codec::Error),
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::validator_side_experimental::peer_manager::Score;

	#[test]
	fn stored_para_reputations_roundtrip() {
		let mut map = HashMap::new();
		let peer1 = PeerId::random();
		let peer2 = PeerId::random();

		map.insert(peer1, ScoreEntry { score: Score::new(100).unwrap(), last_bumped: 1234567890 });
		map.insert(peer2, ScoreEntry { score: Score::new(50).unwrap(), last_bumped: 9876543210 });

		let stored = StoredParaReputations::from_hashmap(&map);
		let encoded = stored.encode();
		let decoded = StoredParaReputations::decode(&mut &encoded[..]).expect("decode should work");

		let restored_map = decoded.to_hashmap();

		assert_eq!(restored_map.len(), 2);
		assert_eq!(restored_map.get(&peer1).unwrap().score, Score::new(100).unwrap());
		assert_eq!(restored_map.get(&peer2).unwrap().score, Score::new(50).unwrap());
	}
}
