// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
use codec::{Codec, Decode};
use core::fmt::{Debug, Display};

use sp_application_crypto::{
	key_types::BEEFY as BEEFY_KEY_TYPE, AppCrypto, AppPublic, ByteArray, RuntimeAppPublic,
};
#[cfg(feature = "bls-experimental")]
use sp_core::{bls377, ecdsa_bls377};
use sp_core::{ecdsa, keccak_256};

use sp_keystore::KeystorePtr;
use sp_std::marker::PhantomData;

use log::warn;

use sp_consensus_beefy::{ecdsa_crypto, BeefyAuthorityId};

#[cfg(feature = "bls-experimental")]
use sp_consensus_beefy::bls_crypto;

use crate::{error, LOG_TARGET};

/// Hasher used for BEEFY signatures.
pub(crate) type BeefySignatureHasher = sp_runtime::traits::Keccak256;

pub trait AuthorityIdBound:
	Codec
	+ Debug
	+ Clone
	+ Ord
	+ Sync
	+ Send
	+ AsRef<[u8]>
	+ ByteArray
	+ AppPublic
	+ AppCrypto
	+ RuntimeAppPublic
	+ Display
	+ BeefyAuthorityId<BeefySignatureHasher>
where
	<Self as RuntimeAppPublic>::Signature: Send + Sync,
{
}

impl AuthorityIdBound for ecdsa_crypto::AuthorityId {}

// impl<T: Codec + Debug + Clone + Ord + Sync + Send + AsRef<[u8]> + AppPublic + AppCrypto +
// RuntimeAppPublic + BeefyAuthorityId<BeefySignatureHasher> >  AuthorityIdBound for T { type
// Signature = AppCrypto::Signature; }

/// A BEEFY specific keystore i mplemented as a `Newtype`. This is basically a
/// wrapper around [`sp_keystore::Keystore`] and allows to customize
/// commoncryptographic functionality.
pub(crate) struct BeefyKeystore<AuthorityId: AuthorityIdBound>(
	Option<KeystorePtr>,
	PhantomData<fn() -> AuthorityId>,
)
where
	<AuthorityId as RuntimeAppPublic>::Signature: Send + Sync;

