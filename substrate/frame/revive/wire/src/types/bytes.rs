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

trait HexCodec: Sized {
	type Error;

	fn to_hex(&self) -> String;
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

mod hex_serde {
	use super::HexCodec;
	use alloc::{format, string::String};
	use serde::{Deserialize, Deserializer, Serializer};

	pub(super) fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
		T: HexCodec,
	{
		serializer.serialize_str(&value.to_hex())
	}

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
