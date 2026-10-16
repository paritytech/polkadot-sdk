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

//! Keystore host calls (indices 328-337) and in-guest signature verification.
//!
//! The keystore lives on the node, so `{ed25519,sr25519,ecdsa}_{generate,public_keys,sign}` and
//! `ecdsa_sign_prehashed` are host calls following the version-2 raw API of sdk3
//! `host_functions/crypto.rs`: the runtime passes output buffers and the node writes into them,
//! never allocating runtime memory. Signature verification and the `secp256k1` recovery helpers
//! run inside the runtime itself, copied from sdk3 `native/crypto.rs`.
//!
//! The `*_public_keys` host calls implement the two-pass probe-then-fetch contract: a call whose
//! output buffer is too small returns the full byte count without writing, and caches the
//! snapshot in [`PublicKeysCacheExt`] so the follow-up call with a large enough buffer sees a
//! consistent view of the keystore.
//!
//! The `*_batch_verify` functions are `register_only` on master and are therefore not provided;
//! neither are the bls/bandersnatch keystore functions. There is deliberately no `verify` host
//! call.

use alloc::{vec, vec::Vec};

use sp_core::{
	crypto::{KeyTypeId, Pair},
	ecdsa, ed25519, sr25519,
};

#[cfg(not(substrate_runtime))]
use super::with_externalities;
#[cfg(substrate_runtime)]
use super::{optional_slice, void_result_from_host};
#[cfg(not(substrate_runtime))]
use super::{read_optional_slice, void_result_into_ffi};
#[cfg(not(substrate_runtime))]
use sp_externalities::ExternalitiesExt;
#[cfg(not(substrate_runtime))]
use sp_jam_runtime_interface::FunctionContext;
#[cfg(not(substrate_runtime))]
use sp_keystore::KeystoreExt;

use crate::jam::EcdsaVerifyError;

/// Per-algorithm snapshots of public keys retained by [`PublicKeysCacheExt`].
///
/// At most one snapshot is cached per algorithm — the one populated by the most recent
/// `*_public_keys` host call whose output buffer was too small to receive the full result. The
/// next call with a sufficiently large buffer drains the snapshot, giving the two-call
/// probe-then-fetch pattern of the convenience wrappers a consistent view of the keystore.
#[cfg(not(substrate_runtime))]
#[derive(Default)]
pub struct PublicKeysCache {
	/// Cached `ed25519` public keys for one key type.
	pub ed25519: Option<(KeyTypeId, Vec<ed25519::Public>)>,
	/// Cached `sr25519` public keys for one key type.
	pub sr25519: Option<(KeyTypeId, Vec<sr25519::Public>)>,
	/// Cached `ecdsa` public keys for one key type.
	pub ecdsa: Option<(KeyTypeId, Vec<ecdsa::Public>)>,
}

#[cfg(not(substrate_runtime))]
sp_externalities::decl_extension! {
	/// Externalities extension backing the [`PublicKeysCache`].
	pub struct PublicKeysCacheExt(PublicKeysCache);
}

#[cfg(not(substrate_runtime))]
impl Default for PublicKeysCacheExt {
	fn default() -> Self {
		Self(PublicKeysCache::default())
	}
}

/// Registers [`PublicKeysCacheExt`] on the given externalities if it is not registered yet.
///
/// NOTE: sdk3 keeps this macro in `lib.rs`; the jam module cannot touch `lib.rs`, so it lives
/// here, only used by this module's host-call node bodies.
#[cfg(not(substrate_runtime))]
macro_rules! ensure_public_keys_cache_ext_registered {
	($ext:expr) => {
		match $ext.register_extension(PublicKeysCacheExt::default()) {
			Ok(()) | Err(sp_externalities::Error::ExtensionAlreadyRegistered) => (),
			Err(e) => panic!("Failed to register `PublicKeysCacheExt`: {e:?}"),
		}
	};
}

