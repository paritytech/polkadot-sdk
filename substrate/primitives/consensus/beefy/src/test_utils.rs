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

#[cfg(feature = "bls-experimental")]
use crate::ecdsa_bls_crypto;
use crate::{
	ecdsa_crypto, AuthorityIdBound, BeefySignatureHasher, Commitment, EquivocationProof, Payload,
	ValidatorSetId, VoteMessage,
};
use sp_application_crypto::{AppCrypto, AppPair, RuntimeAppPublic, Wraps};
use sp_core::{ecdsa, Pair};
use sp_runtime::traits::Hash;

use codec::Encode;
use std::{collections::HashMap, marker::PhantomData};
use strum::IntoEnumIterator;

/// Set of test accounts using [`crate::ecdsa_crypto`] types.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumIter)]
pub enum Keyring<AuthorityId> {
	Alice,
	Bob,
	Charlie,
	Dave,
	Eve,
	Ferdie,
	One,
	Two,
	_Marker(PhantomData<AuthorityId>),
}

/// Trait representing BEEFY specific generation and signing behavior of authority id
///
/// Accepts custom hashing fn for the message and custom convertor fn for the signer.
pub trait BeefySignerAuthority<MsgHash: Hash>: AppPair {
	/// Generate and return signature for `message` using custom hashing `MsgHash`
	fn sign_with_hasher(&self, message: &[u8]) -> <Self as AppCrypto>::Signature;
}

impl<MsgHash> BeefySignerAuthority<MsgHash> for <ecdsa_crypto::AuthorityId as AppCrypto>::Pair
where
	MsgHash: Hash,
	<MsgHash as Hash>::Output: Into<[u8; 32]>,
{
	fn sign_with_hasher(&self, message: &[u8]) -> <Self as AppCrypto>::Signature {
		let hashed_message = <MsgHash as Hash>::hash(message).into();
		self.as_inner_ref().sign_prehashed(&hashed_message).into()
	}
}

#[cfg(feature = "bls-experimental")]
impl<MsgHash> BeefySignerAuthority<MsgHash> for <ecdsa_bls_crypto::AuthorityId as AppCrypto>::Pair
where
	MsgHash: Hash,
	<MsgHash as Hash>::Output: Into<[u8; 32]>,
{
	fn sign_with_hasher(&self, message: &[u8]) -> <Self as AppCrypto>::Signature {
		self.as_inner_ref().sign_with_hasher::<MsgHash>(&message).into()
	}
}

/// Implement Keyring functionalities generically over AuthorityId
impl<AuthorityId> Keyring<AuthorityId>
where
	AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<BeefySignatureHasher>,
	<AuthorityId as RuntimeAppPublic>::Signature:
		Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
{
	/// Sign `msg`.
	pub fn sign(&self, msg: &[u8]) -> <AuthorityId as RuntimeAppPublic>::Signature {
		let key_pair: <AuthorityId as AppCrypto>::Pair = self.pair();
		key_pair.sign_with_hasher(&msg).into()
	}

	/// Return key pair.
	pub fn pair(&self) -> <AuthorityId as AppCrypto>::Pair {
		<AuthorityId as AppCrypto>::Pair::from_string(self.to_seed().as_str(), None)
			.unwrap()
			.into()
	}

	/// Return public key.
	pub fn public(&self) -> AuthorityId {
		self.pair().public().into()
	}

	/// Return seed string.
	pub fn to_seed(&self) -> String {
		format!("//{}", self)
	}

	/// Get Keyring from public key.
	pub fn from_public(who: &AuthorityId) -> Option<Keyring<AuthorityId>> {
		Self::iter().find(|k| k.public() == *who)
	}
}

lazy_static::lazy_static! {
	static ref PRIVATE_KEYS: HashMap<Keyring<ecdsa_crypto::AuthorityId>, ecdsa_crypto::Pair> =
		Keyring::iter().map(|i| (i.clone(), i.pair())).collect();
	static ref PUBLIC_KEYS: HashMap<Keyring<ecdsa_crypto::AuthorityId>, ecdsa_crypto::Public> =
		PRIVATE_KEYS.iter().map(|(name, pair)| (name.clone(), sp_application_crypto::Pair::public(pair))).collect();
}

impl From<Keyring<ecdsa_crypto::AuthorityId>> for ecdsa_crypto::Pair {
	fn from(k: Keyring<ecdsa_crypto::AuthorityId>) -> Self {
		k.pair()
	}
}

impl From<Keyring<ecdsa_crypto::AuthorityId>> for ecdsa::Pair {
	fn from(k: Keyring<ecdsa_crypto::AuthorityId>) -> Self {
		k.pair().into()
	}
}

impl From<Keyring<ecdsa_crypto::AuthorityId>> for ecdsa_crypto::Public {
	fn from(k: Keyring<ecdsa_crypto::AuthorityId>) -> Self {
		(*PUBLIC_KEYS).get(&k).cloned().unwrap()
	}
}

/// Create a new `EquivocationProof` based on given arguments.
pub fn generate_equivocation_proof(
	vote1: (u64, Payload, ValidatorSetId, &Keyring<ecdsa_crypto::AuthorityId>),
	vote2: (u64, Payload, ValidatorSetId, &Keyring<ecdsa_crypto::AuthorityId>),
) -> EquivocationProof<u64, ecdsa_crypto::Public, ecdsa_crypto::Signature> {
	let signed_vote = |block_number: u64,
	                   payload: Payload,
	                   validator_set_id: ValidatorSetId,
	                   keyring: &Keyring<ecdsa_crypto::AuthorityId>| {
		let commitment = Commitment { validator_set_id, block_number, payload };
		let signature = keyring.sign(&commitment.encode());
		VoteMessage { commitment, id: keyring.public(), signature }
	};
	let first = signed_vote(vote1.0, vote1.1, vote1.2, vote1.3);
	let second = signed_vote(vote2.0, vote2.1, vote2.2, vote2.3);
	EquivocationProof { first, second }
}
