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
use serde::{Deserialize, Deserializer, Serializer};

pub trait HexCodec: Sized {
	type Error;
	fn to_hex(&self) -> String;
	fn from_hex(s: String) -> Result<Self, Self::Error>;
}

macro_rules! impl_hex_codec {
    ($($t:ty),*) => {
        $(
            impl HexCodec for $t {
                type Error = core::num::ParseIntError;
                fn to_hex(&self) -> String {
                    format!("0x{:x}", self)
                }
                fn from_hex(s: String) -> Result<Self, Self::Error> {
                    <$t>::from_str_radix(s.trim_start_matches("0x"), 16)
                }
            }
        )*
    };
}

impl_hex_codec!(u8, u32);

impl<const T: usize> HexCodec for [u8; T] {
	type Error = hex::FromHexError;
	fn to_hex(&self) -> String {
		format!("0x{}", hex::encode(self))
	}
	fn from_hex(s: String) -> Result<Self, Self::Error> {
		let data = hex::decode(s.trim_start_matches("0x"))?;
		data.try_into().map_err(|_| hex::FromHexError::InvalidStringLength)
	}
}

impl HexCodec for Vec<u8> {
	type Error = hex::FromHexError;
	fn to_hex(&self) -> String {
		format!("0x{}", hex::encode(self))
	}
	fn from_hex(s: String) -> Result<Self, Self::Error> {
		hex::decode(s.trim_start_matches("0x"))
	}
}

pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
	T: HexCodec,
{
	let s = value.to_hex();
	serializer.serialize_str(&s)
}

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: Deserializer<'de>,
	T: HexCodec,
	<T as HexCodec>::Error: core::fmt::Debug,
{
	let s = String::deserialize(deserializer)?;
	let value = T::from_hex(s).map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?;
	Ok(value)
}
