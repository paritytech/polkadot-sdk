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

#![cfg(feature = "std")]

use core::fmt::Debug;
use sp_runtime::traits::Hash;

#[cfg(feature = "bls-experimental")]
use crate::ecdsa_bls_crypto;
use crate::{
	ecdsa_crypto, AuthorityIdBound, BeefySignatureHasher, Commitment, EquivocationProof, Payload,
	ValidatorSetId, VoteMessage,
};
use codec::Encode;
use sp_application_crypto::{AppCrypto, AppPair, RuntimeAppPublic, Wraps};
#[cfg(feature = "bls-experimental")]
use sp_core::ecdsa_bls377;
use sp_core::{ecdsa, Pair};
use sp_keystore::{Keystore, KeystorePtr};
use std::collections::HashMap;
use strum::IntoEnumIterator;

/// Set of test accounts using [`crate::ecdsa_crypto`] types.
#[allow(missing_docs)]
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

/// Trait representing BEEFY specific generation and signing behavior of authority id
///
/// Accepts custom hashing fn for the message and custom convertor fn for the signer.
pub trait BeefySignerAuthority<MsgHash: Hash>: AppPair {
	/// generate a signature.
	///
	/// Return `true` if signature over `msg` is valid for this id.
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
pub trait GenericKeyring<AuthorityId: AuthorityIdBound>
where
	<AuthorityId as RuntimeAppPublic>::Signature: Send + Sync,
{
	///The key pair type which is used to perform crypto functionalities for the Keyring     
	type KeyPair: AppPair;
	/// Generate key pair in the given store using the provided seed
	fn generate_in_store(
		store: KeystorePtr,
		key_type: sp_application_crypto::KeyTypeId,
		owner: Option<Keyring>,
	) -> AuthorityId;

	/// Sign `msg`.
	fn sign(self, msg: &[u8]) -> <AuthorityId as RuntimeAppPublic>::Signature;

	/// Return key pair.
	fn pair(self) -> Self::KeyPair;

	/// Return public key.
	fn public(self) -> AuthorityId;

	/// Return seed string.
	fn to_seed(self) -> String;

	/// Get Keyring from public key.
	fn from_public(who: &AuthorityId) -> Option<Keyring>;
}

impl<AuthorityId> GenericKeyring<AuthorityId> for Keyring
where
	AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<BeefySignatureHasher>,
	<AuthorityId as RuntimeAppPublic>::Signature:
		Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
{
	type KeyPair = <AuthorityId as AppCrypto>::Pair;
	fn generate_in_store(
		store: KeystorePtr,
		key_type: sp_application_crypto::KeyTypeId,
		owner: Option<Keyring>,
	) -> AuthorityId {
		let optional_seed: Option<String> = owner
			.map(|owner| <Keyring as GenericKeyring<ecdsa_crypto::AuthorityId>>::to_seed(owner));

		match <AuthorityId as AppCrypto>::CRYPTO_ID {
			ecdsa::CRYPTO_ID => AuthorityId::decode(
				&mut Keystore::ecdsa_generate_new(&*store, key_type, optional_seed.as_deref())
					.ok()
					.unwrap()
					.as_ref(),
			)
			.unwrap(),

			#[cfg(feature = "bls-experimental")]
			ecdsa_bls377::CRYPTO_ID => {
				let pk = Keystore::ecdsa_bls377_generate_new(
					&*store,
					key_type,
					optional_seed.as_deref(),
				)
				.ok()
				.unwrap();
				let decoded_pk = AuthorityId::decode(&mut pk.as_ref()).unwrap();
				println!(
					"Seed: {:?}, before decode: {:?}, after decode: {:?}",
					optional_seed, pk, decoded_pk
				);
				decoded_pk
			},

			_ => panic!("Requested CRYPTO_ID is not supported by the BEEFY Keyring"),
		}
	}
	/// Sign `msg`.
	fn sign(self, msg: &[u8]) -> <AuthorityId as RuntimeAppPublic>::Signature {
		let key_pair: Self::KeyPair = <Keyring as GenericKeyring<AuthorityId>>::pair(self);
		key_pair.sign_with_hasher(&msg).into()
	}

	/// Return key pair.
	fn pair(self) -> Self::KeyPair {
		Self::KeyPair::from_string(
			<Keyring as GenericKeyring<AuthorityId>>::to_seed(self).as_str(),
			None,
		)
		.unwrap()
		.into()
	}

	/// Return public key.
	fn public(self) -> AuthorityId {
		<Keyring as GenericKeyring<AuthorityId>>::pair(self).public().into()
	}

	/// Return seed string.
	fn to_seed(self) -> String {
		format!("//{}", self)
	}

	/// Get Keyring from public key.
	fn from_public(who: &AuthorityId) -> Option<Keyring> {
		Self::iter().find(|&k| <Keyring as GenericKeyring<AuthorityId>>::public(k) == *who)
	}
}

lazy_static::lazy_static! {
	static ref PRIVATE_KEYS: HashMap<Keyring, ecdsa_crypto::Pair> =
		Keyring::iter().map(|i| (i, <Keyring as GenericKeyring<ecdsa_crypto::AuthorityId>>::pair(i))).collect();
	static ref PUBLIC_KEYS: HashMap<Keyring, ecdsa_crypto::Public> =
		PRIVATE_KEYS.iter().map(|(&name, pair)| (name, sp_application_crypto::Pair::public(pair))).collect();
}

impl From<Keyring> for ecdsa_crypto::Pair {
	fn from(k: Keyring) -> Self {
		<Keyring as GenericKeyring<ecdsa_crypto::AuthorityId>>::pair(k)
	}
}

impl From<Keyring> for ecdsa::Pair {
	fn from(k: Keyring) -> Self {
		<Keyring as GenericKeyring<ecdsa_crypto::AuthorityId>>::pair(k).into()
	}
}

impl From<Keyring> for ecdsa_crypto::Public {
	fn from(k: Keyring) -> Self {
		(*PUBLIC_KEYS).get(&k).cloned().unwrap()
	}
}

/// Create a new `EquivocationProof` based on given arguments.
pub fn generate_equivocation_proof(
	vote1: (u64, Payload, ValidatorSetId, &Keyring),
	vote2: (u64, Payload, ValidatorSetId, &Keyring),
) -> EquivocationProof<u64, ecdsa_crypto::Public, ecdsa_crypto::Signature> {
	let signed_vote = |block_number: u64,
	                   payload: Payload,
	                   validator_set_id: ValidatorSetId,
	                   keyring: &Keyring| {
		let commitment = Commitment { validator_set_id, block_number, payload };
		let signature = <Keyring as GenericKeyring<ecdsa_crypto::AuthorityId>>::sign(
			*keyring,
			&commitment.encode(),
		);
		VoteMessage {
			commitment,
			id: <Keyring as GenericKeyring<ecdsa_crypto::AuthorityId>>::public(*keyring),
			signature,
		}
	};
	let first = signed_vote(vote1.0, vote1.1, vote1.2, vote1.3);
	let second = signed_vote(vote2.0, vote2.1, vote2.2, vote2.3);
	EquivocationProof { first, second }
}
