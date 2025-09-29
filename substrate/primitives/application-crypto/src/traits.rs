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

use codec::Codec;
use scale_info::TypeInfo;

use alloc::vec::Vec;
use core::fmt::Debug;
use sp_core::crypto::{CryptoType, CryptoTypeId, IsWrappedBy, KeyTypeId, Pair, Public, Signature};

/// Application-specific cryptographic object.
///
/// Combines all the core types and constants that are defined by a particular
/// cryptographic scheme when it is used in a specific application domain.
///
/// Typically, the implementers of this trait are its associated types themselves.
/// This provides a convenient way to access generic information about the scheme
/// given any of the associated types.
pub trait AppCrypto: 'static + Sized + CryptoType {
	/// Identifier for application-specific key type.
	const ID: KeyTypeId;

	/// Identifier of the crypto type of this application-specific key type.
	const CRYPTO_ID: CryptoTypeId;

	/// The corresponding public key type in this application scheme.
	type Public: AppPublic;

	/// The corresponding signature type in this application scheme.
	type Signature: AppSignature;

	/// The corresponding proof of possession type in this application scheme.
	type ProofOfPossession: AppSignature;

	/// The corresponding key pair type in this application scheme.
	type Pair: AppPair;
}

/// Type which implements Hash in std, not when no-std (std variant).
pub trait MaybeHash: core::hash::Hash {}
impl<T: core::hash::Hash> MaybeHash for T {}

/// Application-specific key pair.
pub trait AppPair:
	AppCrypto + Pair<Public = <Self as AppCrypto>::Public, Signature = <Self as AppCrypto>::Signature>
{
	/// The wrapped type which is just a plain instance of `Pair`.
	type Generic: IsWrappedBy<Self>
		+ Pair<Public = <<Self as AppCrypto>::Public as AppPublic>::Generic>
		+ Pair<Signature = <<Self as AppCrypto>::Signature as AppSignature>::Generic>;
}

/// Application-specific public key.
pub trait AppPublic: AppCrypto + Public + Debug + MaybeHash + Codec {
	/// The wrapped type which is just a plain instance of `Public`.
	type Generic: IsWrappedBy<Self> + Public + Debug + MaybeHash + Codec;
}

/// Application-specific signature and Proof Of Possession
pub trait AppSignature: AppCrypto + Signature + Eq + PartialEq + Debug + Clone {
	/// The wrapped type which is just a plain instance of `Signature`.
	type Generic: IsWrappedBy<Self> + Signature + Eq + PartialEq + Debug;
}

/// Runtime interface for a public key.
pub trait RuntimePublic: Sized {
	/// The signature that will be generated when signing with the corresponding private key.
	type Signature: Debug + Eq + PartialEq + Clone;

	/// The Proof Of Possession the corresponding private key.
	type ProofOfPossession: Debug + Eq + PartialEq + Clone;

	/// Returns all public keys for the given key type in the keystore.
	fn all(key_type: KeyTypeId) -> crate::Vec<Self>;

	/// Generate a public/private pair for the given key type with an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be valid utf8.
	///
	/// Returns the generated public key.
	fn generate_pair(key_type: KeyTypeId, seed: Option<Vec<u8>>) -> Self;

	/// Sign the given message with the corresponding private key of this public key.
	///
	/// The private key will be requested from the keystore using the given key type.
	///
	/// Returns the signature or `None` if the private key could not be found or some other error
	/// occurred.
	fn sign<M: AsRef<[u8]>>(&self, key_type: KeyTypeId, msg: &M) -> Option<Self::Signature>;

	/// Verify that the given signature matches the given message using this public key.
	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool;

	/// Generates the necessary proof(s) usually as a signature or list of signatures, for the
	/// corresponding public key to be accepted as legitimate by the network.
	///
	/// This includes attestation of the owner of the public key in the form of signing the owner's
	/// identity. It might also includes other signatures such as signature obtained by signing
	/// the corresponding public in different context than normal signatures in case of BLS
	/// key pair.
	///
	/// The `owner` is an arbitrary byte array representing the identity of the owner of
	/// the key which has been signed by the private key in process of generating the proof.
	///
	/// The private key will be requested from the keystore using the given key type.
	///
	/// Returns the proof of possession or `None` if it failed or is not able to do
	/// so.
	fn generate_proof_of_possession(
		&mut self,
		key_type: KeyTypeId,
		owner: &[u8],
	) -> Option<Self::ProofOfPossession>;

	/// Verifies that the given proof is valid for the corresponding public key.
	/// The proof is usually a signature or list of signatures, for the corresponding
	/// public key to be accepted by the network. It include attestation of the owner of
	/// the public key in the form signing the owner's identity but might also includes
	/// other signatures.
	///
	/// The `owner` is an arbitrary byte array representing the identity of the owner of
	/// the key which has been signed by the private key in process of generating the proof.
	///
	/// Returns `true` if the proof is deemed correct by the cryto type.
	fn verify_proof_of_possession(&self, owner: &[u8], pop: &Self::ProofOfPossession) -> bool;

