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

use alloc::{vec, vec::Vec};

#[cfg(not(substrate_runtime))]
use sp_core::crypto::Pair;
#[cfg(not(substrate_runtime))]
use sp_keystore::KeystoreExt;

#[cfg(feature = "bandersnatch-experimental")]
use sp_core::bandersnatch;
use sp_core::{crypto::KeyTypeId, ecdsa, ed25519, sr25519};

#[cfg(feature = "bls-experimental")]
use sp_core::{bls381, ecdsa_bls381};

use sp_runtime_interface::{
	pass_by::{
		AllocateAndReturnByCodec, AllocateAndReturnPointer, ConvertAndReturnAs,
		PassFatPointerAndDecode, PassFatPointerAndRead, PassFatPointerAndReadWrite,
		PassPointerAndRead, PassPointerAndReadCopy, PassPointerAndWrite,
	},
	runtime_interface,
};

#[cfg(not(substrate_runtime))]
use secp256k1::{
	ecdsa::{RecoverableSignature, RecoveryId},
	Message,
};

#[cfg(not(substrate_runtime))]
use sp_externalities::ExternalitiesExt;

use crate::*;

/// Interfaces for working with crypto related types from within the runtime.
#[runtime_interface]
pub trait Crypto {
	/// Returns all `ed25519` public keys for the given key id from the keystore.
	fn ed25519_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
	) -> AllocateAndReturnByCodec<Vec<ed25519::Public>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_public_keys(id)
	}

	/// Stores all `ed25519` public keys for the given key id from the keystore into the output
	/// buffer, if it is large enough. Returns the number of bytes occupied by the keys, regardless
	/// of whether the buffer was written or not.
	// ERRATA: Caching of the result was added to address security concerns, although it wasn't
	// directly requested by the RFC
	#[version(2)]
	#[raw_api]
	fn ed25519_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		out: PassFatPointerAndReadWrite<&mut [ed25519::Public]>,
	) -> u32 {
		ensure_public_keys_cache_ext_registered!(self);

		let cached = self
			.extension::<PublicKeysCacheExt>()
			.expect("`PublicKeysCacheExt` was just registered; qed")
			.ed25519
			.take()
			.and_then(|(cached_id, keys)| (cached_id == id).then_some(keys));

		let keys = match cached {
			Some(snapshot) if out.len() >= snapshot.len() => snapshot,
			_ => self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ed25519_public_keys(id),
		};

		let key_size = core::mem::size_of::<ed25519::Public>();
		let total = keys.len();

		if out.len() >= total {
			out[..total].copy_from_slice(&keys);
		} else {
			self.extension::<PublicKeysCacheExt>()
				.expect("`PublicKeysCacheExt` is registered; qed")
				.ed25519 = Some((id, keys));
		}

		(total * key_size) as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `ed25519_public_keys` host function
	#[wrapper]
	fn ed25519_public_keys(id: KeyTypeId) -> Vec<ed25519::Public> {
		let key_size = core::mem::size_of::<ed25519::Public>();
		let num_keys = ed25519_public_keys__raw(id, &mut []) as usize / key_size;
		let mut keys = vec![ed25519::Public::default(); num_keys];
		let num_keys = ed25519_public_keys__raw(id, &mut keys) as usize / key_size;
		keys.truncate(num_keys);
		keys
	}

	/// Generate an `ed22519` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn ed25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<ed25519::Public, 32> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_generate_new(id, seed)
			.expect("`ed25519_generate` failed")
	}

	/// Generate an `ed22519` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Stores the public key in the provided output buffer.
	// ERRATA: The RFC mentions the `seed` is `i32` in the prototype section, but in the description
	// it calls for a pointer-size. Applies to all the *_generate functions.
	#[version(2)]
	#[raw_api]
	fn ed25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
		out: PassPointerAndWrite<&mut ed25519::Public, 32>,
	) {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		out.0.copy_from_slice(
			&self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ed25519_generate_new(id, seed)
				.expect("`ed25519_generate` failed"),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ed25519_generate`
	/// host function.
	#[wrapper]
	fn ed25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ed25519::Public {
		let mut public = ed25519::Public::default();
		ed25519_generate__raw(id, seed, &mut public);
		public
	}

	/// Sign the given `msg` with the `ed25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn ed25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<ed25519::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given `msg` with the `ed25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	// ERRATA: The RFC erroneously declares `out` to be `i64`. Applies to all *_sign_* functions.
	#[version(2)]
	#[raw_api]
	fn ed25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
		out: PassPointerAndWrite<&mut ed25519::Signature, 64>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i32> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ed25519_sign` host
	/// function.
	#[wrapper]
	fn ed25519_sign(
		id: KeyTypeId,
		pub_key: &ed25519::Public,
		message: &[u8],
	) -> Option<ed25519::Signature> {
		let mut signature = ed25519::Signature::default();
		ed25519_sign__raw(id, pub_key, message, &mut signature).ok()?;
		Some(signature)
	}

	/// Verify `ed25519` signature.
	///
	/// Returns `true` when the verification was successful.
	fn ed25519_verify(
		sig: PassPointerAndRead<&ed25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
	) -> bool {
		// We don't want to force everyone needing to call the function in an externalities context.
		// So, we assume that we should not use dalek when we are not in externalities context.
		// Otherwise, we check if the extension is present.
		if sp_externalities::with_externalities(|mut e| e.extension::<UseDalekExt>().is_some())
			.unwrap_or_default()
		{
			use ed25519_dalek::Verifier;

			let Ok(public_key) = ed25519_dalek::VerifyingKey::from_bytes(&pub_key.0) else {
				return false;
			};

			let sig = ed25519_dalek::Signature::from_bytes(&sig.0);

			public_key.verify(msg, &sig).is_ok()
		} else {
			ed25519::Pair::verify(sig, msg, pub_key)
		}
	}

	/// Register a `ed25519` signature for batch verification.
	///
	/// Batch verification must be enabled by calling [`start_batch_verify`].
	/// If batch verification is not enabled, the signature will be verified immediately.
	/// To get the result of the batch verification, [`finish_batch_verify`]
	/// needs to be called.
	///
	/// Returns `true` when the verification is either successful or batched.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn ed25519_batch_verify(
		&mut self,
		sig: PassPointerAndRead<&ed25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
	) -> bool {
		let res = ed25519_verify(sig, msg, pub_key);

		if let Some(ext) = self.extension::<VerificationExtDeprecated>() {
			ext.0 &= res;
		}

		res
	}

	/// Verify `sr25519` signature.
	///
	/// Returns `true` when the verification was successful.
	#[version(2)]
	fn sr25519_verify(
		sig: PassPointerAndRead<&sr25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
	) -> bool {
		sr25519::Pair::verify(sig, msg, pub_key)
	}

	/// Register a `sr25519` signature for batch verification.
	///
	/// Batch verification must be enabled by calling [`start_batch_verify`].
	/// If batch verification is not enabled, the signature will be verified immediately.
	/// To get the result of the batch verification, [`finish_batch_verify`]
	/// needs to be called.
	///
	/// Returns `true` when the verification is either successful or batched.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn sr25519_batch_verify(
		&mut self,
		sig: PassPointerAndRead<&sr25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
	) -> bool {
		let res = sr25519_verify(sig, msg, pub_key);

		if let Some(ext) = self.extension::<VerificationExtDeprecated>() {
			ext.0 &= res;
		}

		res
	}

	/// Start verification extension.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn start_batch_verify(&mut self) {
		self.register_extension(VerificationExtDeprecated(true))
			.expect("Failed to register required extension: `VerificationExt`");
	}

	/// Finish batch-verification of signatures.
	///
	/// Verify or wait for verification to finish for all signatures which were previously
	/// deferred by `sr25519_verify`/`ed25519_verify`.
	///
	/// Will panic if no `VerificationExt` is registered (`start_batch_verify` was not called).
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn finish_batch_verify(&mut self) -> bool {
		let result = self
			.extension::<VerificationExtDeprecated>()
			.expect("`finish_batch_verify` should only be called after `start_batch_verify`")
			.0;

		self.deregister_extension::<VerificationExtDeprecated>()
			.expect("No verification extension in current context!");

		result
	}

	/// Returns all `sr25519` public keys for the given key id from the keystore.
	fn sr25519_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
	) -> AllocateAndReturnByCodec<Vec<sr25519::Public>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_public_keys(id)
	}

	/// Stores all `sr25519` public keys for the given key id from the keystore into the output
	/// buffer, if it is large enough. Returns the number of bytes occupied by the keys, regardless
	/// of whether the buffer was written or not.
	// ERRATA: Caching of the result was added to address security concerns, although it wasn't
	// directly requested by the RFC
	#[version(2)]
	#[raw_api]
	fn sr25519_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		out: PassFatPointerAndReadWrite<&mut [sr25519::Public]>,
	) -> u32 {
		ensure_public_keys_cache_ext_registered!(self);

		let cached = self
			.extension::<PublicKeysCacheExt>()
			.expect("`PublicKeysCacheExt` was just registered; qed")
			.sr25519
			.take()
			.and_then(|(cached_id, keys)| (cached_id == id).then_some(keys));

		let keys = match cached {
			Some(snapshot) if out.len() >= snapshot.len() => snapshot,
			_ => self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.sr25519_public_keys(id),
		};

		let key_size = core::mem::size_of::<sr25519::Public>();
		let total = keys.len();

		if out.len() >= total {
			out[..total].copy_from_slice(&keys);
		} else {
			self.extension::<PublicKeysCacheExt>()
				.expect("`PublicKeysCacheExt` is registered; qed")
				.sr25519 = Some((id, keys));
		}

		(total * key_size) as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `sr25519_public_keys` host function
	#[wrapper]
	fn sr25519_public_keys(id: KeyTypeId) -> Vec<sr25519::Public> {
		let key_size = core::mem::size_of::<sr25519::Public>();
		let num_keys = sr25519_public_keys__raw(id, &mut []) as usize / key_size;
		let mut keys = vec![sr25519::Public::default(); num_keys];
		let num_keys = sr25519_public_keys__raw(id, &mut keys) as usize / key_size;
		keys.truncate(num_keys);
		keys
	}

	/// Generate an `sr22519` key for the given key type using an optional seed and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn sr25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<sr25519::Public, 32> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_generate_new(id, seed)
			.expect("`sr25519_generate` failed")
	}

	/// Generate an `sr22519` key for the given key type using an optional seed and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Stores the public key in the provided output buffer.
	#[version(2)]
	#[raw_api]
	fn sr25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
		out: PassPointerAndWrite<&mut sr25519::Public, 32>,
	) {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		out.0.copy_from_slice(
			&self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.sr25519_generate_new(id, seed)
				.expect("`sr25519_generate` failed"),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `sr25519_generate`
	/// host function.
	#[wrapper]
	fn sr25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> sr25519::Public {
		let mut public = sr25519::Public::default();
		sr25519_generate__raw(id, seed, &mut public);
		public
	}

	/// Sign the given `msg` with the `sr25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn sr25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<sr25519::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given `msg` with the `sr25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	#[raw_api]
	fn sr25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
		out: PassPointerAndWrite<&mut sr25519::Signature, 64>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i32> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the `sr25519_sign` host
	/// function.
	#[wrapper]
	fn sr25519_sign(
		id: KeyTypeId,
		pub_key: &sr25519::Public,
		message: &[u8],
	) -> Option<sr25519::Signature> {
		let mut signature = sr25519::Signature::default();
		sr25519_sign__raw(id, pub_key, message, &mut signature).ok()?;
		Some(signature)
	}

	/// Verify an `sr25519` signature.
	///
	/// Returns `true` when the verification in successful regardless of
	/// signature version.
	fn sr25519_verify(
		sig: PassPointerAndRead<&sr25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pubkey: PassPointerAndRead<&sr25519::Public, 32>,
	) -> bool {
		sr25519::Pair::verify_deprecated(sig, msg, pubkey)
	}

	/// Returns all `ecdsa` public keys for the given key id from the keystore.
	fn ecdsa_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
	) -> AllocateAndReturnByCodec<Vec<ecdsa::Public>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_public_keys(id)
	}

	/// Stores all `ecdsa` public keys for the given key id from the keystore into the output
	/// buffer, if it is large enough. Returns the number of bytes occupied by the keys, regardless
	/// of whether the buffer was written or not.
	// ERRATA: Caching of the result was added to address security concerns, although it wasn't
	// directly requested by the RFC
	#[version(2)]
	#[raw_api]
	fn ecdsa_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		out: PassFatPointerAndReadWrite<&mut [ecdsa::Public]>,
	) -> u32 {
		ensure_public_keys_cache_ext_registered!(self);

		let cached = self
			.extension::<PublicKeysCacheExt>()
			.expect("`PublicKeysCacheExt` was just registered; qed")
			.ecdsa
			.take()
			.and_then(|(cached_id, keys)| (cached_id == id).then_some(keys));

		let keys = match cached {
			Some(snapshot) if out.len() >= snapshot.len() => snapshot,
			_ => self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ecdsa_public_keys(id),
		};

		let key_size = core::mem::size_of::<ecdsa::Public>();
		let total = keys.len();

		if out.len() >= total {
			out[..total].copy_from_slice(&keys);
		} else {
			self.extension::<PublicKeysCacheExt>()
				.expect("`PublicKeysCacheExt` is registered; qed")
				.ecdsa = Some((id, keys));
		}

		(total * key_size) as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `ecdsa_public_keys` host function
	#[wrapper]
	fn ecdsa_public_keys(id: KeyTypeId) -> Vec<ecdsa::Public> {
		let key_size = core::mem::size_of::<ecdsa::Public>();
		let num_keys = ecdsa_public_keys__raw(id, &mut []) as usize / key_size;
		let mut keys = vec![ecdsa::Public::default(); num_keys];
		let num_keys = ecdsa_public_keys__raw(id, &mut keys) as usize / key_size;
		keys.truncate(num_keys);
		keys
	}

	/// Generate an `ecdsa` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn ecdsa_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<ecdsa::Public, 33> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_generate_new(id, seed)
			.expect("`ecdsa_generate` failed")
	}

	/// Generate an `ecdsa` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Stores the public key in the provided output buffer.
	#[version(2)]
	#[raw_api]
	fn ecdsa_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
		out: PassPointerAndWrite<&mut ecdsa::Public, 33>,
	) {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		out.0.copy_from_slice(
			&self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ecdsa_generate_new(id, seed)
				.expect("`ecdsa_generate` failed"),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ecdsa_generate` host
	/// function.
	#[wrapper]
	fn ecdsa_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ecdsa::Public {
		let mut public = ecdsa::Public::default();
		ecdsa_generate__raw(id, seed, &mut public);
		public
	}

	/// Sign the given `msg` with the `ecdsa` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn ecdsa_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<ecdsa::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given `msg` with the `ecdsa` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	#[raw_api]
	fn ecdsa_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassFatPointerAndRead<&[u8]>,
		out: PassPointerAndWrite<&mut ecdsa::Signature, 65>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i32> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ecdsa_sign` host
	/// function.
	#[wrapper]
	fn ecdsa_sign(
		id: KeyTypeId,
		pub_key: &ecdsa::Public,
		message: &[u8],
	) -> Option<ecdsa::Signature> {
		let mut signature = ecdsa::Signature::default();
		ecdsa_sign__raw(id, pub_key, message, &mut signature).ok()?;
		Some(signature)
	}

	/// Sign the given a pre-hashed `msg` with the `ecdsa` key that corresponds to the given public
	/// key and key type in the keystore.
	///
	/// Returns the signature.
	// ERRATA: The RFC gathers all the *_sign_{prehashed} functions under a single definition that
	// requires `msg` to be a fat pointer which obviously doesn't make sense for a prehashed
	// message.
	fn ecdsa_sign_prehashed(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Option<ecdsa::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign_prehashed(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given a pre-hashed `msg` with the `ecdsa` key that corresponds to the given public
	/// key and key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	#[raw_api]
	fn ecdsa_sign_prehashed(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		out: PassPointerAndWrite<&mut ecdsa::Signature, 65>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i32> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign_prehashed(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `ecdsa_sign_prehashed` host function.
	#[wrapper]
	fn ecdsa_sign_prehashed(
		id: KeyTypeId,
		pub_key: &ecdsa::Public,
		msg: &[u8; 32],
	) -> Option<ecdsa::Signature> {
		let mut signature = ecdsa::Signature::default();
		ecdsa_sign_prehashed__raw(id, pub_key, msg, &mut signature).ok()?;
		Some(signature)
	}

	/// Verify `ecdsa` signature.
	///
	/// Returns `true` when the verification was successful.
	/// This version is able to handle, non-standard, overflowing signatures.
	fn ecdsa_verify(
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		#[allow(deprecated)]
		ecdsa::Pair::verify_deprecated(sig, msg, pub_key)
	}

	/// Verify `ecdsa` signature.
	///
	/// Returns `true` when the verification was successful.
	#[version(2)]
	fn ecdsa_verify(
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		ecdsa::Pair::verify(sig, msg, pub_key)
	}

	/// Verify `ecdsa` signature with pre-hashed `msg`.
	///
	/// Returns `true` when the verification was successful.
	fn ecdsa_verify_prehashed(
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		ecdsa::Pair::verify_prehashed(sig, msg, pub_key)
	}

	/// Register a `ecdsa` signature for batch verification.
	///
	/// Batch verification must be enabled by calling [`start_batch_verify`].
	/// If batch verification is not enabled, the signature will be verified immediately.
	/// To get the result of the batch verification, [`finish_batch_verify`]
	/// needs to be called.
	///
	/// Returns `true` when the verification is either successful or batched.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn ecdsa_batch_verify(
		&mut self,
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		let res = ecdsa_verify(sig, msg, pub_key);

		if let Some(ext) = self.extension::<VerificationExtDeprecated>() {
			ext.0 &= res;
		}

		res
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	/// This version is able to handle, non-standard, overflowing signatures.
	fn secp256k1_ecdsa_recover(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 64], EcdsaVerifyError>> {
		let rid = libsecp256k1::RecoveryId::parse(
			if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8,
		)
		.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[..64])
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = libsecp256k1::Message::parse(msg);
		let pubkey =
			libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
		let mut res = [0u8; 64];
		res.copy_from_slice(&pubkey.serialize()[1..65]);
		Ok(res)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	#[version(2)]
	fn secp256k1_ecdsa_recover(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 64], EcdsaVerifyError>> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		let mut res = [0u8; 64];
		res.copy_from_slice(&pubkey.serialize_uncompressed()[1..65]);
		Ok(res)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	#[version(3)]
	#[raw_api]
	fn secp256k1_ecdsa_recover(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		out: PassPointerAndWrite<&mut Pubkey512, 64>,
	) -> ConvertAndReturnAs<
		Result<(), EcdsaVerifyError>,
		RIIntResult<VoidResult, RIEcdsaVerifyError>,
		i32,
	> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		out.0.copy_from_slice(&pubkey.serialize_uncompressed()[1..]);
		Ok(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `secp256k1_ecdsa_recover` host function.
	#[wrapper]
	fn secp256k1_ecdsa_recover(
		signature: &[u8; 65],
		message: &[u8; 32],
	) -> Result<[u8; 64], EcdsaVerifyError> {
		let mut public = Pubkey512([0u8; 64]);
		secp256k1_ecdsa_recover__raw(signature, message, &mut public)?;
		Ok(public.0)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	fn secp256k1_ecdsa_recover_compressed(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 33], EcdsaVerifyError>> {
		let rid = libsecp256k1::RecoveryId::parse(
			if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8,
		)
		.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[0..64])
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = libsecp256k1::Message::parse(msg);
		let pubkey =
			libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
		Ok(pubkey.serialize_compressed())
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	#[version(2)]
	fn secp256k1_ecdsa_recover_compressed(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 33], EcdsaVerifyError>> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		Ok(pubkey.serialize())
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	#[version(3)]
	#[raw_api]
	fn secp256k1_ecdsa_recover_compressed(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		out: PassPointerAndWrite<&mut Pubkey264, 33>,
	) -> ConvertAndReturnAs<
		Result<(), EcdsaVerifyError>,
		RIIntResult<VoidResult, RIEcdsaVerifyError>,
		i32,
	> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		out.0.copy_from_slice(&pubkey.serialize());
		Ok(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `secp256k1_ecdsa_recover_compressed` host function.
	#[wrapper]
	fn secp256k1_ecdsa_recover_compressed(
		signature: &[u8; 65],
		message: &[u8; 32],
	) -> Result<[u8; 33], EcdsaVerifyError> {
		let mut public = Pubkey264([0u8; 33]);
		secp256k1_ecdsa_recover_compressed__raw(signature, message, &mut public)?;
		Ok(public.0)
	}

	/// Generate an `bls12-381` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	#[cfg(feature = "bls-experimental")]
	fn bls381_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<bls381::Public, 144> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bls381_generate_new(id, seed)
			.expect("`bls381_generate` failed")
	}

	/// Generate a 'bls12-381' Proof Of Possession for the corresponding public key.
	///
	/// Returns the Proof Of Possession as an option of the ['bls381::Signature'] type
	/// or 'None' if an error occurs.
	#[cfg(feature = "bls-experimental")]
	fn bls381_generate_proof_of_possession(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&bls381::Public, 144>,
		owner: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<bls381::ProofOfPossession>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bls381_generate_proof_of_possession(id, pub_key, owner)
			.ok()
			.flatten()
	}

	/// Generate combination `ecdsa & bls12-381` key for the given key type using an optional `seed`
	/// and store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	#[cfg(feature = "bls-experimental")]
	fn ecdsa_bls381_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<ecdsa_bls381::Public, { 144 + 33 }> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_bls381_generate_new(id, seed)
			.expect("`ecdsa_bls381_generate` failed")
	}

	/// Generate a `bandersnatch` key pair for the given key type using an optional
	/// `seed` and store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	#[cfg(feature = "bandersnatch-experimental")]
	fn bandersnatch_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<bandersnatch::Public, 32> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bandersnatch_generate_new(id, seed)
			.expect("`bandernatch_generate` failed")
	}

	/// Sign the given `msg` with the `bandersnatch` key that corresponds to the given public key
	/// and key type in the keystore.
	///
	/// Returns the signature or `None` if an error occurred.
	#[cfg(feature = "bandersnatch-experimental")]
	fn bandersnatch_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&bandersnatch::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<bandersnatch::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bandersnatch_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}
}