/// Declares the node logic of the `generate`, `public_keys` and `sign` keystore host calls
/// of one algorithm in the module `$module`.
#[cfg(not(substrate_runtime))]
macro_rules! keystore_node {
	($module:ident, $alg:ident, $generate:ident, $public_keys:ident, $sign:ident) => {
		mod $module {
			use super::*;

			/// Node logic of `ext_crypto_*_generate`.
			pub fn generate(id: KeyTypeId, seed: Option<&[u8]>) -> $alg::Public {
				let seed = seed.map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
				with_externalities(|mut ext| {
					ext.extension::<KeystoreExt>()
						.expect("No `keystore` associated for the current context!")
						.$generate(id, seed)
						.expect(concat!("`", stringify!($alg), "_generate` failed"))
				})
			}

			/// Node logic of `ext_crypto_*_public_keys`.
			pub fn public_keys(id: KeyTypeId, out: &mut [u8]) -> u32 {
				with_externalities(|mut ext| {
					ensure_public_keys_cache_ext_registered!(ext);

					let key_size = core::mem::size_of::<$alg::Public>();
					let cached = ext
						.extension::<PublicKeysCacheExt>()
						.expect("`PublicKeysCacheExt` was just registered; qed")
						.$alg
						.take()
						.and_then(|(cached_id, keys)| (cached_id == id).then_some(keys));

					let keys = match cached {
						Some(snapshot) if out.len() >= snapshot.len() * key_size => snapshot,
						_ => ext
							.extension::<KeystoreExt>()
							.expect("No `keystore` associated for the current context!")
							.$public_keys(id),
					};

					let total = keys.len();

					if out.len() >= total * key_size {
						for (i, key) in keys.iter().enumerate() {
							out[i * key_size..(i + 1) * key_size].copy_from_slice(key.as_ref());
						}
					} else {
						ext.extension::<PublicKeysCacheExt>()
							.expect("`PublicKeysCacheExt` is registered; qed")
							.$alg = Some((id, keys));
					}

					(total * key_size) as u32
				})
			}

			/// Node logic of `ext_crypto_*_sign`.
			pub fn sign(
				id: KeyTypeId,
				pub_key: &$alg::Public,
				msg: &[u8],
			) -> Option<$alg::Signature> {
				with_externalities(|mut ext| {
					ext.extension::<KeystoreExt>()
						.expect("No `keystore` associated for the current context!")
						.$sign(id, pub_key, msg)
						.ok()
						.flatten()
				})
			}
		}
	};
}

#[cfg(not(substrate_runtime))]
keystore_node!(ed25519_node, ed25519, ed25519_generate_new, ed25519_public_keys, ed25519_sign);
#[cfg(not(substrate_runtime))]
keystore_node!(sr25519_node, sr25519, sr25519_generate_new, sr25519_public_keys, sr25519_sign);
#[cfg(not(substrate_runtime))]
keystore_node!(ecdsa_node, ecdsa, ecdsa_generate_new, ecdsa_public_keys, ecdsa_sign);

/// Node logic of `ext_crypto_ecdsa_sign_prehashed`.
#[cfg(not(substrate_runtime))]
fn node_ecdsa_sign_prehashed(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	msg: &[u8; 32],
) -> Option<ecdsa::Signature> {
	with_externalities(|mut ext| {
		ext.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign_prehashed(id, pub_key, msg)
			.ok()
			.flatten()
	})
}

/// Converts the FFI representation of a [`KeyTypeId`].
#[cfg(not(substrate_runtime))]
fn key_type_id(id: u32) -> KeyTypeId {
	KeyTypeId(id.to_le_bytes())
}

/// Converts a [`KeyTypeId`] into its FFI representation.
#[cfg(substrate_runtime)]
fn key_type_id_into_ffi(id: KeyTypeId) -> u32 {
	u32::from_le_bytes(id.0)
}

/// Writes the outcome of a `sign` host call into `out`.
#[cfg(not(substrate_runtime))]
fn write_signature(
	ctx: &mut dyn sp_jam_runtime_interface::FunctionContext,
	out: *mut u8,
	signature: Option<impl AsRef<[u8]>>,
) -> Result<i32, alloc::string::String> {
	if let Some(signature) = &signature {
		ctx.write_memory(out, signature.as_ref())?;
	}
	Ok(void_result_into_ffi(signature.map(drop).ok_or(())))
}

