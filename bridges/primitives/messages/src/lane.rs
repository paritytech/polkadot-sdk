// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives of messages module, that represents lane id.

use codec::{Codec, Decode, DecodeWithMemTracking, Encode, EncodeLike, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sp_core::{RuntimeDebug, TypeId, H256};
use sp_io::hashing::blake2_256;
use sp_std::fmt::Debug;

/// Trait representing a generic `LaneId` type.
pub trait LaneIdType:
	Clone
	+ Copy
	+ Codec
	+ EncodeLike
	+ Debug
	+ Default
	+ PartialEq
	+ Eq
	+ Ord
	+ TypeInfo
	+ MaxEncodedLen
	+ Serialize
	+ DeserializeOwned
{
	/// Creates a new `LaneId` type (if supported).
	fn try_new<E: Ord + Encode>(endpoint1: E, endpoint2: E) -> Result<Self, ()>;
}

/// Bridge lane identifier (legacy).
///
/// Note: For backwards compatibility reasons, we also handle the older format `[u8; 4]`.
#[derive(
	Clone,
	Copy,
	Decode,
	DecodeWithMemTracking,
	Default,
	Encode,
	Eq,
	Ord,
	PartialOrd,
	PartialEq,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct LegacyLaneId(pub [u8; 4]);

impl LaneIdType for LegacyLaneId {
	/// Create lane identifier from two locations.
	fn try_new<T: Ord + Encode>(_endpoint1: T, _endpoint2: T) -> Result<Self, ()> {
		// we don't support this for `LegacyLaneId`, because it was hard-coded before
		Err(())
	}
}

#[cfg(feature = "std")]
impl TryFrom<Vec<u8>> for LegacyLaneId {
	type Error = ();

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		if value.len() == 4 {
			return <[u8; 4]>::try_from(value).map(Self).map_err(|_| ());
		}
		Err(())
	}
}

impl core::fmt::Debug for LegacyLaneId {
	fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
		self.0.fmt(fmt)
	}
}

impl AsRef<[u8]> for LegacyLaneId {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl TypeId for LegacyLaneId {
	const TYPE_ID: [u8; 4] = *b"blan";
}

/// Bridge lane identifier.
///
/// Lane connects two endpoints at both sides of the bridge. We assume that every endpoint
/// has its own unique identifier. We want lane identifiers to be **the same on the both sides
/// of the bridge** (and naturally unique across global consensus if endpoints have unique
/// identifiers). So lane id is the hash (`blake2_256`) of **ordered** encoded locations
/// concatenation (separated by some binary data). I.e.:
///
/// ```nocompile
/// let endpoint1 = X2(GlobalConsensus(NetworkId::Polkadot), Parachain(42));
/// let endpoint2 = X2(GlobalConsensus(NetworkId::Kusama), Parachain(777));
///
/// let final_lane_key = if endpoint1 < endpoint2 {
///     (endpoint1, VALUES_SEPARATOR, endpoint2)
/// } else {
///     (endpoint2, VALUES_SEPARATOR, endpoint1)
/// }.using_encoded(blake2_256);
/// ```
#[derive(
	Clone,
	Copy,
	Decode,
	DecodeWithMemTracking,
	Default,
	Encode,
	Eq,
	Ord,
	PartialOrd,
	PartialEq,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct HashedLaneId(H256);

impl HashedLaneId {
	/// Create lane identifier from given hash.
	///
	/// There's no `From<H256>` implementation for the `LaneId`, because using this conversion
	/// in a wrong way (i.e. computing hash of endpoints manually) may lead to issues. So we
	/// want the call to be explicit.
	#[cfg(feature = "std")]
	pub const fn from_inner(inner: H256) -> Self {
		Self(inner)
	}

	/// Access the inner lane representation.
	pub fn inner(&self) -> &H256 {
		&self.0
	}
}

impl core::fmt::Display for HashedLaneId {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		core::fmt::Display::fmt(&self.0, f)
	}
}

impl core::fmt::Debug for HashedLaneId {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		core::fmt::Debug::fmt(&self.0, f)
	}
}

impl TypeId for HashedLaneId {
	const TYPE_ID: [u8; 4] = *b"hlan";
}

impl LaneIdType for HashedLaneId {
	/// Create lane identifier from two locations.
	fn try_new<T: Ord + Encode>(endpoint1: T, endpoint2: T) -> Result<Self, ()> {
		const VALUES_SEPARATOR: [u8; 31] = *b"bridges-lane-id-value-separator";

		Ok(Self(
			if endpoint1 < endpoint2 {
				(endpoint1, VALUES_SEPARATOR, endpoint2)
			} else {
				(endpoint2, VALUES_SEPARATOR, endpoint1)
			}
			.using_encoded(blake2_256)
			.into(),
		))
	}
}

#[cfg(feature = "std")]
impl TryFrom<Vec<u8>> for HashedLaneId {
	type Error = ();

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		if value.len() == 32 {
			return <[u8; 32]>::try_from(value).map(|v| Self(H256::from(v))).map_err(|_| ());
		}
		Err(())
	}
}

/// Lane state.
#[derive(Clone, Copy, Decode, Encode, Eq, PartialEq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum LaneState {
	/// Lane is opened and messages may be sent/received over it.
	Opened,
	/// Lane is closed and all attempts to send/receive messages to/from this lane
	/// will fail.
	///
	/// Keep in mind that the lane has two ends and the state of the same lane at
	/// its ends may be different. Those who are controlling/serving the lane
	/// and/or sending messages over the lane, have to coordinate their actions on
	/// both ends to make sure that lane is operating smoothly on both ends.
	Closed,
}

