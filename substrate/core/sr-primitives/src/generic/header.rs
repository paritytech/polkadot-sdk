// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Generic implementation of a block header.

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "std")]
use log::debug;
use crate::codec::{Decode, Encode, Codec, Input, Output, HasCompact, EncodeAsRef, Error};
use crate::traits::{
	self, Member, SimpleArithmetic, SimpleBitOps, MaybeDisplay, Hash as HashT, MaybeSerializeDebug,
	MaybeSerializeDebugButNotDeserialize
};
use crate::generic::Digest;
use primitives::U256;
use core::convert::TryFrom;

/// Abstraction over a block header for a substrate chain.
#[derive(PartialEq, Eq, Clone)]
#[cfg_attr(feature = "std", derive(Debug, Serialize, Deserialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "std", serde(deny_unknown_fields))]
pub struct Header<Number: Copy + Into<U256> + TryFrom<U256>, Hash: HashT> {
	/// The parent hash.
	pub parent_hash: Hash::Output,
	/// The block number.
	#[cfg_attr(feature = "std", serde(
		serialize_with = "serialize_number",
		deserialize_with = "deserialize_number"))]
	pub number: Number,
	/// The state trie merkle root
	pub state_root: Hash::Output,
	/// The merkle root of the extrinsics.
	pub extrinsics_root: Hash::Output,
	/// A chain-specific digest of data useful for light clients or referencing auxiliary data.
	pub digest: Digest<Hash::Output>,
}

#[cfg(feature = "std")]
pub fn serialize_number<S, T: Copy + Into<U256> + TryFrom<U256>>(
	val: &T, s: S,
) -> Result<S::Ok, S::Error> where S: serde::Serializer {
	let u256: U256 = (*val).into();
	serde::Serialize::serialize(&u256, s)
}

#[cfg(feature = "std")]
pub fn deserialize_number<'a, D, T: Copy + Into<U256> + TryFrom<U256>>(
	d: D,
) -> Result<T, D::Error> where D: serde::Deserializer<'a> {
	let u256: U256 = serde::Deserialize::deserialize(d)?;
	TryFrom::try_from(u256).map_err(|_| serde::de::Error::custom("Try from failed"))
}

impl<Number, Hash> Decode for Header<Number, Hash> where
	Number: HasCompact + Copy + Into<U256> + TryFrom<U256>,
	Hash: HashT,
	Hash::Output: Decode,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
		Ok(Header {
			parent_hash: Decode::decode(input)?,
			number: <<Number as HasCompact>::Type>::decode(input)?.into(),
			state_root: Decode::decode(input)?,
			extrinsics_root: Decode::decode(input)?,
			digest: Decode::decode(input)?,
		})
	}
}

impl<Number, Hash> Encode for Header<Number, Hash> where
	Number: HasCompact + Copy + Into<U256> + TryFrom<U256>,
	Hash: HashT,
	Hash::Output: Encode,
{
	fn encode_to<T: Output>(&self, dest: &mut T) {
		dest.push(&self.parent_hash);
		dest.push(&<<<Number as HasCompact>::Type as EncodeAsRef<_>>::RefType>::from(&self.number));
		dest.push(&self.state_root);
		dest.push(&self.extrinsics_root);
		dest.push(&self.digest);
	}
}

impl<Number, Hash> codec::EncodeLike for Header<Number, Hash> where
	Number: HasCompact + Copy + Into<U256> + TryFrom<U256>,
	Hash: HashT,
	Hash::Output: Encode,
{}

impl<Number, Hash> traits::Header for Header<Number, Hash> where
	Number: Member + MaybeSerializeDebug + rstd::hash::Hash + MaybeDisplay +
		SimpleArithmetic + Codec + Copy + Into<U256> + TryFrom<U256>,
	Hash: HashT,
	Hash::Output: Default + rstd::hash::Hash + Copy + Member +
		MaybeSerializeDebugButNotDeserialize + MaybeDisplay + SimpleBitOps + Codec,
{
	type Number = Number;
	type Hash = <Hash as HashT>::Output;
	type Hashing = Hash;

	fn number(&self) -> &Self::Number { &self.number }
	fn set_number(&mut self, num: Self::Number) { self.number = num }

	fn extrinsics_root(&self) -> &Self::Hash { &self.extrinsics_root }
	fn set_extrinsics_root(&mut self, root: Self::Hash) { self.extrinsics_root = root }

	fn state_root(&self) -> &Self::Hash { &self.state_root }
	fn set_state_root(&mut self, root: Self::Hash) { self.state_root = root }

	fn parent_hash(&self) -> &Self::Hash { &self.parent_hash }
	fn set_parent_hash(&mut self, hash: Self::Hash) { self.parent_hash = hash }

	fn digest(&self) -> &Digest<Self::Hash> { &self.digest }

	#[cfg(feature = "std")]
	fn digest_mut(&mut self) -> &mut Digest<Self::Hash> {
		debug!(target: "header", "Retrieving mutable reference to digest");
		&mut self.digest
	}

	#[cfg(not(feature = "std"))]
	fn digest_mut(&mut self) -> &mut Digest<Self::Hash> { &mut self.digest }

	fn new(
		number: Self::Number,
		extrinsics_root: Self::Hash,
		state_root: Self::Hash,
		parent_hash: Self::Hash,
		digest: Digest<Self::Hash>,
	) -> Self {
		Header {
			number,
			extrinsics_root,
			state_root,
			parent_hash,
			digest,
		}
	}
}

impl<Number, Hash> Header<Number, Hash> where
	Number: Member + rstd::hash::Hash + Copy + MaybeDisplay + SimpleArithmetic + Codec + Into<U256> + TryFrom<U256>,
	Hash: HashT,
	Hash::Output: Default + rstd::hash::Hash + Copy + Member + MaybeDisplay + SimpleBitOps + Codec,
 {
	/// Convenience helper for computing the hash of the header without having
	/// to import the trait.
	pub fn hash(&self) -> Hash::Output {
		Hash::hash_of(self)
	}
}

#[cfg(all(test, feature = "std"))]
mod tests {
	use super::*;

	#[test]
	fn should_serialize_numbers() {
		fn serialize(num: u128) -> String {
			let mut v = vec![];
			{
				let mut ser = serde_json::Serializer::new(std::io::Cursor::new(&mut v));
				serialize_number(&num, &mut ser).unwrap();
			}
			String::from_utf8(v).unwrap()
		}

		assert_eq!(serialize(0), "\"0x0\"".to_owned());
		assert_eq!(serialize(1), "\"0x1\"".to_owned());
		assert_eq!(serialize(u64::max_value() as u128), "\"0xffffffffffffffff\"".to_owned());
		assert_eq!(serialize(u64::max_value() as u128 + 1), "\"0x10000000000000000\"".to_owned());
	}

	#[test]
	fn should_deserialize_number() {
		fn deserialize(num: &str) -> u128 {
			let mut der = serde_json::Deserializer::new(serde_json::de::StrRead::new(num));
			deserialize_number(&mut der).unwrap()
		}

		assert_eq!(deserialize("\"0x0\""), 0);
		assert_eq!(deserialize("\"0x1\""), 1);
		assert_eq!(deserialize("\"0xffffffffffffffff\""), u64::max_value() as u128);
		assert_eq!(deserialize("\"0x10000000000000000\""), u64::max_value() as u128 + 1);
	}
}