sp_jam_runtime_interface::host_calls! {
	/// Generate an `ed25519` key for the given key type using an optional `seed` (valid utf8)
	/// and store it in the keystore, writing the 32-byte public key into `out`.
	///
	/// `id` is the little endian `KeyTypeId` and a null `seed` pointer means no seed.
	#[index = 328]
	fn ext_crypto_ed25519_generate(ctx: &mut dyn FunctionContext, id: u32, seed: *const u8, seed_len: u64, out: *mut u8) {
		let seed = read_optional_slice(ctx, seed, seed_len)?;
		let public = ed25519_node::generate(key_type_id(id), seed.as_deref());
		ctx.write_memory(out, public.as_ref())
	}

	/// Store all `ed25519` public keys for the given key id from the keystore into the output
	/// buffer, if it is large enough. Returns the number of bytes occupied by the keys,
	/// regardless of whether the buffer was written or not.
	#[index = 329]
	fn ext_crypto_ed25519_public_keys(ctx: &mut dyn FunctionContext, id: u32, out: *mut u8, out_len: u64) -> u32 {
		ctx.with_buffer(out, out_len, |out| ed25519_node::public_keys(key_type_id(id), out))
	}

	/// Sign the given `msg` with the `ed25519` key that corresponds to the given 32-byte public
	/// key and key type in the keystore, writing the 64-byte signature into `out`. `0` is
	/// `Ok`, `-1` is `Err`.
	#[index = 330]
	fn ext_crypto_ed25519_sign(
		ctx: &mut dyn FunctionContext,
		id: u32,
		pub_key: *const u8,
		msg: *const u8,
		msg_len: u64,
		out: *mut u8,
	) -> i32 {
		let pub_key = ed25519::Public::from_raw(ctx.read_array(pub_key)?);
		let msg = ctx.read_memory(msg, msg_len)?;
		let signature = ed25519_node::sign(key_type_id(id), &pub_key, &msg);
		write_signature(ctx, out, signature)
	}

	/// Generate an `sr25519` key; see [`ext_crypto_ed25519_generate`].
	#[index = 331]
	fn ext_crypto_sr25519_generate(ctx: &mut dyn FunctionContext, id: u32, seed: *const u8, seed_len: u64, out: *mut u8) {
		let seed = read_optional_slice(ctx, seed, seed_len)?;
		let public = sr25519_node::generate(key_type_id(id), seed.as_deref());
		ctx.write_memory(out, public.as_ref())
	}

	/// Store all `sr25519` public keys for the given key id into the output buffer; see
	/// [`ext_crypto_ed25519_public_keys`].
	#[index = 332]
	fn ext_crypto_sr25519_public_keys(ctx: &mut dyn FunctionContext, id: u32, out: *mut u8, out_len: u64) -> u32 {
		ctx.with_buffer(out, out_len, |out| sr25519_node::public_keys(key_type_id(id), out))
	}

	/// Sign the given `msg` with the `sr25519` key; see [`ext_crypto_ed25519_sign`].
	#[index = 333]
	fn ext_crypto_sr25519_sign(
		ctx: &mut dyn FunctionContext,
		id: u32,
		pub_key: *const u8,
		msg: *const u8,
		msg_len: u64,
		out: *mut u8,
	) -> i32 {
		let pub_key = sr25519::Public::from_raw(ctx.read_array(pub_key)?);
		let msg = ctx.read_memory(msg, msg_len)?;
		let signature = sr25519_node::sign(key_type_id(id), &pub_key, &msg);
		write_signature(ctx, out, signature)
	}

	/// Generate an `ecdsa` key, writing the 33-byte public key into `out`; see
	/// [`ext_crypto_ed25519_generate`].
	#[index = 334]
	fn ext_crypto_ecdsa_generate(ctx: &mut dyn FunctionContext, id: u32, seed: *const u8, seed_len: u64, out: *mut u8) {
		let seed = read_optional_slice(ctx, seed, seed_len)?;
		let public = ecdsa_node::generate(key_type_id(id), seed.as_deref());
		ctx.write_memory(out, public.as_ref())
	}

	/// Store all `ecdsa` public keys for the given key id into the output buffer; see
	/// [`ext_crypto_ed25519_public_keys`].
	#[index = 335]
	fn ext_crypto_ecdsa_public_keys(ctx: &mut dyn FunctionContext, id: u32, out: *mut u8, out_len: u64) -> u32 {
		ctx.with_buffer(out, out_len, |out| ecdsa_node::public_keys(key_type_id(id), out))
	}

	/// Sign the given `msg` with the `ecdsa` key (33-byte public key), writing the 65-byte
	/// signature into `out`; see [`ext_crypto_ed25519_sign`].
	#[index = 336]
	fn ext_crypto_ecdsa_sign(
		ctx: &mut dyn FunctionContext,
		id: u32,
		pub_key: *const u8,
		msg: *const u8,
		msg_len: u64,
		out: *mut u8,
	) -> i32 {
		let pub_key = ecdsa::Public::from_raw(ctx.read_array(pub_key)?);
		let msg = ctx.read_memory(msg, msg_len)?;
		let signature = ecdsa_node::sign(key_type_id(id), &pub_key, &msg);
		write_signature(ctx, out, signature)
	}

	/// Sign the given pre-hashed 32-byte `msg` with the `ecdsa` key that corresponds to the
	/// given 33-byte public key and key type in the keystore, writing the 65-byte signature into
	/// `out`. `0` is `Ok`, `-1` is `Err`.
	#[index = 337]
	fn ext_crypto_ecdsa_sign_prehashed(ctx: &mut dyn FunctionContext, id: u32, pub_key: *const u8, msg: *const u8, out: *mut u8) -> i32 {
		let pub_key = ecdsa::Public::from_raw(ctx.read_array(pub_key)?);
		let msg = ctx.read_array(msg)?;
		let signature = node_ecdsa_sign_prehashed(key_type_id(id), &pub_key, &msg);
		write_signature(ctx, out, signature)
	}
}