	/// Returns `Self` as raw vec.
	fn to_raw_vec(&self) -> Vec<u8>;
}

/// Runtime interface for an application's public key.
pub trait RuntimeAppPublic: Sized {
	/// An identifier for this application-specific key type.
	const ID: KeyTypeId;

	/// The signature that will be generated when signing with the corresponding private key.
	type Signature: Debug + Eq + PartialEq + Clone + TypeInfo + Codec;

	/// The Proof Of Possession the corresponding private key.
	type ProofOfPossession: Debug + Eq + PartialEq + TypeInfo + Clone;

	/// Returns all public keys for this application in the keystore.
	fn all() -> crate::Vec<Self>;

	/// Generate a public/private pair with an optional `seed` and store it in the keystore.
	///
	/// The `seed` needs to be valid utf8.
	///
	/// Returns the generated public key.
	fn generate_pair(seed: Option<Vec<u8>>) -> Self;

	/// Sign the given message with the corresponding private key of this public key.
	///
	/// The private key will be requested from the keystore.
	///
	/// Returns the signature or `None` if the private key could not be found or some other error
	/// occurred.
	fn sign<M: AsRef<[u8]>>(&self, msg: &M) -> Option<Self::Signature>;

	/// Verify that the given signature matches the given message using this public key.
	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool;

	/// Generate proof of legitimacy for the corresponding public key
	///
	/// Returns the proof of possession, usually a signature or a list of signature,  or `None` if
	/// it failed or is not able to do so.
	fn generate_proof_of_possession(&mut self, owner: &[u8]) -> Option<Self::ProofOfPossession>;

	/// Verify that the given proof of possession is valid for the corresponding public key.
	fn verify_proof_of_possession(&self, owner: &[u8], pop: &Self::ProofOfPossession) -> bool;

	/// Returns `Self` as raw vec.
	fn to_raw_vec(&self) -> Vec<u8>;
}

impl<T> RuntimeAppPublic for T
where
	T: AppPublic + AsRef<<T as AppPublic>::Generic> + AsMut<<T as AppPublic>::Generic>,
	<T as AppPublic>::Generic: RuntimePublic,
	<T as AppCrypto>::Signature: TypeInfo
		+ Codec
		+ From<<<T as AppPublic>::Generic as RuntimePublic>::Signature>
		+ AsRef<<<T as AppPublic>::Generic as RuntimePublic>::Signature>,
	<T as AppCrypto>::ProofOfPossession: TypeInfo
		+ Codec
		+ From<<<T as AppPublic>::Generic as RuntimePublic>::ProofOfPossession>
		+ AsRef<<<T as AppPublic>::Generic as RuntimePublic>::ProofOfPossession>,
{
	const ID: KeyTypeId = <T as AppCrypto>::ID;

	type Signature = <T as AppCrypto>::Signature;
	type ProofOfPossession = <T as AppCrypto>::ProofOfPossession;

	fn all() -> crate::Vec<Self> {
		<<T as AppPublic>::Generic as RuntimePublic>::all(Self::ID)
			.into_iter()
			.map(|p| p.into())
			.collect()
	}

	fn generate_pair(seed: Option<Vec<u8>>) -> Self {
		<<T as AppPublic>::Generic as RuntimePublic>::generate_pair(Self::ID, seed).into()
	}

	fn sign<M: AsRef<[u8]>>(&self, msg: &M) -> Option<Self::Signature> {
		<<T as AppPublic>::Generic as RuntimePublic>::sign(self.as_ref(), Self::ID, msg)
			.map(|s| s.into())
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		<<T as AppPublic>::Generic as RuntimePublic>::verify(self.as_ref(), msg, signature.as_ref())
	}

	fn generate_proof_of_possession(&mut self, owner: &[u8]) -> Option<Self::ProofOfPossession> {
		<<T as AppPublic>::Generic as RuntimePublic>::generate_proof_of_possession(
			self.as_mut(),
			Self::ID,
			owner,
		)
		.map(|s| s.into())
	}

	fn verify_proof_of_possession(&self, owner: &[u8], pop: &Self::ProofOfPossession) -> bool {
		<<T as AppPublic>::Generic as RuntimePublic>::verify_proof_of_possession(
			self.as_ref(),
			owner,
			pop.as_ref(),
		)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		<<T as AppPublic>::Generic as RuntimePublic>::to_raw_vec(self.as_ref())
	}
}

/// Something that is bound to a fixed [`RuntimeAppPublic`].
pub trait BoundToRuntimeAppPublic {
	/// The [`RuntimeAppPublic`] this type is bound to.
	type Public: RuntimeAppPublic;
}

impl<T: RuntimeAppPublic> BoundToRuntimeAppPublic for T {
	type Public = Self;
}
