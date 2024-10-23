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
//! Define Byte wrapper types for encoding and decoding hex strings
use alloc::{vec, vec::Vec};
use codec::{Decode, Encode};
use core::{
	fmt::{Debug, Display, Formatter, Result as FmtResult},
	str::FromStr,
};
use hex_serde::HexCodec;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

mod hex_serde {
	#[cfg(not(feature = "std"))]
	use alloc::{format, string::String, vec::Vec};
	use serde::{Deserialize, Deserializer, Serializer};

	pub trait HexCodec: Sized {
		type Error;
		fn to_hex(&self) -> String;
		fn from_hex(s: String) -> Result<Self, Self::Error>;
	}

	impl HexCodec for u8 {
		type Error = core::num::ParseIntError;
		fn to_hex(&self) -> String {
			format!("0x{:x}", self)
		}
		fn from_hex(s: String) -> Result<Self, Self::Error> {
			u8::from_str_radix(s.trim_start_matches("0x"), 16)
		}
	}

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
}

impl FromStr for Bytes {
	type Err = hex::FromHexError;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let data = hex::decode(s.trim_start_matches("0x"))?;
		Ok(Bytes(data))
	}
}

macro_rules! impl_hex {
    ($type:ident, $inner:ty, $default:expr) => {
        #[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Clone, Serialize, Deserialize)]
        #[doc = concat!("`", stringify!($inner), "`", " wrapper type for encoding and decoding hex strings")]
        pub struct $type(#[serde(with = "hex_serde")] pub $inner);

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
                write!(f, concat!(stringify!($type), "({})"), self.0.to_hex())
            }
        }

        impl Display for $type {
            fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
                write!(f, "{}", self.0.to_hex())
            }
        }
    };
}

impl_hex!(Byte, u8, 0u8);
impl_hex!(Bytes, Vec<u8>, vec![]);
impl_hex!(Bytes8, [u8; 8], [0u8; 8]);
impl_hex!(Bytes256, [u8; 256], [0u8; 256]);

#[test]
fn serialize_works() {
	let a = Byte(42);
	let s = serde_json::to_string(&a).unwrap();
	assert_eq!(s, "\"0x2a\"");
	let b = serde_json::from_str::<Byte>(&s).unwrap();
	assert_eq!(a, b);

	let a = Bytes(b"bello world".to_vec());
	let s = serde_json::to_string(&a).unwrap();
	assert_eq!(s, "\"0x62656c6c6f20776f726c64\"");
	let b = serde_json::from_str::<Bytes>(&s).unwrap();
	assert_eq!(a, b);

	let a = Bytes256([42u8; 256]);
	let s = serde_json::to_string(&a).unwrap();
	let b = serde_json::from_str::<Bytes256>(&s).unwrap();
	assert_eq!(a, b);
}
