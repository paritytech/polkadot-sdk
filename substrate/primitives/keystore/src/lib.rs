// This file is part of Substrate.

// Copyright (C) 2020 Parity Technologies (UK) Ltd.
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

//! Keystore traits
pub mod testing;
pub mod vrf;

use std::sync::Arc;
use async_trait::async_trait;
use futures::{executor::block_on, future::join_all};
use sp_core::{
	crypto::{KeyTypeId, CryptoTypePublicPair},
	ed25519, sr25519, ecdsa,
};
use crate::vrf::{VRFTranscriptData, VRFSignature};

/// CryptoStore error
#[derive(Debug, derive_more::Display)]
pub enum Error {
	/// Public key type is not supported
	#[display(fmt="Key not supported: {:?}", _0)]
	KeyNotSupported(KeyTypeId),
	/// Pair not found for public key and KeyTypeId
	#[display(fmt="Pair was not found: {}", _0)]
	PairNotFound(String),
	/// Validation error
	#[display(fmt="Validation error: {}", _0)]
	ValidationError(String),
	/// Keystore unavailable
	#[display(fmt="Keystore unavailable")]
	Unavailable,
	/// Programming errors
	#[display(fmt="An unknown keystore error occurred: {}", _0)]
	Other(String)
}

/// Something that generates, stores and provides access to keys.
#[async_trait]
pub trait CryptoStore: Send + Sync {
	/// Returns all sr25519 public keys for the given key type.
	async fn sr25519_public_keys(&self, id: KeyTypeId) -> Vec<sr25519::Public>;
	/// Generate a new sr25519 key pair for the given key type and an optional seed.
	///
	/// If the given seed is `Some(_)`, the key pair will only be stored in memory.
	///
	/// Returns the public key of the generated key pair.
	async fn sr25519_generate_new(
		&self,
		id: KeyTypeId,
		seed: Option<&str>,
	) -> Result<sr25519::Public, Error>;
	/// Returns all ed25519 public keys for the given key type.
	async fn ed25519_public_keys(&self, id: KeyTypeId) -> Vec<ed25519::Public>;
	/// Generate a new ed25519 key pair for the given key type and an optional seed.
	///
	/// If the given seed is `Some(_)`, the key pair will only be stored in memory.
	///
	/// Returns the public key of the generated key pair.
	async fn ed25519_generate_new(
		&self,
		id: KeyTypeId,
		seed: Option<&str>,
	) -> Result<ed25519::Public, Error>;
	/// Returns all ecdsa public keys for the given key type.
	async fn ecdsa_public_keys(&self, id: KeyTypeId) -> Vec<ecdsa::Public>;
	/// Generate a new ecdsa key pair for the given key type and an optional seed.
	///
	/// If the given seed is `Some(_)`, the key pair will only be stored in memory.
	///
	/// Returns the public key of the generated key pair.
	async fn ecdsa_generate_new(
		&self,
		id: KeyTypeId,
		seed: Option<&str>,
	) -> Result<ecdsa::Public, Error>;

	/// Insert a new key. This doesn't require any known of the crypto; but a public key must be
	/// manually provided.
	///
	/// Places it into the file system store.
	///
	/// `Err` if there's some sort of weird filesystem error, but should generally be `Ok`.
	async fn insert_unknown(
		&self,
		_key_type: KeyTypeId,
		_suri: &str,
		_public: &[u8]
	) -> Result<(), ()>;

	/// Find intersection between provided keys and supported keys
	///
	/// Provided a list of (CryptoTypeId,[u8]) pairs, this would return
	/// a filtered set of public keys which are supported by the keystore.
	async fn supported_keys(
		&self,
		id: KeyTypeId,
		keys: Vec<CryptoTypePublicPair>
	) -> Result<Vec<CryptoTypePublicPair>, Error>;
	/// List all supported keys
	///
	/// Returns a set of public keys the signer supports.
	async fn keys(&self, id: KeyTypeId) -> Result<Vec<CryptoTypePublicPair>, Error>;

	/// Checks if the private keys for the given public key and key type combinations exist.
	///
	/// Returns `true` iff all private keys could be found.
	async fn has_keys(&self, public_keys: &[(Vec<u8>, KeyTypeId)]) -> bool;

