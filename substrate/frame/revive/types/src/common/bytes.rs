// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use alloc::{format, string::String, vec::Vec};
use alloy_core::hex;
use codec::{Decode, Encode};
use core::{
	fmt::{Debug, Display, Formatter, Result as FmtResult},
	str::FromStr,
};
use ethereum_types::U256;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{bounded::BoundedVec, ConstU32};

macro_rules! impl_hex {
	($type:ident, $inner:ty, $default:expr) => {
		#[doc = concat!("`", stringify!($inner), "` wrapper for JSON hex encoding.")]
		#[derive(
			Encode,
			Decode,
			Eq,
			PartialEq,
			Ord,
			PartialOrd,
			TypeInfo,
			Clone,
			Serialize,
			Deserialize,
			Hash,
		)]
		pub struct $type(
			/// The wrapped value encoded as a JSON hex string.
			#[serde(with = "hex_serde")]
			pub $inner,
		);

		impl Default for $type {
			fn default() -> Self {
				$type($default)
			}
		}

		impl From<$inner> for $type {
			fn from(inner: $inner) -> Self {
				$type(inner)
			}
		}

		impl Debug for $type {
			fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
				let hex = self.0.to_hex();
				let truncated = &hex[..hex.len().min(100)];
				let ellipsis = if hex.len() > 100 { "..." } else { "" };
				write!(f, "{}({}{})", stringify!($type), truncated, ellipsis)
			}
		}

		impl Display for $type {
			fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
				write!(f, "{}", self.0.to_hex())
			}
		}
	};
}

impl_hex!(Bytes, Vec<u8>, Vec::new());
impl_hex!(Byte, u8, 0u8);
impl_hex!(Bytes8, [u8; 8], [0u8; 8]);
impl_hex!(Bytes32, [u8; 32], [0u8; 32]);
impl_hex!(Bytes256, [u8; 256], [0u8; 256]);

impl FromStr for Bytes {
	type Err = hex::FromHexError;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		let data = hex::decode(value.trim_start_matches("0x"))?;
		Ok(Bytes(data))
	}
}

impl Bytes {
	/// Returns whether the byte collection contains no bytes.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Converts to minimal hex format without padding zeroes.
	pub fn to_short_hex(&self) -> String {
		let word = U256::from_big_endian(&self.0);
		format!("0x{:x}", word)
	}

	/// Converts to hex format without the `0x` prefix.
	pub fn to_hex_no_prefix(&self) -> String {
		hex::encode(&self.0)
	}
}

/// Bounded byte sequence with the same SCALE encoding and bounded decode semantics as
/// `BoundedVec<u8, ConstU32<_>>`.
#[derive(
	Debug,
	Default,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	TypeInfo,
	Encode,
	Decode,
	Serialize,
	Deserialize,
)]
#[serde(transparent)]
pub struct BoundedBytes<const LIMIT: u32>(
	/// Bounded raw bytes.
	#[serde(with = "bounded_hex_serde")]
	pub BoundedVec<u8, ConstU32<LIMIT>>,
);

impl<const LIMIT: u32> core::hash::Hash for BoundedBytes<LIMIT> {
	fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
		core::hash::Hash::hash(&self.0[..], state);
	}
}

impl<const LIMIT: u32> From<BoundedVec<u8, ConstU32<LIMIT>>> for BoundedBytes<LIMIT> {
	fn from(value: BoundedVec<u8, ConstU32<LIMIT>>) -> Self {
		Self(value)
	}
}

impl<const LIMIT: u32> TryFrom<Bytes> for BoundedBytes<LIMIT> {
	type Error = Bytes;

	fn try_from(value: Bytes) -> Result<Self, Self::Error> {
		BoundedVec::try_from(value.0).map(Self).map_err(Bytes)
	}
}

impl<const LIMIT: u32> TryFrom<Vec<u8>> for BoundedBytes<LIMIT> {
	type Error = Vec<u8>;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		BoundedVec::try_from(value).map(Self)
	}
}

/// Conversion to and from `0x`-prefixed hex strings, used by JSON-facing
/// helpers to encode the byte newtypes produced by [`impl_hex`].
trait HexCodec: Sized {
	/// Error returned when [`HexCodec::from_hex`] receives a string that does
	/// not parse as the expected hex shape.
	type Error;