impl<AuthorityId: AuthorityIdBound> BeefyKeystore<AuthorityId>
where
	<AuthorityId as RuntimeAppPublic>::Signature: Send + Sync,
{
	/// Check if the keystore contains a private key for one of the public keys
	/// contained in `keys`. A public key with a matching private key is known
	/// as a local authority id.
	///
	/// Return the public key for which we also do have a private key. If no
	/// matching private key is found, `None` will be returned.
	pub fn authority_id(&self, keys: &[AuthorityId]) -> Option<AuthorityId> {
		let store = self.0.clone()?;

		// we do check for multiple private keys as a key store sanity check.
		let public: Vec<AuthorityId> = keys
			.iter()
			.filter(|k| {
				store
					.has_keys(&[(<AuthorityId as RuntimeAppPublic>::to_raw_vec(k), BEEFY_KEY_TYPE)])
			})
			.cloned()
			.collect();

		if public.len() > 1 {
			warn!(
				target: LOG_TARGET,
				"🥩 Multiple private keys found for: {:?} ({})",
				public,
				public.len()
			);
		}

		public.get(0).cloned()
	}

	/// Sign `message` with the `public` key.
	///
	/// Note that `message` usually will be pre-hashed before being signed.
	///
	/// Return the message signature or an error in case of failure.
	pub fn sign(
		&self,
		public: &AuthorityId,
		message: &[u8],
	) -> Result<<AuthorityId as RuntimeAppPublic>::Signature, error::Error> {
		let store = self.0.clone().ok_or_else(|| error::Error::Keystore("no Keystore".into()))?;

		//ECDSA shoud do ecdsa_sign_prehashed because it needs to be hashed by keccak_256
		//as such we need to deal with signing case by case

		let signature_byte_array: Vec<u8> = match <AuthorityId as AppCrypto>::CRYPTO_ID {
			ecdsa::CRYPTO_ID => {
				let msg_hash = keccak_256(message);
				let public: ecdsa::Public = ecdsa::Public::try_from(public.as_slice()).unwrap();

				let sig = store
					.ecdsa_sign_prehashed(BEEFY_KEY_TYPE, &public, &msg_hash)
					.map_err(|e| error::Error::Keystore(e.to_string()))?
					.ok_or_else(|| {
						error::Error::Signature("ecdsa_sign_prehashed() failed".to_string())
					})?;
				let sig_ref: &[u8] = sig.as_ref();
				sig_ref.to_vec()
			},

			#[cfg(feature = "bls-experimental")]
			ecdsa_bls377::CRYPTO_ID => store
				.ecdsa_bls377_sign_with_keccak256(BEEFY_KEY_TYPE, public, &msg)
				.map_err(|e| error::Error::Keystore(e.to_string()))?
				.ok_or_else(|| {
					error::Error::AuthorityId::Signature(
						"bls377_sign()
  		failed"
							.to_string(),
					)
				})?
				.as_ref(),

			_ => {
				let sig = store
					.sign_with(
						<AuthorityId as AppCrypto>::ID,
						<AuthorityId as AppCrypto>::CRYPTO_ID,
						public.as_slice(),
						message,
					)
					.map_err(|e| error::Error::Signature(format!("{}. Key: {:?}", e, public)))?
					.ok_or_else(|| {
						error::Error::Signature(format!(
							"Could not find key in keystore. Key: {:?}",
							public
						))
					})?;

				sig
			},
		};

		//check that `sig` has the expected result type
		let signature = <AuthorityId as RuntimeAppPublic>::Signature::decode(
			&mut signature_byte_array.as_slice(),
		)
		.map_err(|_| {
			error::Error::Signature(format!(
				"invalid signature {:?} for key {:?}",
				signature_byte_array, public
			))
		})?;

		Ok(signature)
	}

	/// Returns a vector of [`sp_consensus_beefy::crypto::Public`] keys which are currently
	/// supported (i.e. found in the keystore).
	pub fn public_keys(&self) -> Result<Vec<AuthorityId>, error::Error> {
		let store = self.0.clone().ok_or_else(|| error::Error::Keystore("no Keystore".into()))?;

		let pk = match <AuthorityId as AppCrypto>::CRYPTO_ID {
			ecdsa::CRYPTO_ID => store
				.ecdsa_public_keys(BEEFY_KEY_TYPE)
				.drain(..)
				.map(|pk| AuthorityId::try_from(pk.as_ref()))
				.collect::<Result<Vec<_>, _>>()
				.or_else(|_| {
					Err(error::Error::Keystore(
						"unable to convert public key into authority id".into(),
					))
				}),

			#[cfg(feature = "bls-experimental")]
			ecdsa_bls377::CRYPTO_ID => store
				.ecdsa_bls377_public_keys(BEEFY_KEY_TYPE)
				.drain(..)
				.map(|pk| AuthorityId::try_from(pk.as_ref()))
				.collect::<Result<Vec<_>, _>>()
				.or_else(|_| {
					Err(error::Error::Keystore(
						"unable to convert public key into authority id".into(),
					))
				}),

			_ => Err(error::Error::Keystore("key type is not supported by BEEFY Keystore".into())),
		};

		pk
	}

	/// Use the `public` key to verify that `sig` is a valid signature for `message`.
	///
	/// Return `true` if the signature is authentic, `false` otherwise.
	pub fn verify(
		public: &AuthorityId,
		sig: &<AuthorityId as RuntimeAppPublic>::Signature,
		message: &[u8],
	) -> bool {
		BeefyAuthorityId::<BeefySignatureHasher>::verify(public, sig, message)
	}
}

impl<AuthorityId: AuthorityIdBound> From<Option<KeystorePtr>> for BeefyKeystore<AuthorityId>
where
	<AuthorityId as RuntimeAppPublic>::Signature: Send + Sync,
{
	fn from(store: Option<KeystorePtr>) -> BeefyKeystore<AuthorityId> {
		BeefyKeystore(store, PhantomData)
	}
}

#[cfg(test)]
pub mod tests {
	use sp_consensus_beefy::{ecdsa_crypto, Keyring};
	use sp_core::{ecdsa, Pair};
	use sp_keystore::testing::MemoryKeystore;

	use super::*;
	use crate::error::Error;

	fn keystore() -> KeystorePtr {
		MemoryKeystore::new().into()
	}

	#[test]
	fn verify_should_work() {
		let msg = keccak_256(b"I am Alice!");
		let sig = Keyring::Alice.sign(b"I am Alice!");

		assert!(ecdsa::Pair::verify_prehashed(
			&sig.clone().into(),
			&msg,
			&Keyring::Alice.public().into(),
		));

		// different public key -> fail
		assert!(!ecdsa::Pair::verify_prehashed(
			&sig.clone().into(),
			&msg,
			&Keyring::Bob.public().into(),
		));

		let msg = keccak_256(b"I am not Alice!");

		// different msg -> fail
		assert!(
			!ecdsa::Pair::verify_prehashed(&sig.into(), &msg, &Keyring::Alice.public().into(),)
		);
	}

