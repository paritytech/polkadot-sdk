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
use sp_core::{
	ed25519::{Pair, Public, Signature},
	ByteArray, Pair as PairT, H256,
};
use sp_runtime::AccountId32;

/// Set of test accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumIter)]
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

fn hex2arr(h: &str) -> [u8; 32] {
	array_bytes::hex2array_unchecked(h)
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
		self.pair().public()
	}

	pub fn to_seed(self) -> String {
		format!("//{}", self)
	}

	fn public_bytes(&self) -> [u8; 32] {
		match *self {
			Keyring::Alice =>
				hex2arr("88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee"),
			Keyring::Bob =>
				hex2arr("d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae69"),
			Keyring::Charlie =>
				hex2arr("439660b36c6c03afafca027b910b4fecf99801834c62a5e6006f27d978de234f"),
			Keyring::Dave =>
				hex2arr("5e639b43e0052c47447dac87d6fd2b6ec50bdd4d0f614e4299c665249bbd09d9"),
			Keyring::Eve =>
				hex2arr("1dfe3e22cc0d45c70779c1095f7489a8ef3cf52d62fbd8c2fa38c9f1723502b5"),
			Keyring::Ferdie =>
				hex2arr("568cb4a574c6d178feb39c27dfc8b3f789e5f5423e19c71633c748b9acf086b5"),
			Keyring::AliceStash =>
				hex2arr("451781cd0c5504504f69ceec484cc66e4c22a2b6a9d20fb1a426d91ad074a2a8"),
			Keyring::BobStash =>
				hex2arr("292684abbb28def63807c5f6e84e9e8689769eb37b1ab130d79dbfbf1b9a0d44"),
			Keyring::CharlieStash =>
				hex2arr("dd6a6118b6c11c9c9e5a4f34ed3d545e2c74190f90365c60c230fa82e9423bb9"),
			Keyring::DaveStash =>
				hex2arr("1d0432d75331ab299065bee79cdb1bdc2497c597a3087b4d955c67e3c000c1e2"),
			Keyring::EveStash =>
				hex2arr("c833bdd2e1a7a18acc1c11f8596e2e697bb9b42d6b6051e474091a1d43a294d7"),
			Keyring::FerdieStash =>
				hex2arr("199d749dbf4b8135cb1f3c8fd697a390fc0679881a8a110c1d06375b3b62cd09"),
			Keyring::One =>
				hex2arr("16f97016bbea8f7b45ae6757b49efc1080accc175d8f018f9ba719b60b0815e4"),
			Keyring::Two =>
				hex2arr("5079bcd20fd97d7d2f752c4607012600b401950260a91821f73e692071c82bf5"),
		}
	}
	// fn public_bytes(&self) -> &'static [u8; 32] {
	// 	match *self {
	// 		Keyring::Alice => &[
	// 			0x88, 0xdc, 0x34, 0x17, 0xd5, 0x05, 0x8e, 0xc4, 0xb4, 0x50, 0x3e, 0x0c, 0x12, 0xea,
	// 			0x1a, 0x0a, 0x89, 0xbe, 0x20, 0x0f, 0xe9, 0x89, 0x22, 0x42, 0x3d, 0x43, 0x34, 0x01,
	// 			0x4f, 0xa6, 0xb0, 0xee,
	// 		],
	// 		Keyring::Bob => &[
	// 			0xd1, 0x7c, 0x2d, 0x78, 0x23, 0xeb, 0xf2, 0x60, 0xfd, 0x13, 0x8f, 0x2d, 0x7e, 0x27,
	// 			0xd1, 0x14, 0xc0, 0x14, 0x5d, 0x96, 0x8b, 0x5f, 0xf5, 0x00, 0x61, 0x25, 0xf2, 0x41,
	// 			0x4f, 0xad, 0xae, 0x69,
	// 		],
	// 		Keyring::Charlie => &[
	// 			0x43, 0x96, 0x60, 0xb3, 0x6c, 0x6c, 0x03, 0xaf, 0xaf, 0xca, 0x02, 0x7b, 0x91, 0x0b,
	// 			0x4f, 0xec, 0xf9, 0x98, 0x01, 0x83, 0x4c, 0x62, 0xa5, 0xe6, 0x00, 0x6f, 0x27, 0xd9,
	// 			0x78, 0xde, 0x23, 0x4f,
	// 		],
	// 		Keyring::Dave => &[
	// 			0x5e, 0x63, 0x9b, 0x43, 0xe0, 0x05, 0x2c, 0x47, 0x44, 0x7d, 0xac, 0x87, 0xd6, 0xfd,
	// 			0x2b, 0x6e, 0xc5, 0x0b, 0xdd, 0x4d, 0x0f, 0x61, 0x4e, 0x42, 0x99, 0xc6, 0x65, 0x24,
	// 			0x9b, 0xbd, 0x09, 0xd9,
	// 		],
	// 		Keyring::Eve => &[
	// 			0x1d, 0xfe, 0x3e, 0x22, 0xcc, 0x0d, 0x45, 0xc7, 0x07, 0x79, 0xc1, 0x09, 0x5f, 0x74,
	// 			0x89, 0xa8, 0xef, 0x3c, 0xf5, 0x2d, 0x62, 0xfb, 0xd8, 0xc2, 0xfa, 0x38, 0xc9, 0xf1,
	// 			0x72, 0x35, 0x02, 0xb5,
	// 		],
	// 		Keyring::Ferdie => &[
	// 			0x56, 0x8c, 0xb4, 0xa5, 0x74, 0xc6, 0xd1, 0x78, 0xfe, 0xb3, 0x9c, 0x27, 0xdf, 0xc8,
	// 			0xb3, 0xf7, 0x89, 0xe5, 0xf5, 0x42, 0x3e, 0x19, 0xc7, 0x16, 0x33, 0xc7, 0x48, 0xb9,
	// 			0xac, 0xf0, 0x86, 0xb5,
	// 		],
	// 		Keyring::AliceStash => &[
	// 			0x45, 0x17, 0x81, 0xcd, 0x0c, 0x55, 0x04, 0x50, 0x4f, 0x69, 0xce, 0xec, 0x48, 0x4c,
	// 			0xc6, 0x6e, 0x4c, 0x22, 0xa2, 0xb6, 0xa9, 0xd2, 0x0f, 0xb1, 0xa4, 0x26, 0xd9, 0x1a,
	// 			0xd0, 0x74, 0xa2, 0xa8,
	// 		],
	// 		Keyring::BobStash => &[
	// 			0x29, 0x26, 0x84, 0xab, 0xbb, 0x28, 0xde, 0xf6, 0x38, 0x07, 0xc5, 0xf6, 0xe8, 0x4e,
	// 			0x9e, 0x86, 0x89, 0x76, 0x9e, 0xb3, 0x7b, 0x1a, 0xb1, 0x30, 0xd7, 0x9d, 0xbf, 0xbf,
	// 			0x1b, 0x9a, 0x0d, 0x44,
	// 		],
	// 		Keyring::CharlieStash => &[
	// 			0xdd, 0x6a, 0x61, 0x18, 0xb6, 0xc1, 0x1c, 0x9c, 0x9e, 0x5a, 0x4f, 0x34, 0xed, 0x3d,
	// 			0x54, 0x5e, 0x2c, 0x74, 0x19, 0x0f, 0x90, 0x36, 0x5c, 0x60, 0xc2, 0x30, 0xfa, 0x82,
	// 			0xe9, 0x42, 0x3b, 0xb9,
	// 		],
	// 		Keyring::DaveStash => &[
	// 			0x1d, 0x04, 0x32, 0xd7, 0x53, 0x31, 0xab, 0x29, 0x90, 0x65, 0xbe, 0xe7, 0x9c, 0xdb,
	// 			0x1b, 0xdc, 0x24, 0x97, 0xc5, 0x97, 0xa3, 0x08, 0x7b, 0x4d, 0x95, 0x5c, 0x67, 0xe3,
	// 			0xc0, 0x00, 0xc1, 0xe2,
	// 		],
	// 		Keyring::EveStash => &[
	// 			0xc8, 0x33, 0xbd, 0xd2, 0xe1, 0xa7, 0xa1, 0x8a, 0xcc, 0x1c, 0x11, 0xf8, 0x59, 0x6e,
	// 			0x2e, 0x69, 0x7b, 0xb9, 0xb4, 0x2d, 0x6b, 0x60, 0x51, 0xe4, 0x74, 0x09, 0x1a, 0x1d,
	// 			0x43, 0xa2, 0x94, 0xd7,
	// 		],
	// 		Keyring::FerdieStash => &[
	// 			0x19, 0x9d, 0x74, 0x9d, 0xbf, 0x4b, 0x81, 0x35, 0xcb, 0x1f, 0x3c, 0x8f, 0xd6, 0x97,
	// 			0xa3, 0x90, 0xfc, 0x06, 0x79, 0x88, 0x1a, 0x8a, 0x11, 0x0c, 0x1d, 0x06, 0x37, 0x5b,
	// 			0x3b, 0x62, 0xcd, 0x09,
	// 		],
	// 		Keyring::One => &[
	// 			0x16, 0xf9, 0x70, 0x16, 0xbb, 0xea, 0x8f, 0x7b, 0x45, 0xae, 0x67, 0x57, 0xb4, 0x9e,
	// 			0xfc, 0x10, 0x80, 0xac, 0xcc, 0x17, 0x5d, 0x8f, 0x01, 0x8f, 0x9b, 0xa7, 0x19, 0xb6,
	// 			0x0b, 0x08, 0x15, 0xe4,
	// 		],
	// 		Keyring::Two => &[
	// 			0x50, 0x79, 0xbc, 0xd2, 0x0f, 0xd9, 0x7d, 0x7d, 0x2f, 0x75, 0x2c, 0x46, 0x07, 0x01,
	// 			0x26, 0x00, 0xb4, 0x01, 0x95, 0x02, 0x60, 0xa9, 0x18, 0x21, 0xf7, 0x3e, 0x69, 0x20,
	// 			0x71, 0xc8, 0x2b, 0xf5,
	// 		],
	// 		// Keyring::Alice =>
	// 		// 	&hex!("88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee"),
	// 		// Keyring::Bob =>
	// 		// 	&hex!("d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae69"),
	// 		// Keyring::Charlie =>
	// 		// 	&hex!("439660b36c6c03afafca027b910b4fecf99801834c62a5e6006f27d978de234f"),
	// 		// Keyring::Dave =>
	// 		// 	&hex!("5e639b43e0052c47447dac87d6fd2b6ec50bdd4d0f614e4299c665249bbd09d9"),
	// 		// Keyring::Eve =>
	// 		// 	&hex!("1dfe3e22cc0d45c70779c1095f7489a8ef3cf52d62fbd8c2fa38c9f1723502b5"),
	// 		// Keyring::Ferdie =>
	// 		// 	&hex!("568cb4a574c6d178feb39c27dfc8b3f789e5f5423e19c71633c748b9acf086b5"),
	// 		// Keyring::AliceStash =>
	// 		// 	&hex!("451781cd0c5504504f69ceec484cc66e4c22a2b6a9d20fb1a426d91ad074a2a8"),
	// 		// Keyring::BobStash =>
	// 		// 	&hex!("292684abbb28def63807c5f6e84e9e8689769eb37b1ab130d79dbfbf1b9a0d44"),
	// 		// Keyring::CharlieStash =>
	// 		// 	&hex!("dd6a6118b6c11c9c9e5a4f34ed3d545e2c74190f90365c60c230fa82e9423bb9"),
	// 		// Keyring::DaveStash =>
	// 		// 	&hex!("1d0432d75331ab299065bee79cdb1bdc2497c597a3087b4d955c67e3c000c1e2"),
	// 		// Keyring::EveStash =>
	// 		// 	&hex!("c833bdd2e1a7a18acc1c11f8596e2e697bb9b42d6b6051e474091a1d43a294d7"),
	// 		// Keyring::FerdieStash =>
	// 		// 	&hex!("199d749dbf4b8135cb1f3c8fd697a390fc0679881a8a110c1d06375b3b62cd09"),
	// 		// Keyring::One =>
	// 		// 	&hex!("16f97016bbea8f7b45ae6757b49efc1080accc175d8f018f9ba719b60b0815e4"),
	// 		// Keyring::Two =>
	// 		// 	&hex!("5079bcd20fd97d7d2f752c4607012600b401950260a91821f73e692071c82bf5"),
	// 	}
	// }
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
		Public::from_raw(k.public_bytes())
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
		k.public_bytes()
	}
}

impl From<Keyring> for H256 {
	fn from(k: Keyring) -> Self {
		k.public_bytes().into()
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
		assert!(Keyring::iter().all(|k| { k.pair().public().as_ref() == k.public_bytes() }));
		// little helper to print out public keys hex string
		// use array_bytes::Hex;
		// Keyring::iter().map(|i| (i, i.pair())).for_each(|(name, pair)| {
		// 	let public = pair.public();
		// 	let bytes: &[u8; 32] = public.as_ref();
		// 	println!("Keyring::{}: {:?}", name, bytes.hex(""));
		// });
	}
}
