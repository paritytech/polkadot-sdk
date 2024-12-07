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

//! Testing utilities.

use crate::{
	codec::{Codec, Decode, Encode, MaxEncodedLen},
	generic::{self, UncheckedExtrinsic},
	scale_info::TypeInfo,
	traits::{self, BlakeTwo256, Dispatchable, OpaqueKeys},
	DispatchResultWithInfo, KeyTypeId,
};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
use sp_core::crypto::{key_types, ByteArray, CryptoType, Dummy};
pub use sp_core::{sr25519, H256};
use std::{cell::RefCell, fmt::Debug};

/// A dummy type which can be used instead of regular cryptographic primitives.
///
/// 1. Wraps a `u64` `AccountId` and is able to `IdentifyAccount`.
/// 2. Can be converted to any `Public` key.
/// 3. Implements `RuntimeAppPublic` so it can be used instead of regular application-specific
///    crypto.
#[derive(
	Default,
	PartialEq,
	Eq,
	Clone,
	Encode,
	Decode,
	Debug,
	Hash,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct UintAuthorityId(pub u64);

impl From<u64> for UintAuthorityId {
	fn from(id: u64) -> Self {
		UintAuthorityId(id)
	}
}

impl From<UintAuthorityId> for u64 {
	fn from(id: UintAuthorityId) -> u64 {
		id.0
	}
}

impl UintAuthorityId {
	/// Convert this authority ID into a public key.
	pub fn to_public_key<T: ByteArray>(&self) -> T {
		let mut bytes = [0u8; 32];
		bytes[0..8].copy_from_slice(&self.0.to_le_bytes());
		T::from_slice(&bytes).unwrap()
	}

	/// Set the list of keys returned by the runtime call for all keys of that type.
	pub fn set_all_keys<T: Into<UintAuthorityId>>(keys: impl IntoIterator<Item = T>) {
		ALL_KEYS.with(|l| *l.borrow_mut() = keys.into_iter().map(Into::into).collect())
	}
}

impl CryptoType for UintAuthorityId {
	type Pair = Dummy;
}

impl AsRef<[u8]> for UintAuthorityId {
	fn as_ref(&self) -> &[u8] {
		// Unsafe, i know, but it's test code and it's just there because it's really convenient to
		// keep `UintAuthorityId` as a u64 under the hood.
		unsafe {
			std::slice::from_raw_parts(
				&self.0 as *const u64 as *const _,
				std::mem::size_of::<u64>(),
			)
		}
	}
}

thread_local! {
	/// A list of all UintAuthorityId keys returned to the runtime.
	static ALL_KEYS: RefCell<Vec<UintAuthorityId>> = RefCell::new(vec![]);
}

impl sp_application_crypto::RuntimeAppPublic for UintAuthorityId {
	const ID: KeyTypeId = key_types::DUMMY;

	type Signature = TestSignature;

	fn all() -> Vec<Self> {
		ALL_KEYS.with(|l| l.borrow().clone())
	}

	fn generate_pair(_: Option<Vec<u8>>) -> Self {
		use rand::RngCore;
		UintAuthorityId(rand::thread_rng().next_u64())
	}

	fn sign<M: AsRef<[u8]>>(&self, msg: &M) -> Option<Self::Signature> {
		Some(TestSignature(self.0, msg.as_ref().to_vec()))
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		traits::Verify::verify(signature, msg.as_ref(), &self.0)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		AsRef::<[u8]>::as_ref(self).to_vec()
	}
}

impl OpaqueKeys for UintAuthorityId {
	type KeyTypeIdProviders = ();

	fn key_ids() -> &'static [KeyTypeId] {
		&[key_types::DUMMY]
	}

	fn get_raw(&self, _: KeyTypeId) -> &[u8] {
		self.as_ref()
	}

	fn get<T: Decode>(&self, _: KeyTypeId) -> Option<T> {
		self.using_encoded(|mut x| T::decode(&mut x)).ok()
	}
}