	#[test]
	fn pair_works() {
		let want = ecdsa_crypto::Pair::from_string("//Alice", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Alice.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//Bob", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Bob.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//Charlie", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Charlie.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//Dave", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Dave.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//Eve", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Eve.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//Ferdie", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Ferdie.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//One", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::One.pair().to_raw_vec();
		assert_eq!(want, got);

		let want = ecdsa_crypto::Pair::from_string("//Two", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = Keyring::Two.pair().to_raw_vec();
		assert_eq!(want, got);
	}

	#[test]
	fn authority_id_works() {
		let store = keystore();

		let alice: ecdsa_crypto::Public = store
			.ecdsa_generate_new(BEEFY_KEY_TYPE, Some(&Keyring::Alice.to_seed()))
			.ok()
			.unwrap()
			.into();

		let bob = Keyring::Bob.public();
		let charlie = Keyring::Charlie.public();

		let store: BeefyKeystore<ecdsa_crypto::Public> = Some(store).into();

		let mut keys = vec![bob, charlie];

		let id = store.authority_id(keys.as_slice());
		assert!(id.is_none());

		keys.push(alice.clone());

		let id = store.authority_id(keys.as_slice()).unwrap();
		assert_eq!(id, alice);
	}

	#[test]
	fn sign_works() {
		let store = keystore();

		let alice: ecdsa_crypto::Public = store
			.ecdsa_generate_new(BEEFY_KEY_TYPE, Some(&Keyring::Alice.to_seed()))
			.ok()
			.unwrap()
			.into();

		let store: BeefyKeystore<ecdsa_crypto::Public> = Some(store).into();

		let msg = b"are you involved or commited?";

		let sig1 = store.sign(&alice, msg).unwrap();
		let sig2 = Keyring::Alice.sign(msg);

		assert_eq!(sig1, sig2);
	}

	#[test]
	fn sign_error() {
		let store = keystore();

		store
			.ecdsa_generate_new(BEEFY_KEY_TYPE, Some(&Keyring::Bob.to_seed()))
			.ok()
			.unwrap();

		let store: BeefyKeystore<ecdsa_crypto::Public> = Some(store).into();

		let alice = Keyring::Alice.public();

		let msg = b"are you involved or commited?";
		let sig = store.sign(&alice, msg).err().unwrap();
		let err = Error::Signature("ecdsa_sign_prehashed() failed".to_string());

		assert_eq!(sig, err);
	}

	#[test]
	fn sign_no_keystore() {
		let store: BeefyKeystore<ecdsa_crypto::Public> = None.into();

		let alice = Keyring::Alice.public();
		let msg = b"are you involved or commited";

		let sig = store.sign(&alice, msg).err().unwrap();
		let err = Error::Keystore("no Keystore".to_string());
		assert_eq!(sig, err);
	}

	#[test]
	fn verify_works() {
		let store = keystore();

		let alice: ecdsa_crypto::Public = store
			.ecdsa_generate_new(BEEFY_KEY_TYPE, Some(&Keyring::Alice.to_seed()))
			.ok()
			.unwrap()
			.into();

		let store: BeefyKeystore<ecdsa_crypto::Public> = Some(store).into();

		// `msg` and `sig` match
		let msg = b"are you involved or commited?";
		let sig = store.sign(&alice, msg).unwrap();
		assert!(BeefyKeystore::verify(&alice, &sig, msg));

		// `msg and `sig` don't match
		let msg = b"you are just involved";
		assert!(!BeefyKeystore::verify(&alice, &sig, msg));
	}

	// Note that we use keys with and without a seed for this test.
	#[test]
	fn public_keys_works() {
		const TEST_TYPE: sp_application_crypto::KeyTypeId =
			sp_application_crypto::KeyTypeId(*b"test");

		let store = keystore();

		let add_key =
			|key_type, seed: Option<&str>| store.ecdsa_generate_new(key_type, seed).unwrap();

		// test keys
		let _ = add_key(TEST_TYPE, Some(Keyring::Alice.to_seed().as_str()));
		let _ = add_key(TEST_TYPE, Some(Keyring::Bob.to_seed().as_str()));

		let _ = add_key(TEST_TYPE, None);
		let _ = add_key(TEST_TYPE, None);

		// BEEFY keys
		let _ = add_key(BEEFY_KEY_TYPE, Some(Keyring::Dave.to_seed().as_str()));
		let _ = add_key(BEEFY_KEY_TYPE, Some(Keyring::Eve.to_seed().as_str()));

		let key1: ecdsa_crypto::Public = add_key(BEEFY_KEY_TYPE, None).into();
		let key2: ecdsa_crypto::Public = add_key(BEEFY_KEY_TYPE, None).into();

		let store: BeefyKeystore<ecdsa_crypto::Public> = Some(store).into();

		let keys = store.public_keys().ok().unwrap();

		assert!(keys.len() == 4);
		assert!(keys.contains(&Keyring::Dave.public()));
		assert!(keys.contains(&Keyring::Eve.public()));
		assert!(keys.contains(&key1));
		assert!(keys.contains(&key2));
	}
}