	/// Sign with key
	///
	/// Signs a message with the private key that matches
	/// the public key passed.
	///
	/// Returns the SCALE encoded signature if key is found & supported,
	/// an error otherwise.
	async fn sign_with(
		&self,
		id: KeyTypeId,
		key: &CryptoTypePublicPair,
		msg: &[u8],
	) -> Result<Vec<u8>, Error>;

	/// Sign with any key
	///
	/// Given a list of public keys, find the first supported key and
	/// sign the provided message with that key.
	///
	/// Returns a tuple of the used key and the SCALE encoded signature.
	async fn sign_with_any(
		&self,
		id: KeyTypeId,
		keys: Vec<CryptoTypePublicPair>,
		msg: &[u8]
	) -> Result<(CryptoTypePublicPair, Vec<u8>), Error> {
		if keys.len() == 1 {
			return self.sign_with(id, &keys[0], msg).await.map(|s| (keys[0].clone(), s));
		} else {
			for k in self.supported_keys(id, keys).await? {
				if let Ok(sign) = self.sign_with(id, &k, msg).await {
					return Ok((k, sign));
				}
			}
		}
		Err(Error::KeyNotSupported(id))
	}

	/// Sign with all keys
	///
	/// Provided a list of public keys, sign a message with
	/// each key given that the key is supported.
	///
	/// Returns a list of `Result`s each representing the SCALE encoded
	/// signature of each key or a Error for non-supported keys.
	async fn sign_with_all(
		&self,
		id: KeyTypeId,
		keys: Vec<CryptoTypePublicPair>,
		msg: &[u8],
	) -> Result<Vec<Result<Vec<u8>, Error>>, ()> {
		let futs = keys.iter()
			.map(|k| self.sign_with(id, k, msg));

		Ok(join_all(futs).await)
	}

	/// Generate VRF signature for given transcript data.
	///
	/// Receives KeyTypeId and Public key to be able to map
	/// them to a private key that exists in the keystore which
	/// is, in turn, used for signing the provided transcript.
	///
	/// Returns a result containing the signature data.
	/// Namely, VRFOutput and VRFProof which are returned
	/// inside the `VRFSignature` container struct.
	///
	/// This function will return an error in the cases where
	/// the public key and key type provided do not match a private
	/// key in the keystore. Or, in the context of remote signing
	/// an error could be a network one.
	async fn sr25519_vrf_sign(
		&self,
		key_type: KeyTypeId,
		public: &sr25519::Public,
		transcript_data: VRFTranscriptData,
	) -> Result<VRFSignature, Error>;
}

/// Sync version of the CryptoStore
///
/// Some parts of Substrate still rely on a sync version of the `CryptoStore`.
/// To make the transition easier this auto trait wraps any async `CryptoStore` and
/// exposes a `sync` interface using `block_on`. Usage of this is deprecated and it
/// will be removed as soon as the internal usage has transitioned successfully.
/// If you are starting out building something new **do not use this**,
/// instead, use [`CryptoStore`].
pub trait SyncCryptoStore: CryptoStore + Send + Sync {
	/// Returns all sr25519 public keys for the given key type.
	fn sr25519_public_keys(&self, id: KeyTypeId) -> Vec<sr25519::Public>;

	/// Generate a new sr25519 key pair for the given key type and an optional seed.
	///
	/// If the given seed is `Some(_)`, the key pair will only be stored in memory.
	///
	/// Returns the public key of the generated key pair.
	fn sr25519_generate_new(
		&self,
		id: KeyTypeId,
		seed: Option<&str>,
	) -> Result<sr25519::Public, Error>;

	/// Returns all ed25519 public keys for the given key type.
	fn ed25519_public_keys(&self, id: KeyTypeId) -> Vec<ed25519::Public>;

	/// Generate a new ed25519 key pair for the given key type and an optional seed.
	///
	/// If the given seed is `Some(_)`, the key pair will only be stored in memory.
	///
	/// Returns the public key of the generated key pair.
	fn ed25519_generate_new(
		&self,
		id: KeyTypeId,
		seed: Option<&str>,
	) -> Result<ed25519::Public, Error>;

	/// Returns all ecdsa public keys for the given key type.
	fn ecdsa_public_keys(&self, id: KeyTypeId) -> Vec<ecdsa::Public>;