/// Fetches public keys through a `public_keys` host call, which gets the output buffer and
/// returns the number of bytes occupied by the keys.
fn public_keys<P: From<[u8; N]>, const N: usize>(
	mut fetch: impl FnMut(&mut [u8]) -> u32,
) -> Vec<P> {
	let num_bytes = fetch(&mut []) as usize;
	let mut buffer = vec![0u8; num_bytes];
	let num_bytes = fetch(&mut buffer) as usize;
	buffer.truncate(num_bytes);
	buffer
		.chunks_exact(N)
		.map(|chunk| {
			P::from(
				<&[u8] as TryInto<[u8; N]>>::try_into(chunk)
					.expect("`chunks_exact` yields `N` byte chunks; qed"),
			)
		})
		.collect()
}

/// Returns all `ed25519` public keys for the given key id from the keystore.
pub fn ed25519_public_keys(id: KeyTypeId) -> Vec<ed25519::Public> {
	public_keys::<ed25519::Public, 32>(|out| {
		#[cfg(substrate_runtime)]
		// SAFETY: The pointer and length come from a live slice.
		return unsafe {
			ext_crypto_ed25519_public_keys(
				key_type_id_into_ffi(id),
				out.as_mut_ptr(),
				out.len() as u64,
			)
		};
		#[cfg(not(substrate_runtime))]
		ed25519_node::public_keys(id, out)
	})
}

/// Generate an `ed25519` key for the given key type using an optional `seed` and store it in
/// the keystore.
///
/// The `seed` needs to be a valid utf8.
///
/// Returns the public key.
pub fn ed25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ed25519::Public {
	#[cfg(substrate_runtime)]
	{
		let mut public = ed25519::Public::default();
		let (seed, seed_len) = optional_slice(seed.as_deref());
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		unsafe {
			ext_crypto_ed25519_generate(
				key_type_id_into_ffi(id),
				seed,
				seed_len,
				AsMut::<[u8]>::as_mut(&mut public).as_mut_ptr(),
			)
		};
		public
	}
	#[cfg(not(substrate_runtime))]
	ed25519_node::generate(id, seed.as_deref())
}

