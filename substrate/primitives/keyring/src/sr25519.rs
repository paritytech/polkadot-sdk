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

//! Support code for the runtime. A set of test accounts.

pub use sp_core::sr25519;

use crate::ParseKeyringError;
#[cfg(feature = "std")]
use sp_core::sr25519::Signature;
use sp_core::{
	hex2array,
	sr25519::{Pair, Public},
	ByteArray, Pair as PairT, H256,
};
use sp_runtime::AccountId32;

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
	AliceStash,
	BobStash,
	CharlieStash,
	DaveStash,
	EveStash,
	FerdieStash,
	One,
	Two,
}

impl Keyring {
	pub fn from_public(who: &Public) -> Option<Keyring> {
		Self::iter().find(|&k| &Public::from(k) == who)
	}

	pub fn from_account_id(who: &AccountId32) -> Option<Keyring> {
		Self::iter().find(|&k| &k.to_account_id() == who)
	}

	pub fn from_raw_public(who: [u8; 32]) -> Option<Keyring> {
		Self::from_public(&Public::from_raw(who))
	}

	pub fn to_raw_public(self) -> [u8; 32] {
		*Public::from(self).as_array_ref()
	}

	pub fn from_h256_public(who: H256) -> Option<Keyring> {
		Self::from_public(&Public::from_raw(who.into()))
	}

	pub fn to_h256_public(self) -> H256 {
		Public::from(self).as_array_ref().into()
	}

	pub fn to_raw_public_vec(self) -> Vec<u8> {
		Public::from(self).to_raw_vec()
	}

	pub fn to_account_id(self) -> AccountId32 {
		self.to_raw_public().into()
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

	/// Get account id of a `numeric` account.
	pub fn numeric_id(idx: usize) -> AccountId32 {
		(*Self::numeric(idx).public().as_array_ref()).into()
	}

	pub fn well_known() -> impl Iterator<Item = Keyring> {
		Self::iter().take(12)
	}

	pub fn invulnerable() -> impl Iterator<Item = Keyring> {
		Self::iter().take(6)
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
			Keyring::AliceStash => "Alice//stash",
			Keyring::BobStash => "Bob//stash",
			Keyring::CharlieStash => "Charlie//stash",
			Keyring::DaveStash => "Dave//stash",
			Keyring::EveStash => "Eve//stash",
			Keyring::FerdieStash => "Ferdie//stash",
			Keyring::One => "One",
			Keyring::Two => "Two",
		}
	}
}

impl From<Keyring> for sp_runtime::MultiSigner {
	fn from(x: Keyring) -> Self {
		sp_runtime::MultiSigner::Sr25519(x.into())
	}
}

impl FromStr for Keyring {
	type Err = ParseKeyringError;

	fn from_str(s: &str) -> Result<Self, <Self as FromStr>::Err> {
		match s {
			"Alice" | "alice" => Ok(Keyring::Alice),
			"Bob" | "bob" => Ok(Keyring::Bob),
			"Charlie" | "charlie" => Ok(Keyring::Charlie),
			"Dave" | "dave" => Ok(Keyring::Dave),
			"Eve" | "eve" => Ok(Keyring::Eve),
			"Ferdie" | "ferdie" => Ok(Keyring::Ferdie),
			"Alice//stash" | "alice//stash" => Ok(Keyring::AliceStash),
			"Bob//stash" | "bob//stash" => Ok(Keyring::BobStash),
			"Charlie//stash" | "charlie//stash" => Ok(Keyring::CharlieStash),
			"Dave//stash" | "dave//stash" => Ok(Keyring::DaveStash),
			"Eve//stash" | "eve//stash" => Ok(Keyring::EveStash),
			"Ferdie//stash" | "ferdie//stash" => Ok(Keyring::FerdieStash),
			"One" | "one" => Ok(Keyring::One),
			"Two" | "two" => Ok(Keyring::Two),
			_ => Err(ParseKeyringError),
		}
	}
}

impl From<Keyring> for AccountId32 {
	fn from(k: Keyring) -> Self {
		k.to_account_id()
	}
}

impl From<Keyring> for Public {
	fn from(k: Keyring) -> Self {
		Public::from_raw(k.into())
	}
}

impl From<Keyring> for Pair {
	fn from(k: Keyring) -> Self {
		k.pair()
	}
}

impl From<Keyring> for [u8; 32] {
	fn from(k: Keyring) -> Self {
		match k {
			Keyring::Alice =>
				hex2array!("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"),
			Keyring::Bob =>
				hex2array!("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48"),
			Keyring::Charlie =>
				hex2array!("90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22"),
			Keyring::Dave =>
				hex2array!("306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20"),
			Keyring::Eve =>
				hex2array!("e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e"),
			Keyring::Ferdie =>
				hex2array!("1cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c"),
			Keyring::AliceStash =>
				hex2array!("be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f"),
			Keyring::BobStash =>
				hex2array!("fe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e"),
			Keyring::CharlieStash =>
				hex2array!("1e07379407fecc4b89eb7dbd287c2c781cfb1907a96947a3eb18e4f8e7198625"),
			Keyring::DaveStash =>
				hex2array!("e860f1b1c7227f7c22602f53f15af80747814dffd839719731ee3bba6edc126c"),
			Keyring::EveStash =>
				hex2array!("8ac59e11963af19174d0b94d5d78041c233f55d2e19324665bafdfb62925af2d"),
			Keyring::FerdieStash =>
				hex2array!("101191192fc877c24d725b337120fa3edc63d227bbc92705db1e2cb65f56981a"),
			Keyring::One =>
				hex2array!("ac859f8a216eeb1b320b4c76d118da3d7407fa523484d0a980126d3b4d0d220a"),
			Keyring::Two =>
				hex2array!("1254f7017f0b8347ce7ab14f96d818802e7e9e0c0d1b7c9acb3c726b080e7a03"),
		}
	}
}

impl From<Keyring> for H256 {
	fn from(k: Keyring) -> Self {
		k.into()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{sr25519::Pair, Pair as PairT};

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
		assert!(Keyring::iter().all(|k| { k.pair().public().as_ref() == <[u8; 32]>::from(k) }));
	}

	#[test]
	fn verify_well_known() {
		assert_eq!(
			Keyring::well_known().collect::<Vec<Keyring>>(),
			vec![
				Keyring::Alice,
				Keyring::Bob,
				Keyring::Charlie,
				Keyring::Dave,
				Keyring::Eve,
				Keyring::Ferdie,
				Keyring::AliceStash,
				Keyring::BobStash,
				Keyring::CharlieStash,
				Keyring::DaveStash,
				Keyring::EveStash,
				Keyring::FerdieStash
			]
		);
	}

	#[test]
	fn verify_invulnerable() {
		assert_eq!(
			Keyring::invulnerable().collect::<Vec<Keyring>>(),
			vec![
				Keyring::Alice,
				Keyring::Bob,
				Keyring::Charlie,
				Keyring::Dave,
				Keyring::Eve,
				Keyring::Ferdie
			]
		);
	}
}