	/// Generate a new ecdsa key pair for the given key type and an optional seed.
	///
	/// If the given seed is `Some(_)`, the key pair will only be stored in memory.
	///
	/// Returns the public key of the generated key pair.
	fn ecdsa_generate_new(
		&self,
		id: KeyTypeId,
		seed: Option<&str>,
	) -> Result<ecdsa::Public, Error>;

	/// Insert a new key. This doesn't require any known of the crypto; but a public key must be
	/// manually provided.
	///
	/// Places it into the file system store.
	///
	/// `Err` if there's some sort of weird filesystem error, but should generally be `Ok`.
	fn insert_unknown(&self, key_type: KeyTypeId, suri: &str, public: &[u8]) -> Result<(), ()>;

	/// Find intersection between provided keys and supported keys
	///
	/// Provided a list of (CryptoTypeId,[u8]) pairs, this would return
	/// a filtered set of public keys which are supported by the keystore.
	fn supported_keys(
		&self,
		id: KeyTypeId,
		keys: Vec<CryptoTypePublicPair>
	) -> Result<Vec<CryptoTypePublicPair>, Error>;

	/// List all supported keys
	///
	/// Returns a set of public keys the signer supports.
	fn keys(&self, id: KeyTypeId) -> Result<Vec<CryptoTypePublicPair>, Error> {
		block_on(CryptoStore::keys(self, id))
	}

	/// Checks if the private keys for the given public key and key type combinations exist.
	///
	/// Returns `true` iff all private keys could be found.
	fn has_keys(&self, public_keys: &[(Vec<u8>, KeyTypeId)]) -> bool;

	/// Sign with key
	///
	/// Signs a message with the private key that matches
	/// the public key passed.
	///
	/// Returns the SCALE encoded signature if key is found & supported,
	/// an error otherwise.
	fn sign_with(
		&self,
		id: KeyTypeId,
		key: &CryptoTypePublicPair,
		msg: &[u8],
	) -> Result<Vec<u8>, Error>;

	/// Sign with any key
	///
	/// Given a list of public keys, find the first supported key and
	/// sign the provided message with that key.
	///
	/// Returns a tuple of the used key and the SCALE encoded signature.
	fn sign_with_any(
		&self,
		id: KeyTypeId,
		keys: Vec<CryptoTypePublicPair>,
		msg: &[u8]
	) -> Result<(CryptoTypePublicPair, Vec<u8>), Error> {
		if keys.len() == 1 {
			return SyncCryptoStore::sign_with(self, id, &keys[0], msg).map(|s| (keys[0].clone(), s));
		} else {
			for k in SyncCryptoStore::supported_keys(self, id, keys)? {
				if let Ok(sign) = SyncCryptoStore::sign_with(self, id, &k, msg) {
					return Ok((k, sign));
				}
			}
		}
		Err(Error::KeyNotSupported(id))
	}

	/// Sign with all keys
	///
	/// Provided a list of public keys, sign a message with
	/// each key given that the key is supported.
	///
	/// Returns a list of `Result`s each representing the SCALE encoded
	/// signature of each key or a Error for non-supported keys.
	fn sign_with_all(
		&self,
		id: KeyTypeId,
		keys: Vec<CryptoTypePublicPair>,
		msg: &[u8],
	) -> Result<Vec<Result<Vec<u8>, Error>>, ()>{
		Ok(keys.iter().map(|k| SyncCryptoStore::sign_with(self, id, k, msg)).collect())
	}

	/// Generate VRF signature for given transcript data.
	///
	/// Receives KeyTypeId and Public key to be able to map
	/// them to a private key that exists in the keystore which
	/// is, in turn, used for signing the provided transcript.
	///
	/// Returns a result containing the signature data.
	/// Namely, VRFOutput and VRFProof which are returned
	/// inside the `VRFSignature` container struct.
	///
	/// This function will return an error in the cases where
	/// the public key and key type provided do not match a private
	/// key in the keystore. Or, in the context of remote signing
	/// an error could be a network one.
	fn sr25519_vrf_sign(
		&self,
		key_type: KeyTypeId,
		public: &sr25519::Public,
		transcript_data: VRFTranscriptData,
	) -> Result<VRFSignature, Error>;
}

/// A pointer to a keystore.
pub type SyncCryptoStorePtr = Arc<dyn SyncCryptoStore>;

sp_externalities::decl_extension! {
	/// The keystore extension to register/retrieve from the externalities.
	pub struct KeystoreExt(SyncCryptoStorePtr);
}
