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

use crate::ParseKeyringError;
#[cfg(feature = "std")]
use sp_core::bandersnatch::Signature;
use sp_core::{
	bandersnatch::{Pair, Public},
	crypto::UncheckedFrom,
	hex2array, ByteArray, Pair as PairT,
};

extern crate alloc;
use alloc::{format, str::FromStr, string::String, vec::Vec};

/// Set of test accounts.
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumIter, Ord, PartialOrd,
)]
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

	#[cfg(feature = "std")]
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

impl FromStr for Keyring {
	type Err = ParseKeyringError;

	fn from_str(s: &str) -> Result<Self, <Self as FromStr>::Err> {
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
			Keyring::Alice => {
				hex2array!("2b62f3cc88815b7b7ebd35cd80fe5f4892f7a55ad1a65003134c38effcefadd8")
			},
			Keyring::Bob => {
				hex2array!("dc8c7a1b7f0519c5dcbb12e1b8f4f760d341239aeb6e0a1efc27ae39c7ed9889")
			},
			Keyring::Charlie => {
				hex2array!("e8524d8b12597bac747e4aae0ef4a042e736adcda2c6aa39e08a5f26874e5f48")
			},
			Keyring::Dave => {
				hex2array!("aa902401c2b90411e3f8db9d84a8eddca9c31f7d96c06ff4bb6e39d3d3b65a0b")
			},
			Keyring::Eve => {
				hex2array!("7ec57310eae4e8cd6e1c3a72e5608610733fbb7d5aaf9d132c1600e1164f9807")
			},
			Keyring::Ferdie => {
				hex2array!("03580b40c0dc6adeff388e1dae0664ee70af24c51aae2490abe64e28d8659020")
			},
			Keyring::One => {
				hex2array!("1a2215572a62d026a60458a6d93907ad70eed2a0d67835482a088ff2726cce25")
			},
			Keyring::Two => {
				hex2array!("1cf31b677feb2dfe1c7368287f1c345a9e6af06625548d2c9114dbccb20628b1")
			},
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