/// Sign the given `msg` with the `ed25519` key that corresponds to the given public key and key
/// type in the keystore.
///
/// Returns the signature.
pub fn ed25519_sign(
	id: KeyTypeId,
	pub_key: &ed25519::Public,
	message: &[u8],
) -> Option<ed25519::Signature> {
	#[cfg(substrate_runtime)]
	{
		let mut signature = ed25519::Signature::default();
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		void_result_from_host(unsafe {
			ext_crypto_ed25519_sign(
				key_type_id_into_ffi(id),
				AsRef::<[u8]>::as_ref(pub_key).as_ptr(),
				message.as_ptr(),
				message.len() as u64,
				AsMut::<[u8]>::as_mut(&mut signature).as_mut_ptr(),
			)
		})
		.ok()?;
		Some(signature)
	}
	#[cfg(not(substrate_runtime))]
	ed25519_node::sign(id, pub_key, message)
}

/// Verify an `ed25519` signature.
///
/// Returns `true` when the verification was successful.
pub fn ed25519_verify(sig: &ed25519::Signature, msg: &[u8], pub_key: &ed25519::Public) -> bool {
	#[cfg(not(substrate_runtime))]
	if sp_externalities::with_externalities(|mut ext| {
		ext.extension::<crate::jam::UseDalekExt>().is_some()
	})
	.unwrap_or_default()
	{
		use ed25519_dalek::Verifier;

		let Ok(public_key) = ed25519_dalek::VerifyingKey::from_bytes(&pub_key.0) else {
			return false;
		};
		return public_key.verify(msg, &ed25519_dalek::Signature::from_bytes(&sig.0)).is_ok();
	}
	ed25519::Pair::verify(sig, msg, pub_key)
}

/// Returns all `sr25519` public keys for the given key id from the keystore.
pub fn sr25519_public_keys(id: KeyTypeId) -> Vec<sr25519::Public> {
	public_keys::<sr25519::Public, 32>(|out| {
		#[cfg(substrate_runtime)]
		// SAFETY: The pointer and length come from a live slice.
		return unsafe {
			ext_crypto_sr25519_public_keys(
				key_type_id_into_ffi(id),
				out.as_mut_ptr(),
				out.len() as u64,
			)
		};
		#[cfg(not(substrate_runtime))]
		sr25519_node::public_keys(id, out)
	})
}

/// Generate an `sr25519` key for the given key type using an optional `seed` and store it in
/// the keystore.
///
/// The `seed` needs to be a valid utf8.
///
/// Returns the public key.
pub fn sr25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> sr25519::Public {
	#[cfg(substrate_runtime)]
	{
		let mut public = sr25519::Public::default();
		let (seed, seed_len) = optional_slice(seed.as_deref());
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		unsafe {
			ext_crypto_sr25519_generate(
				key_type_id_into_ffi(id),
				seed,
				seed_len,
				AsMut::<[u8]>::as_mut(&mut public).as_mut_ptr(),
			)
		};
		public
	}
	#[cfg(not(substrate_runtime))]
	sr25519_node::generate(id, seed.as_deref())
}

/// Sign the given `msg` with the `sr25519` key that corresponds to the given public key and key
/// type in the keystore.
///
/// Returns the signature.
pub fn sr25519_sign(
	id: KeyTypeId,
	pub_key: &sr25519::Public,
	message: &[u8],
) -> Option<sr25519::Signature> {
	#[cfg(substrate_runtime)]
	{
		let mut signature = sr25519::Signature::default();
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		void_result_from_host(unsafe {
			ext_crypto_sr25519_sign(
				key_type_id_into_ffi(id),
				AsRef::<[u8]>::as_ref(pub_key).as_ptr(),
				message.as_ptr(),
				message.len() as u64,
				AsMut::<[u8]>::as_mut(&mut signature).as_mut_ptr(),
			)
		})
		.ok()?;
		Some(signature)
	}
	#[cfg(not(substrate_runtime))]
	sr25519_node::sign(id, pub_key, message)
}

