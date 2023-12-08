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

//! A set of well-known keys used for testing.

pub use sp_core::bandersnatch;
use sp_core::{
	bandersnatch::{Pair, Public, Signature},
	crypto::UncheckedFrom,
	hex2array, ByteArray, Pair as PairT,
};

/// Set of test accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumIter)]
pub enum Keyring {
	Alice,
	Bob,
	Charlie,
	Dave,
	Eve,
	Ferdie,
	One,
	Two,
}

const PUBLIC_RAW_LEN: usize = <Public as ByteArray>::LEN;

impl Keyring {
	pub fn from_public(who: &Public) -> Option<Keyring> {
		Self::iter().find(|&k| &Public::from(k) == who)
	}

	pub fn from_raw_public(who: [u8; PUBLIC_RAW_LEN]) -> Option<Keyring> {
		Self::from_public(&Public::unchecked_from(who))
	}

	pub fn to_raw_public(self) -> [u8; PUBLIC_RAW_LEN] {
		*Public::from(self).as_ref()
	}

	pub fn to_raw_public_vec(self) -> Vec<u8> {
		Public::from(self).to_raw_vec()
	}

	pub fn sign(self, msg: &[u8]) -> Signature {
		Pair::from(self).sign(msg)
	}

	pub fn pair(self) -> Pair {
		Pair::from_string(&format!("//{}", <&'static str>::from(self)), None)
			.expect("static values are known good; qed")
	}

	/// Returns an iterator over all test accounts.
	pub fn iter() -> impl Iterator<Item = Keyring> {
		<Self as strum::IntoEnumIterator>::iter()
	}

	pub fn public(self) -> Public {
		Public::from(self)
	}

	pub fn to_seed(self) -> String {
		format!("//{}", self)
	}

	/// Create a crypto `Pair` from a numeric value.
	pub fn numeric(idx: usize) -> Pair {
		Pair::from_string(&format!("//{}", idx), None).expect("numeric values are known good; qed")
	}
}

impl From<Keyring> for &'static str {
	fn from(k: Keyring) -> Self {
		match k {
			Keyring::Alice => "Alice",
			Keyring::Bob => "Bob",
			Keyring::Charlie => "Charlie",
			Keyring::Dave => "Dave",
			Keyring::Eve => "Eve",
			Keyring::Ferdie => "Ferdie",
			Keyring::One => "One",
			Keyring::Two => "Two",
		}
	}
}

#[derive(Debug)]
pub struct ParseKeyringError;

impl std::fmt::Display for ParseKeyringError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "ParseKeyringError")
	}
}

impl std::str::FromStr for Keyring {
	type Err = ParseKeyringError;

	fn from_str(s: &str) -> Result<Self, <Self as std::str::FromStr>::Err> {
		match s {
			"Alice" => Ok(Keyring::Alice),
			"Bob" => Ok(Keyring::Bob),
			"Charlie" => Ok(Keyring::Charlie),
			"Dave" => Ok(Keyring::Dave),
			"Eve" => Ok(Keyring::Eve),
			"Ferdie" => Ok(Keyring::Ferdie),
			"One" => Ok(Keyring::One),
			"Two" => Ok(Keyring::Two),
			_ => Err(ParseKeyringError),
		}
	}
}

impl From<Keyring> for Public {
	fn from(k: Keyring) -> Self {
		Public::unchecked_from(<[u8; PUBLIC_RAW_LEN]>::from(k))
	}
}

impl From<Keyring> for Pair {
	fn from(k: Keyring) -> Self {
		k.pair()
	}
}

impl From<Keyring> for [u8; PUBLIC_RAW_LEN] {
	fn from(k: Keyring) -> Self {
		match k {
			Keyring::Alice =>
				hex2array!("9c8af77d3a4e3f6f076853922985b9e6724fc9675329087f47aff1ceaaae772180"),
			Keyring::Bob =>
				hex2array!("1abfbb76dc8374a1a6d93d59a5c81f07c18835f4681a6258aa0f514d363bff4780"),
			Keyring::Charlie =>
				hex2array!("0f4a9990aca3d39a7cd8bf187e2e81a9ea6f9cedb2db405f2fffff384c5dd02680"),
			Keyring::Dave =>
				hex2array!("bd7a87d4dfa89926a408b5acbed554ae3b053fa3532531053295cbabf07d337000"),
			Keyring::Eve =>
				hex2array!("f992d5b8eac8fc004d521bee6edc1174cfa7fae3a1baec8262511ee351f9f85e00"),
			Keyring::Ferdie =>
				hex2array!("1ce2613e89bc5c8e358aad884099cfb576a61176f2f9968cd0d486a04457245180"),
			Keyring::One =>
				hex2array!("a29e03ac273e521274d8e501a6242abd2ab393d7e197221a9113bdf8e2e5b34d00"),
			Keyring::Two =>
				hex2array!("f968d47e819ddb18a9d0f2ebd16501680b1a3f07ee375c6f81310e5f99a04f4d00"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{bandersnatch::Pair, Pair as PairT};

	#[test]
	fn should_work() {
		assert!(Pair::verify(
			&Keyring::Alice.sign(b"I am Alice!"),
			b"I am Alice!",
			&Keyring::Alice.public(),
		));
		assert!(!Pair::verify(
			&Keyring::Alice.sign(b"I am Alice!"),
			b"I am Bob!",
			&Keyring::Alice.public(),
		));
		assert!(!Pair::verify(
			&Keyring::Alice.sign(b"I am Alice!"),
			b"I am Alice!",
			&Keyring::Bob.public(),
		));
	}
	#[test]
	fn verify_static_public_keys() {
		assert!(Keyring::iter()
			.all(|k| { k.pair().public().as_ref() == <[u8; PUBLIC_RAW_LEN]>::from(k) }));
	}
}