	/// Renders the value as a `0x`-prefixed hex string.
	fn to_hex(&self) -> String;
	/// Parses a `0x`-prefixed hex string into the value, returning
	/// [`Self::Error`] on a malformed input.
	fn from_hex(value: String) -> Result<Self, Self::Error>;
}

impl HexCodec for u8 {
	type Error = core::num::ParseIntError;

	fn to_hex(&self) -> String {
		format!("0x{self:x}")
	}

	fn from_hex(value: String) -> Result<Self, Self::Error> {
		u8::from_str_radix(value.trim_start_matches("0x"), 16)
	}
}

/// `serde` adapter that keeps bounded bytes JSON-compatible with the unbounded common byte wrapper
/// while preserving bounded construction.
mod bounded_hex_serde {
	use super::{BoundedVec, Bytes, ConstU32};
	use serde::{Deserialize, Deserializer, Serialize, Serializer};

	/// Serializes bounded bytes as a `0x`-prefixed hex string.
	pub(super) fn serialize<S, const LIMIT: u32>(
		value: &BoundedVec<u8, ConstU32<LIMIT>>,
		serializer: S,
	) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		Bytes(<BoundedVec<u8, ConstU32<LIMIT>> as AsRef<[u8]>>::as_ref(value).to_vec())
			.serialize(serializer)
	}

	/// Deserializes a `0x`-prefixed hex string into bounded bytes.
	pub(super) fn deserialize<'de, D, const LIMIT: u32>(
		deserializer: D,
	) -> Result<BoundedVec<u8, ConstU32<LIMIT>>, D::Error>
	where
		D: Deserializer<'de>,
	{
		let bytes = Bytes::deserialize(deserializer)?;
		BoundedVec::try_from(bytes.0)
			.map_err(|_| serde::de::Error::custom("decoded bytes exceed bounded limit"))
	}
}

impl<const N: usize> HexCodec for [u8; N] {
	type Error = hex::FromHexError;

	fn to_hex(&self) -> String {
		format!("0x{}", hex::encode(self))
	}

	fn from_hex(value: String) -> Result<Self, Self::Error> {
		let data = hex::decode(value.trim_start_matches("0x"))?;
		data.try_into().map_err(|_| hex::FromHexError::InvalidStringLength)
	}
}

impl HexCodec for Vec<u8> {
	type Error = hex::FromHexError;

	fn to_hex(&self) -> String {
		format!("0x{}", hex::encode(self))
	}

	fn from_hex(value: String) -> Result<Self, Self::Error> {
		hex::decode(value.trim_start_matches("0x"))
	}
}

/// `serde` adapter that routes JSON serialization of any [`HexCodec`] value
/// through its `0x`-prefixed hex string form.
mod hex_serde {
	use super::HexCodec;
	use alloc::{format, string::String};
	use serde::{Deserialize, Deserializer, Serializer};

	/// Serializes a [`HexCodec`] value as its `0x`-prefixed hex string.
	pub(super) fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
		T: HexCodec,
	{
		serializer.serialize_str(&value.to_hex())
	}

	/// Deserializes a `0x`-prefixed hex string into a [`HexCodec`] value,
	/// surfacing the codec's own error inside a `serde::de::Error::custom`.
	pub(super) fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
	where
		D: Deserializer<'de>,
		T: HexCodec,
		<T as HexCodec>::Error: core::fmt::Debug,
	{
		let value = String::deserialize(deserializer)?;
		T::from_hex(value).map_err(|error| serde::de::Error::custom(format!("{:?}", error)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rejects_scale_encoded_vec_above_limit() {
		// Arrange
		let encoded = vec![1u8, 2, 3].encode();
		// Act
		let decoded = BoundedBytes::<2>::decode(&mut &encoded[..]);
		// Assert
		assert!(decoded.is_err());
	}

	#[test]
	fn round_trips_valid_bounded_bytes_and_preserves_inner_bytes() {
		// Arrange
		let original = BoundedBytes::<4>::try_from(vec![1u8, 2, 3]).unwrap();
		let encoded = original.encode();
		// Act
		let decoded = BoundedBytes::<4>::decode(&mut &encoded[..]).unwrap();
		// Assert
		assert_eq!(decoded, original);
		assert_eq!(&decoded.0[..], &[1u8, 2, 3]);
	}
}