/// Verify an `sr25519` signature.
///
/// Returns `true` when the verification was successful.
pub fn sr25519_verify(sig: &sr25519::Signature, msg: &[u8], pub_key: &sr25519::Public) -> bool {
	sr25519::Pair::verify(sig, msg, pub_key)
}

/// Returns all `ecdsa` public keys for the given key id from the keystore.
pub fn ecdsa_public_keys(id: KeyTypeId) -> Vec<ecdsa::Public> {
	public_keys::<ecdsa::Public, 33>(|out| {
		#[cfg(substrate_runtime)]
		// SAFETY: The pointer and length come from a live slice.
		return unsafe {
			ext_crypto_ecdsa_public_keys(
				key_type_id_into_ffi(id),
				out.as_mut_ptr(),
				out.len() as u64,
			)
		};
		#[cfg(not(substrate_runtime))]
		ecdsa_node::public_keys(id, out)
	})
}

/// Generate an `ecdsa` key for the given key type using an optional `seed` and store it in the
/// keystore.
///
/// The `seed` needs to be a valid utf8.
///
/// Returns the public key.
pub fn ecdsa_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ecdsa::Public {
	#[cfg(substrate_runtime)]
	{
		let mut public = ecdsa::Public::default();
		let (seed, seed_len) = optional_slice(seed.as_deref());
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		unsafe {
			ext_crypto_ecdsa_generate(
				key_type_id_into_ffi(id),
				seed,
				seed_len,
				AsMut::<[u8]>::as_mut(&mut public).as_mut_ptr(),
			)
		};
		public
	}
	#[cfg(not(substrate_runtime))]
	ecdsa_node::generate(id, seed.as_deref())
}

/// Sign the given `msg` with the `ecdsa` key that corresponds to the given public key and key
/// type in the keystore.
///
/// Returns the signature.
pub fn ecdsa_sign(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	message: &[u8],
) -> Option<ecdsa::Signature> {
	#[cfg(substrate_runtime)]
	{
		let mut signature = ecdsa::Signature::default();
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		void_result_from_host(unsafe {
			ext_crypto_ecdsa_sign(
				key_type_id_into_ffi(id),
				AsRef::<[u8]>::as_ref(pub_key).as_ptr(),
				message.as_ptr(),
				message.len() as u64,
				AsMut::<[u8]>::as_mut(&mut signature).as_mut_ptr(),
			)
		})
		.ok()?;
		Some(signature)
	}
	#[cfg(not(substrate_runtime))]
	ecdsa_node::sign(id, pub_key, message)
}

/// Sign the given pre-hashed `msg` with the `ecdsa` key that corresponds to the given public
/// key and key type in the keystore.
///
/// Returns the signature.
pub fn ecdsa_sign_prehashed(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	msg: &[u8; 32],
) -> Option<ecdsa::Signature> {
	#[cfg(substrate_runtime)]
	{
		let mut signature = ecdsa::Signature::default();
		// SAFETY: The pointers come from live values of the sizes the host call expects.
		void_result_from_host(unsafe {
			ext_crypto_ecdsa_sign_prehashed(
				key_type_id_into_ffi(id),
				AsRef::<[u8]>::as_ref(pub_key).as_ptr(),
				msg.as_ptr(),
				AsMut::<[u8]>::as_mut(&mut signature).as_mut_ptr(),
			)
		})
		.ok()?;
		Some(signature)
	}
	#[cfg(not(substrate_runtime))]
	node_ecdsa_sign_prehashed(id, pub_key, msg)
}

/// Verify an `ecdsa` signature.
///
/// Returns `true` when the verification was successful.
pub fn ecdsa_verify(sig: &ecdsa::Signature, msg: &[u8], pub_key: &ecdsa::Public) -> bool {
	ecdsa::Pair::verify(sig, msg, pub_key)
}

