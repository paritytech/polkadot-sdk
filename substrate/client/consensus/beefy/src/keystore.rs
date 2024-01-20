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
use codec::Decode;

use sp_application_crypto::{key_types::BEEFY as BEEFY_KEY_TYPE, AppCrypto, RuntimeAppPublic};
#[cfg(feature = "bls-experimental")]
use sp_core::ecdsa_bls377;
use sp_core::{ecdsa, keccak_256};

use sp_keystore::KeystorePtr;
use std::marker::PhantomData;

use log::warn;

use sp_consensus_beefy::{AuthorityIdBound, BeefyAuthorityId, BeefySignatureHasher};

#[cfg(feature = "bls-experimental")]
use sp_consensus_beefy::ecdsa_bls_crypto;

use crate::{error, LOG_TARGET};


/// A BEEFY specific keystore implemented as a `Newtype`. This is basically a
/// wrapper around [`sp_keystore::Keystore`] and allows to customize
/// common cryptographic functionality.
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

		// ECDSA should use ecdsa_sign_prehashed since it needs to be hashed by keccak_256 instead
		// of blake2. As such we need to deal with producing the signatures case-by-case
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
			ecdsa_bls377::CRYPTO_ID => {
				let public: ecdsa_bls377::Public =
					ecdsa_bls377::Public::try_from(public.as_slice()).unwrap();
				let sig = store
					.ecdsa_bls377_sign_with_keccak256(BEEFY_KEY_TYPE, &public, &message)
					.map_err(|e| error::Error::Keystore(e.to_string()))?
					.ok_or_else(|| error::Error::Signature("bls377_sign()  failed".to_string()))?;
				let sig_ref: &[u8] = sig.as_ref();
				sig_ref.to_vec()
			},

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
	use sp_consensus_beefy::{ecdsa_crypto, test_utils::BeefySignerAuthority, test_utils::GenericKeyring, test_utils::Keyring};
	use sp_core::Pair as PairT;
	use sp_keystore::testing::MemoryKeystore;

	use super::*;
	use crate::error::Error;

	fn keystore() -> KeystorePtr {
		MemoryKeystore::new().into()
	}

	fn pair_verify_should_work<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>()
	where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		let msg = b"I am Alice!";
		let sig = <Keyring as GenericKeyring<AuthorityId>>::sign(Keyring::Alice, b"I am Alice!");

		assert!(<AuthorityId as BeefyAuthorityId<BeefySignatureHasher>>::verify(
			&<Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Alice),
			&sig,
			&msg.as_slice(),
		));

		// different public key -> fail
		assert!(!<AuthorityId as BeefyAuthorityId<BeefySignatureHasher>>::verify(
			&<Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Bob),
			&sig,
			&msg.as_slice(),
		));

		let msg = b"I am not Alice!";

		// different msg -> fail
		assert!(!<AuthorityId as BeefyAuthorityId<BeefySignatureHasher>>::verify(
			&<Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Alice),
			&sig,
			&msg.as_slice(),
		));
	}

	#[test]
	fn pair_verify_should_work_ecdsa() {
		pair_verify_should_work::<ecdsa_crypto::AuthorityId>();
	}

	#[cfg(feature = "bls-experimental")]
	#[test]
	fn pair_verify_should_work_ecdsa_n_bls() {
		pair_verify_should_work::<ecdsa_bls_crypto::AuthorityId>();
	}

	fn pair_works<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>()
	where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Alice", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Alice).to_raw_vec();
		assert_eq!(want, got);

		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Bob", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Bob).to_raw_vec();
		assert_eq!(want, got);

		let want =
			<Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Charlie", None)
				.expect("Pair failed")
				.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Charlie).to_raw_vec();
		assert_eq!(want, got);

		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Dave", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Dave).to_raw_vec();
		assert_eq!(want, got);

		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Eve", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Eve).to_raw_vec();
		assert_eq!(want, got);

		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Ferdie", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Ferdie).to_raw_vec();
		assert_eq!(want, got);

		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//One", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::One).to_raw_vec();
		assert_eq!(want, got);

		let want = <Keyring as GenericKeyring<AuthorityId>>::KeyPair::from_string("//Two", None)
			.expect("Pair failed")
			.to_raw_vec();
		let got = <Keyring as GenericKeyring<AuthorityId>>::pair(Keyring::Two).to_raw_vec();
		assert_eq!(want, got);
	}

	#[test]
	fn ecdsa_pair_works() {
		pair_works::<ecdsa_crypto::AuthorityId>();
	}

	#[cfg(feature = "bls-experimental")]
	#[test]
	fn ecdsa_n_bls_pair_works() {
		pair_works::<ecdsa_bls_crypto::AuthorityId>();
	}

	fn authority_id_works<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>()
	where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		let store = keystore();

		<Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			Some(Keyring::Alice),
		);

		let alice = <Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Alice);

		let bob = <Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Bob);
		let charlie = <Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Charlie);

		let beefy_store: BeefyKeystore<AuthorityId> = Some(store).into();

		let mut keys = vec![bob, charlie];

		let id = beefy_store.authority_id(keys.as_slice());
		assert!(id.is_none());

		keys.push(alice.clone());

		let id = beefy_store.authority_id(keys.as_slice()).unwrap();
		assert_eq!(id, alice);
	}

	#[cfg(feature = "bls-experimental")]
	#[test]

	fn authority_id_works_for_ecdsa() {
		authority_id_works::<ecdsa_crypto::AuthorityId>();
	}

	#[cfg(feature = "bls-experimental")]
	#[test]

	fn authority_id_works_for_ecdsa_n_bls() {
		authority_id_works::<ecdsa_bls_crypto::AuthorityId>();
	}

	fn sign_works<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>()
	where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		let store = keystore();

		<Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			Some(Keyring::Alice),
		);

		let alice = <Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Alice);

		let store: BeefyKeystore<AuthorityId> = Some(store).into();

		let msg = b"are you involved or commited?";

		let sig1 = store.sign(&alice, msg).unwrap();
		let sig2 = <Keyring as GenericKeyring<AuthorityId>>::sign(Keyring::Alice, msg);

		assert_eq!(sig1, sig2);
	}

	#[test]
	fn sign_works_for_ecdsa() {
		sign_works::<ecdsa_crypto::AuthorityId>();
	}

	#[cfg(feature = "bls-experimental")]
	#[test]
	fn sign_works_for_ecdsa_n_bls() {
		sign_works::<ecdsa_bls_crypto::AuthorityId>();
	}

	fn sign_error<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>(
		expected_error_message: &str,
	) where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		let store = keystore();

		<Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			Some(Keyring::Bob),
		);

		let store: BeefyKeystore<AuthorityId> = Some(store).into();

		let alice = <Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Alice);

		let msg = b"are you involved or commited?";
		let sig = store.sign(&alice, msg).err().unwrap();
		let err = Error::Signature(expected_error_message.to_string());

		assert_eq!(sig, err);
	}

	#[test]
	fn sign_error_for_ecdsa() {
		sign_error::<ecdsa_crypto::AuthorityId>("ecdsa_sign_prehashed() failed");
	}

	#[cfg(feature = "bls-experimental")]
	#[test]
	fn sign_error_for_ecdsa_n_bls() {
		sign_error::<ecdsa_bls_crypto::AuthorityId>("bls377_sign()  failed");
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

	fn verify_works<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>()
	where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		let store = keystore();

		<Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			Some(Keyring::Alice),
		);

		let store: BeefyKeystore<AuthorityId> = Some(store).into();

		let alice = <Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Alice);

		// `msg` and `sig` match
		let msg = b"are you involved or commited?";
		let sig = store.sign(&alice, msg).unwrap();
		assert!(BeefyKeystore::verify(&alice, &sig, msg));

		// `msg and `sig` don't match
		let msg = b"you are just involved";
		assert!(!BeefyKeystore::verify(&alice, &sig, msg));
	}

	#[test]
	fn verify_works_for_ecdsa() {
		verify_works::<ecdsa_crypto::AuthorityId>();
	}

	#[cfg(feature = "bls-experimental")]
	#[test]

	fn verify_works_for_ecdsa_n_bls() {
		verify_works::<ecdsa_bls_crypto::AuthorityId>();
	}

	// Note that we use keys with and without a seed for this test.
	fn public_keys_works<
		AuthorityId: AuthorityIdBound + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Public>,
	>()
	where
		<AuthorityId as sp_runtime::RuntimeAppPublic>::Signature:
			Send + Sync + From<<<AuthorityId as AppCrypto>::Pair as AppCrypto>::Signature>,
		<AuthorityId as AppCrypto>::Pair: BeefySignerAuthority<sp_runtime::traits::Keccak256>,
	{
		const TEST_TYPE: sp_application_crypto::KeyTypeId =
			sp_application_crypto::KeyTypeId(*b"test");

		let store = keystore();

		// test keys
		let _ = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			TEST_TYPE,
			Some(Keyring::Alice),
		);
		let _ = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			TEST_TYPE,
			Some(Keyring::Bob),
		);

		// BEEFY keys
		let _ = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			Some(Keyring::Dave),
		);
		let _ = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			Some(Keyring::Eve),
		);

		let _ = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			TEST_TYPE,
			None,
		);
		let _ = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			TEST_TYPE,
			None,
		);

		let key1 = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			None,
		);
		let key2 = <Keyring as GenericKeyring<AuthorityId>>::generate_in_store(
			store.clone(),
			BEEFY_KEY_TYPE,
			None,
		);

		let store: BeefyKeystore<AuthorityId> = Some(store).into();

		let keys = store.public_keys().ok().unwrap();

		assert!(keys.len() == 4);
		assert!(keys.contains(&<Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Dave)));
		assert!(keys.contains(&<Keyring as GenericKeyring<AuthorityId>>::public(Keyring::Eve)));
		assert!(keys.contains(&key1));
		assert!(keys.contains(&key2));
	}

	#[test]
	fn public_keys_works_for_ecdsa_keystore() {
		public_keys_works::<ecdsa_crypto::AuthorityId>();
	}

	#[cfg(feature = "bls-experimental")]
	#[test]

	fn public_keys_works_for_ecdsa_n_bls() {
		public_keys_works::<ecdsa_bls_crypto::AuthorityId>();
	}
}