impl traits::IdentifyAccount for UintAuthorityId {
	type AccountId = u64;

	fn into_account(self) -> Self::AccountId {
		self.0
	}
}

impl traits::Verify for UintAuthorityId {
	type Signer = Self;

	fn verify<L: traits::Lazy<[u8]>>(
		&self,
		_msg: L,
		signer: &<Self::Signer as traits::IdentifyAccount>::AccountId,
	) -> bool {
		self.0 == *signer
	}
}

/// A dummy signature type, to match `UintAuthorityId`.
#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize, Encode, Decode, TypeInfo)]
pub struct TestSignature(pub u64, pub Vec<u8>);

impl traits::Verify for TestSignature {
	type Signer = UintAuthorityId;

	fn verify<L: traits::Lazy<[u8]>>(&self, mut msg: L, signer: &u64) -> bool {
		signer == &self.0 && msg.get() == &self.1[..]
	}
}

/// Digest item
pub type DigestItem = generic::DigestItem;

/// Header Digest
pub type Digest = generic::Digest;

/// Block Header
pub type Header = generic::Header<u64, BlakeTwo256>;

impl Header {
	/// A new header with the given number and default hash for all other fields.
	pub fn new_from_number(number: <Self as traits::Header>::Number) -> Self {
		Self {
			number,
			extrinsics_root: Default::default(),
			state_root: Default::default(),
			parent_hash: Default::default(),
			digest: Default::default(),
		}
	}
}

/// Testing block
#[derive(PartialEq, Eq, Clone, Serialize, Debug, Encode, Decode, TypeInfo)]
pub struct Block<Xt> {
	/// Block header
	pub header: Header,
	/// List of extrinsics
	pub extrinsics: Vec<Xt>,
}

impl<Xt> traits::HeaderProvider for Block<Xt> {
	type HeaderT = Header;
}

impl<
		Xt: 'static
			+ Codec
			+ Sized
			+ Send
			+ Sync
			+ Serialize
			+ Clone
			+ Eq
			+ Debug
			+ traits::ExtrinsicLike,
	> traits::Block for Block<Xt>
{
	type Extrinsic = Xt;
	type Header = Header;
	type Hash = <Header as traits::Header>::Hash;

	fn header(&self) -> &Self::Header {
		&self.header
	}
	fn extrinsics(&self) -> &[Self::Extrinsic] {
		&self.extrinsics[..]
	}
	fn deconstruct(self) -> (Self::Header, Vec<Self::Extrinsic>) {
		(self.header, self.extrinsics)
	}
	fn new(header: Self::Header, extrinsics: Vec<Self::Extrinsic>) -> Self {
		Block { header, extrinsics }
	}
	fn encode_from(header: &Self::Header, extrinsics: &[Self::Extrinsic]) -> Vec<u8> {
		(header, extrinsics).encode()
	}
}

impl<'a, Xt> Deserialize<'a> for Block<Xt>
where
	Block<Xt>: Decode,
{
	fn deserialize<D: Deserializer<'a>>(de: D) -> Result<Self, D::Error> {
		let r = <Vec<u8>>::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| DeError::custom(format!("Invalid value passed into decode: {}", e)))
	}
}

/// Extrinsic type with `u64` accounts and mocked signatures, used in testing.
pub type TestXt<Call, Extra> = UncheckedExtrinsic<u64, Call, (), Extra>;

/// Wrapper over a `u64` that can be used as a `RuntimeCall`.
#[derive(PartialEq, Eq, Debug, Clone, Encode, Decode, TypeInfo)]
pub struct MockCallU64(pub u64);

impl Dispatchable for MockCallU64 {
	type RuntimeOrigin = u64;
	type Config = ();
	type Info = ();
	type PostInfo = ();
	fn dispatch(self, _origin: Self::RuntimeOrigin) -> DispatchResultWithInfo<Self::PostInfo> {
		Ok(())
	}
}

impl From<u64> for MockCallU64 {
	fn from(value: u64) -> Self {
		Self(value)
	}
}