/// Verify an `ecdsa` signature with a pre-hashed `msg`.
///
/// Returns `true` when the verification was successful.
pub fn ecdsa_verify_prehashed(
	sig: &ecdsa::Signature,
	msg: &[u8; 32],
	pub_key: &ecdsa::Public,
) -> bool {
	ecdsa::Pair::verify_prehashed(sig, msg, pub_key)
}

/// Verify and recover a SECP256k1 ECDSA signature.
///
/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
/// - `msg` is the blake2-256 hash of the message.
///
/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey (doesn't include the
/// 0x04 prefix).
pub fn secp256k1_ecdsa_recover(
	sig: &[u8; 65],
	msg: &[u8; 32],
) -> Result<[u8; 64], EcdsaVerifyError> {
	let rid =
		libsecp256k1::RecoveryId::parse(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8)
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
/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
pub fn secp256k1_ecdsa_recover_compressed(
	sig: &[u8; 65],
	msg: &[u8; 32],
) -> Result<[u8; 33], EcdsaVerifyError> {
	let rid =
		libsecp256k1::RecoveryId::parse(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8)
			.map_err(|_| EcdsaVerifyError::BadV)?;
	let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[..64])
		.map_err(|_| EcdsaVerifyError::BadRS)?;
	let msg = libsecp256k1::Message::parse(msg);
	let pubkey =
		libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
	Ok(pubkey.serialize_compressed())
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	use super::*;
	use crate::TestExternalities;
	use sp_keystore::{testing::MemoryKeystore, KeystoreExt};

	const ED25519: KeyTypeId = KeyTypeId(*b"ed25");
	const SR25519: KeyTypeId = KeyTypeId(*b"sr25");
	const ECDSA: KeyTypeId = KeyTypeId(*b"ecds");

	fn keystore_ext() -> TestExternalities {
		let mut ext = TestExternalities::default();
		ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
		ext
	}

	#[test]
	fn ed25519_generate_public_keys_sign_verify() {
		keystore_ext().execute_with(|| {
			let public = ed25519_generate(ED25519, Some(b"//Alice".to_vec()));
			let other = ed25519_generate(ED25519, Some(b"//Bob".to_vec()));

			let keys = ed25519_public_keys(ED25519);
			assert_eq!(keys.len(), 2);
			assert!(keys.contains(&public));
			assert!(keys.contains(&other));

			let msg = b"test message";
			let signature = ed25519_sign(ED25519, &public, msg).expect("key was just generated");
			assert!(ed25519_verify(&signature, msg, &public));

			// Tampered signatures do not verify.
			let mut tampered = signature;
			tampered.0[0] ^= 0xff;
			assert!(!ed25519_verify(&tampered, msg, &public));
			assert!(!ed25519_verify(&signature, b"other message", &public));
			assert!(!ed25519_verify(&signature, msg, &other));
		});
	}

	#[test]
	fn ed25519_dalek_extension_selects_compatible_verification() {
		let mut ext = keystore_ext();
		ext.register_extension(crate::jam::UseDalekExt::default());
		ext.execute_with(|| {
			let public = ed25519_generate(ED25519, Some(b"//Alice".to_vec()));
			let signature = ed25519_sign(ED25519, &public, b"compatibility").expect("key exists");
			assert!(ed25519_verify(&signature, b"compatibility", &public));
		});
	}

	#[test]
	fn sr25519_generate_public_keys_sign_verify() {
		keystore_ext().execute_with(|| {
			let public = sr25519_generate(SR25519, Some(b"//Alice".to_vec()));
			let other = sr25519_generate(SR25519, Some(b"//Bob".to_vec()));

			let keys = sr25519_public_keys(SR25519);
			assert_eq!(keys.len(), 2);
			assert!(keys.contains(&public));
			assert!(keys.contains(&other));

			let msg = b"test message";
			let signature = sr25519_sign(SR25519, &public, msg).expect("key was just generated");
			assert!(sr25519_verify(&signature, msg, &public));

			// Tampered signatures do not verify.
			let mut tampered = signature;
			tampered.0[0] ^= 0xff;
			assert!(!sr25519_verify(&tampered, msg, &public));
			assert!(!sr25519_verify(&signature, b"other message", &public));
			assert!(!sr25519_verify(&signature, msg, &other));
		});
	}

	#[test]
	fn ecdsa_generate_public_keys_sign_verify() {
		keystore_ext().execute_with(|| {
			let public = ecdsa_generate(ECDSA, Some(b"//Alice".to_vec()));
			let other = ecdsa_generate(ECDSA, Some(b"//Bob".to_vec()));

			let keys = ecdsa_public_keys(ECDSA);
			assert_eq!(keys.len(), 2);
			assert!(keys.contains(&public));
			assert!(keys.contains(&other));

			let msg = b"test message";
			let signature = ecdsa_sign(ECDSA, &public, msg).expect("key was just generated");
			assert!(ecdsa_verify(&signature, msg, &public));

			// Tampered signatures do not verify.
			let mut tampered = signature;
			tampered.0[0] ^= 0xff;
			assert!(!ecdsa_verify(&tampered, msg, &public));
			assert!(!ecdsa_verify(&signature, b"other message", &public));
			assert!(!ecdsa_verify(&signature, msg, &other));
		});
	}

	#[test]
	fn ecdsa_sign_prehashed_verifies() {
		keystore_ext().execute_with(|| {
			let public = ecdsa_generate(ECDSA, Some(b"//Alice".to_vec()));
			let msg = sp_crypto_hashing::blake2_256(b"test message");

			let signature =
				ecdsa_sign_prehashed(ECDSA, &public, &msg).expect("key was just generated");
			assert!(ecdsa_verify_prehashed(&signature, &msg, &public));

			let other_msg = sp_crypto_hashing::blake2_256(b"other message");
			assert!(!ecdsa_verify_prehashed(&signature, &other_msg, &public));
		});
	}

	#[test]
	fn secp256k1_ecdsa_recover_round_trip() {
		let pair = ecdsa::Pair::from_seed(b"12345678901234567890123456789012");
		let msg = sp_crypto_hashing::blake2_256(b"test message");
		let sig = pair.sign_prehashed(&msg);

		let recovered =
			secp256k1_ecdsa_recover(&sig.0, &msg).unwrap_or_else(|_| panic!("signature is valid"));
		let recovered = ecdsa::Public::from_full(&recovered).expect("64-byte full public key");
		assert_eq!(recovered, pair.public());

		let recovered = secp256k1_ecdsa_recover_compressed(&sig.0, &msg)
			.unwrap_or_else(|_| panic!("signature is valid"));
		assert_eq!(ecdsa::Public::from(recovered), pair.public());
	}

	#[test]
	fn secp256k1_ecdsa_recover_bad_signature() {
		let pair = ecdsa::Pair::from_seed(b"12345678901234567890123456789012");
		let msg = sp_crypto_hashing::blake2_256(b"test message");
		let sig = pair.sign_prehashed(&msg);

		// A bad recovery id is rejected.
		let mut bad = sig.0;
		bad[64] = 42;
		assert!(matches!(secp256k1_ecdsa_recover(&bad, &msg), Err(EcdsaVerifyError::BadV)));

		// A signature for a different message recovers a different key.
		let other_msg = sp_crypto_hashing::blake2_256(b"other message");
		let recovered = secp256k1_ecdsa_recover(&sig.0, &other_msg)
			.unwrap_or_else(|_| panic!("signature is well-formed"));
		let recovered = ecdsa::Public::from_full(&recovered).expect("64-byte full public key");
		assert_ne!(recovered, pair.public());
	}

	#[test]
	fn sign_with_unknown_key_returns_none() {
		keystore_ext().execute_with(|| {
			let unknown = ed25519::Pair::from_seed(b"12345678901234567890123456789012").public();
			assert!(ed25519_sign(ED25519, &unknown, b"test message").is_none());
		});
	}

	#[test]
	#[should_panic(expected = "No `keystore` associated for the current context!")]
	fn sign_without_keystore_panics() {
		TestExternalities::default().execute_with(|| {
			let public = ed25519::Public::default();
			let _ = ed25519_sign(ED25519, &public, b"test message");
		});
	}
}