impl LaneState {
	/// Returns true if lane state allows sending/receiving messages.
	pub fn is_active(&self) -> bool {
		matches!(*self, LaneState::Opened)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::MessageNonce;

	#[test]
	fn lane_id_debug_format_matches_inner_hash_format() {
		assert_eq!(
			format!("{:?}", HashedLaneId(H256::from([1u8; 32]))),
			format!("{:?}", H256::from([1u8; 32])),
		);
		assert_eq!(format!("{:?}", LegacyLaneId([0, 0, 0, 1])), format!("{:?}", [0, 0, 0, 1]),);
	}

	#[test]
	fn hashed_encode_decode_works() {
		// simple encode/decode - new format
		let lane_id = HashedLaneId(H256::from([1u8; 32]));
		let encoded_lane_id = lane_id.encode();
		let decoded_lane_id = HashedLaneId::decode(&mut &encoded_lane_id[..]).expect("decodable");
		assert_eq!(lane_id, decoded_lane_id);
		assert_eq!(
			"0101010101010101010101010101010101010101010101010101010101010101",
			hex::encode(encoded_lane_id)
		);
	}

	#[test]
	fn legacy_encode_decode_works() {
		// simple encode/decode - old format
		let lane_id = LegacyLaneId([0, 0, 0, 1]);
		let encoded_lane_id = lane_id.encode();
		let decoded_lane_id = LegacyLaneId::decode(&mut &encoded_lane_id[..]).expect("decodable");
		assert_eq!(lane_id, decoded_lane_id);
		assert_eq!("00000001", hex::encode(encoded_lane_id));

		// decode sample
		let bytes = vec![0, 0, 0, 2, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
		let (lane, nonce_start, nonce_end): (LegacyLaneId, MessageNonce, MessageNonce) =
			Decode::decode(&mut &bytes[..]).unwrap();
		assert_eq!(lane, LegacyLaneId([0, 0, 0, 2]));
		assert_eq!(nonce_start, 1);
		assert_eq!(nonce_end, 1);

		// run encode/decode for `LaneId` with different positions
		let expected_lane = LegacyLaneId([0, 0, 0, 1]);
		let expected_nonce_start = 1088_u64;
		let expected_nonce_end = 9185_u64;

		// decode: LaneId,Nonce,Nonce
		let bytes = (expected_lane, expected_nonce_start, expected_nonce_end).encode();
		let (lane, nonce_start, nonce_end): (LegacyLaneId, MessageNonce, MessageNonce) =
			Decode::decode(&mut &bytes[..]).unwrap();
		assert_eq!(lane, expected_lane);
		assert_eq!(nonce_start, expected_nonce_start);
		assert_eq!(nonce_end, expected_nonce_end);

		// decode: Nonce,LaneId,Nonce
		let bytes = (expected_nonce_start, expected_lane, expected_nonce_end).encode();
		let (nonce_start, lane, nonce_end): (MessageNonce, LegacyLaneId, MessageNonce) =
			Decode::decode(&mut &bytes[..]).unwrap();
		assert_eq!(lane, expected_lane);
		assert_eq!(nonce_start, expected_nonce_start);
		assert_eq!(nonce_end, expected_nonce_end);

		// decode: Nonce,Nonce,LaneId
		let bytes = (expected_nonce_start, expected_nonce_end, expected_lane).encode();
		let (nonce_start, nonce_end, lane): (MessageNonce, MessageNonce, LegacyLaneId) =
			Decode::decode(&mut &bytes[..]).unwrap();
		assert_eq!(lane, expected_lane);
		assert_eq!(nonce_start, expected_nonce_start);
		assert_eq!(nonce_end, expected_nonce_end);
	}

	#[test]
	fn hashed_lane_id_is_generated_using_ordered_endpoints() {
		assert_eq!(HashedLaneId::try_new(1, 2).unwrap(), HashedLaneId::try_new(2, 1).unwrap());
	}

	#[test]
	fn hashed_lane_id_is_different_for_different_endpoints() {
		assert_ne!(HashedLaneId::try_new(1, 2).unwrap(), HashedLaneId::try_new(1, 3).unwrap());
	}

	#[test]
	fn hashed_lane_id_is_different_even_if_arguments_has_partial_matching_encoding() {
		/// Some artificial type that generates the same encoding for different values
		/// concatenations. I.e. the encoding for `(Either::Two(1, 2), Either::Two(3, 4))`
		/// is the same as encoding of `(Either::Three(1, 2, 3), Either::One(4))`.
		/// In practice, this type is not useful, because you can't do a proper decoding.
		/// But still there may be some collisions even in proper types.
		#[derive(Eq, Ord, PartialEq, PartialOrd)]
		enum Either {
			Three(u64, u64, u64),
			Two(u64, u64),
			One(u64),
		}

		impl codec::Encode for Either {
			fn encode(&self) -> Vec<u8> {
				match *self {
					Self::One(a) => a.encode(),
					Self::Two(a, b) => (a, b).encode(),
					Self::Three(a, b, c) => (a, b, c).encode(),
				}
			}
		}

		assert_ne!(
			HashedLaneId::try_new(Either::Two(1, 2), Either::Two(3, 4)).unwrap(),
			HashedLaneId::try_new(Either::Three(1, 2, 3), Either::One(4)).unwrap(),
		);
	}
}
