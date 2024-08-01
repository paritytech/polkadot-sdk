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

pub use sp_core::ed25519;
#[cfg(feature = "std")]
use sp_core::ed25519::Signature;
use sp_core::{
	ed25519::{Pair, Public},
	hex2array, ByteArray, Pair as PairT, H256,
};
use sp_runtime::AccountId32;

extern crate alloc;
use alloc::{format, string::String, vec::Vec};

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
		sp_runtime::MultiSigner::Ed25519(x.into())
	}
}

impl From<Keyring> for Public {
	fn from(k: Keyring) -> Self {
		Public::from_raw(k.into())
	}
}

impl From<Keyring> for AccountId32 {
	fn from(k: Keyring) -> Self {
		k.to_account_id()
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
				hex2array!("88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee"),
			Keyring::Bob =>
				hex2array!("d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae69"),
			Keyring::Charlie =>
				hex2array!("439660b36c6c03afafca027b910b4fecf99801834c62a5e6006f27d978de234f"),
			Keyring::Dave =>
				hex2array!("5e639b43e0052c47447dac87d6fd2b6ec50bdd4d0f614e4299c665249bbd09d9"),
			Keyring::Eve =>
				hex2array!("1dfe3e22cc0d45c70779c1095f7489a8ef3cf52d62fbd8c2fa38c9f1723502b5"),
			Keyring::Ferdie =>
				hex2array!("568cb4a574c6d178feb39c27dfc8b3f789e5f5423e19c71633c748b9acf086b5"),
			Keyring::AliceStash =>
				hex2array!("451781cd0c5504504f69ceec484cc66e4c22a2b6a9d20fb1a426d91ad074a2a8"),
			Keyring::BobStash =>
				hex2array!("292684abbb28def63807c5f6e84e9e8689769eb37b1ab130d79dbfbf1b9a0d44"),
			Keyring::CharlieStash =>
				hex2array!("dd6a6118b6c11c9c9e5a4f34ed3d545e2c74190f90365c60c230fa82e9423bb9"),
			Keyring::DaveStash =>
				hex2array!("1d0432d75331ab299065bee79cdb1bdc2497c597a3087b4d955c67e3c000c1e2"),
			Keyring::EveStash =>
				hex2array!("c833bdd2e1a7a18acc1c11f8596e2e697bb9b42d6b6051e474091a1d43a294d7"),
			Keyring::FerdieStash =>
				hex2array!("199d749dbf4b8135cb1f3c8fd697a390fc0679881a8a110c1d06375b3b62cd09"),
			Keyring::One =>
				hex2array!("16f97016bbea8f7b45ae6757b49efc1080accc175d8f018f9ba719b60b0815e4"),
			Keyring::Two =>
				hex2array!("5079bcd20fd97d7d2f752c4607012600b401950260a91821f73e692071c82bf5"),
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
	use sp_core::{ed25519::Pair, Pair as PairT};

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
}
