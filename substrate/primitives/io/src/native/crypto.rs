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

//! Native PolkaVM/JAM implementations of the `crypto` interface.

use crate::*;
use alloc::{vec, vec::Vec};
use libsecp256k1::Message;
#[cfg(feature = "bandersnatch-experimental")]
use sp_core::bandersnatch;
#[cfg(feature = "bls-experimental")]
use sp_core::{bls381, ecdsa_bls381};
use sp_core::{
	crypto::{KeyTypeId, Pair},
	ecdsa, ed25519, sr25519,
};
use sp_runtime_interface::pack_ptr_and_len;

// Forwarding externs for the keystore-dependent `crypto` host calls. The keystore lives on the
// node: on riscv the runtime forwards these calls through `ecalli` and the node's
// `crypto::HostFunctions` serve them from the authoritative `KeystoreExt` (the version-2 raw API
// in `host_functions/crypto.rs`, where the `PublicKeysCacheExt` result caching also lives). Each
// extern's name byte-matches the host-registered `ext_crypto_<fn>_version_2` function so the
// node-side polkavm linker resolves the import by symbol (`polkavm::Linker::instantiate_pre`);
// the index fixes the blob import slot. Keep the allocation in `host_functions/mod.rs` in sync;
// nothing may pass 343 in this change.
#[polkavm_derive::polkavm_import]
extern "C" {
	/// Forward `crypto::ed25519_generate_version_2`: generate an ed25519 key in the node
	/// keystore and write the 32-byte public key into `out`.
	#[polkavm_import(index = 328)]
	fn ext_crypto_ed25519_generate_version_2(id_ptr: u32, seed: u64, out_ptr: u32);

	/// Forward `crypto::ed25519_public_keys_version_2`: write all ed25519 public keys for `id`
	/// into `out`, returning the full byte count whether or not the buffer was written.
	#[polkavm_import(index = 329)]
	fn ext_crypto_ed25519_public_keys_version_2(id_ptr: u32, out_ptr_len: u64) -> u32;

	/// Forward `crypto::ed25519_sign_version_2`: sign `msg` with the ed25519 key matching
	/// `pub_key` and write the 64-byte signature into `out`; `0` is `Ok`, any other value `Err`.
	#[polkavm_import(index = 330)]
	fn ext_crypto_ed25519_sign_version_2(
		id_ptr: u32,
		pub_key_ptr: u32,
		msg_ptr_len: u64,
		out_ptr: u32,
	) -> i32;

	/// Forward `crypto::sr25519_generate_version_2`; see `ext_crypto_ed25519_generate_version_2`.
	#[polkavm_import(index = 331)]
	fn ext_crypto_sr25519_generate_version_2(id_ptr: u32, seed: u64, out_ptr: u32);

	/// Forward `crypto::sr25519_public_keys_version_2`; see the ed25519 analog.
	#[polkavm_import(index = 332)]
	fn ext_crypto_sr25519_public_keys_version_2(id_ptr: u32, out_ptr_len: u64) -> u32;

	/// Forward `crypto::sr25519_sign_version_2`; see `ext_crypto_ed25519_sign_version_2`.
	#[polkavm_import(index = 333)]
	fn ext_crypto_sr25519_sign_version_2(
		id_ptr: u32,
		pub_key_ptr: u32,
		msg_ptr_len: u64,
		out_ptr: u32,
	) -> i32;

	/// Forward `crypto::ecdsa_generate_version_2`: write the 33-byte ecdsa public key into `out`.
	#[polkavm_import(index = 334)]
	fn ext_crypto_ecdsa_generate_version_2(id_ptr: u32, seed: u64, out_ptr: u32);

	/// Forward `crypto::ecdsa_public_keys_version_2`; see `ext_crypto_ed25519_public_keys_version_2`.
	#[polkavm_import(index = 335)]
	fn ext_crypto_ecdsa_public_keys_version_2(id_ptr: u32, out_ptr_len: u64) -> u32;

	/// Forward `crypto::ecdsa_sign_version_2`: write the 65-byte ecdsa signature into `out`.
	#[polkavm_import(index = 336)]
	fn ext_crypto_ecdsa_sign_version_2(
		id_ptr: u32,
		pub_key_ptr: u32,
		msg_ptr_len: u64,
		out_ptr: u32,
	) -> i32;

	/// Forward `crypto::ecdsa_sign_prehashed_version_2`: sign the 32-byte pre-hashed `msg`
	/// (passed by pointer, not by fat pointer) with the ecdsa key matching `pub_key`.
	#[polkavm_import(index = 337)]
	fn ext_crypto_ecdsa_sign_prehashed_version_2(
		id_ptr: u32,
		pub_key_ptr: u32,
		msg_ptr: u32,
		out_ptr: u32,
	) -> i32;
}
/// Native PolkaVM/JAM implementation of `bandersnatch_generate`.
#[cfg(feature = "bandersnatch-experimental")]
pub fn bandersnatch_generate(_id: KeyTypeId, _seed: Option<Vec<u8>>) -> bandersnatch::Public {
	panic!(
		"`crypto::bandersnatch_generate` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `bandersnatch_sign`.
#[cfg(feature = "bandersnatch-experimental")]
pub fn bandersnatch_sign(
	_id: KeyTypeId,
	_pub_key: &bandersnatch::Public,
	_msg: &[u8],
) -> Option<bandersnatch::Signature> {
	panic!("`crypto::bandersnatch_sign` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `bls381_generate`.
#[cfg(feature = "bls-experimental")]
pub fn bls381_generate(_id: KeyTypeId, _seed: Option<Vec<u8>>) -> bls381::Public {
	panic!("`crypto::bls381_generate` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `bls381_generate_proof_of_possession`.
#[cfg(feature = "bls-experimental")]
pub fn bls381_generate_proof_of_possession(
	_id: KeyTypeId,
	_pub_key: &bls381::Public,
	_owner: &[u8],
) -> Option<bls381::ProofOfPossession> {
	panic!("`crypto::bls381_generate_proof_of_possession` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `ecdsa_bls381_generate`.
#[cfg(feature = "bls-experimental")]
pub fn ecdsa_bls381_generate(_id: KeyTypeId, _seed: Option<Vec<u8>>) -> ecdsa_bls381::Public {
	panic!(
		"`crypto::ecdsa_bls381_generate` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `ecdsa_generate__raw`.
pub fn ecdsa_generate__raw(id: KeyTypeId, seed: Option<Vec<u8>>, out: &mut ecdsa::Public) {
	let seed = seed.encode();
	let out_mut: &mut [u8] = (*out).as_mut();
	unsafe {
		ext_crypto_ecdsa_generate_version_2(
			id.as_ref().as_ptr() as u32,
			pack_ptr_and_len(seed.as_ptr() as u32, seed.len() as u32),
			out_mut.as_ptr() as u32,
		)
	}
}

/// Native PolkaVM/JAM implementation of `ecdsa_public_keys__raw`.
pub fn ecdsa_public_keys__raw(id: KeyTypeId, out: &mut [ecdsa::Public]) -> u32 {
	unsafe {
		ext_crypto_ecdsa_public_keys_version_2(
			id.as_ref().as_ptr() as u32,
			pack_ptr_and_len(out.as_ptr() as u32, core::mem::size_of_val(out) as u32),
		)
	}
}

/// Native PolkaVM/JAM implementation of `ecdsa_sign__raw`.
pub fn ecdsa_sign__raw(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	msg: &[u8],
	out: &mut ecdsa::Signature,
) -> Result<(), ()> {
	let pub_key_ref: &[u8] = (*pub_key).as_ref();
	let out_mut: &mut [u8] = (*out).as_mut();
	let result = unsafe {
		ext_crypto_ecdsa_sign_version_2(
			id.as_ref().as_ptr() as u32,
			pub_key_ref.as_ptr() as u32,
			pack_ptr_and_len(msg.as_ptr() as u32, msg.len() as u32),
			out_mut.as_ptr() as u32,
		)
	};
	if result == 0 {
		Ok(())
	} else {
		Err(())
	}
}

/// Native PolkaVM/JAM implementation of `ecdsa_sign_prehashed__raw`.
pub fn ecdsa_sign_prehashed__raw(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	msg: &[u8; 32],
	out: &mut ecdsa::Signature,
) -> Result<(), ()> {
	let pub_key_ref: &[u8] = (*pub_key).as_ref();
	let msg_ref: &[u8] = (*msg).as_ref();
	let out_mut: &mut [u8] = (*out).as_mut();
	let result = unsafe {
		ext_crypto_ecdsa_sign_prehashed_version_2(
			id.as_ref().as_ptr() as u32,
			pub_key_ref.as_ptr() as u32,
			msg_ref.as_ptr() as u32,
			out_mut.as_ptr() as u32,
		)
	};
	if result == 0 {
		Ok(())
	} else {
		Err(())
	}
}

/// Native PolkaVM/JAM implementation of `ecdsa_verify`.
pub fn ecdsa_verify(sig: &ecdsa::Signature, msg: &[u8], pub_key: &ecdsa::Public) -> bool {
	ecdsa::Pair::verify(sig, msg, pub_key)
}

/// Native PolkaVM/JAM implementation of `ecdsa_verify_prehashed`.
pub fn ecdsa_verify_prehashed(
	sig: &ecdsa::Signature,
	msg: &[u8; 32],
	pub_key: &ecdsa::Public,
) -> bool {
	ecdsa::Pair::verify_prehashed(sig, msg, pub_key)
}

/// Native PolkaVM/JAM implementation of `ed25519_generate__raw`.
pub fn ed25519_generate__raw(id: KeyTypeId, seed: Option<Vec<u8>>, out: &mut ed25519::Public) {
	let seed = seed.encode();
	let out_mut: &mut [u8] = (*out).as_mut();
	unsafe {
		ext_crypto_ed25519_generate_version_2(
			id.as_ref().as_ptr() as u32,
			pack_ptr_and_len(seed.as_ptr() as u32, seed.len() as u32),
			out_mut.as_ptr() as u32,
		)
	}
}

/// Native PolkaVM/JAM implementation of `ed25519_public_keys__raw`.
pub fn ed25519_public_keys__raw(id: KeyTypeId, out: &mut [ed25519::Public]) -> u32 {
	unsafe {
		ext_crypto_ed25519_public_keys_version_2(
			id.as_ref().as_ptr() as u32,
			pack_ptr_and_len(out.as_ptr() as u32, core::mem::size_of_val(out) as u32),
		)
	}
}

/// Native PolkaVM/JAM implementation of `ed25519_sign__raw`.
pub fn ed25519_sign__raw(
	id: KeyTypeId,
	pub_key: &ed25519::Public,
	msg: &[u8],
	out: &mut ed25519::Signature,
) -> Result<(), ()> {
	let pub_key_ref: &[u8] = (*pub_key).as_ref();
	let out_mut: &mut [u8] = (*out).as_mut();
	let result = unsafe {
		ext_crypto_ed25519_sign_version_2(
			id.as_ref().as_ptr() as u32,
			pub_key_ref.as_ptr() as u32,
			pack_ptr_and_len(msg.as_ptr() as u32, msg.len() as u32),
			out_mut.as_ptr() as u32,
		)
	};
	if result == 0 {
		Ok(())
	} else {
		Err(())
	}
}

/// Native PolkaVM/JAM implementation of `ed25519_verify`.
pub fn ed25519_verify(sig: &ed25519::Signature, msg: &[u8], pub_key: &ed25519::Public) -> bool {
	ed25519::Pair::verify(sig, msg, pub_key)
}

/// Native PolkaVM/JAM implementation of `secp256k1_ecdsa_recover__raw`.
pub fn secp256k1_ecdsa_recover__raw(
	sig: &[u8; 65],
	msg: &[u8; 32],
	out: &mut Pubkey512,
) -> Result<(), EcdsaVerifyError> {
	let rid =
		libsecp256k1::RecoveryId::parse(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8)
			.map_err(|_| EcdsaVerifyError::BadV)?;
	let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[..64])
		.map_err(|_| EcdsaVerifyError::BadRS)?;
	let msg = libsecp256k1::Message::parse(msg);
	let pubkey =
		libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
	out.0.copy_from_slice(&pubkey.serialize()[1..65]);
	Ok(())
}

/// Native PolkaVM/JAM implementation of `secp256k1_ecdsa_recover_compressed__raw`.
pub fn secp256k1_ecdsa_recover_compressed__raw(
	sig: &[u8; 65],
	msg: &[u8; 32],
	out: &mut Pubkey264,
) -> Result<(), EcdsaVerifyError> {
	let rid =
		libsecp256k1::RecoveryId::parse(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8)
			.map_err(|_| EcdsaVerifyError::BadV)?;
	let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[..64])
		.map_err(|_| EcdsaVerifyError::BadRS)?;
	let msg = libsecp256k1::Message::parse(msg);
	let pubkey =
		libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
	out.0.copy_from_slice(&pubkey.serialize_compressed());
	Ok(())
}

/// Native PolkaVM/JAM implementation of `sr25519_generate__raw`.
pub fn sr25519_generate__raw(id: KeyTypeId, seed: Option<Vec<u8>>, out: &mut sr25519::Public) {
	let seed = seed.encode();
	let out_mut: &mut [u8] = (*out).as_mut();
	unsafe {
		ext_crypto_sr25519_generate_version_2(
			id.as_ref().as_ptr() as u32,
			pack_ptr_and_len(seed.as_ptr() as u32, seed.len() as u32),
			out_mut.as_ptr() as u32,
		)
	}
}

/// Native PolkaVM/JAM implementation of `sr25519_public_keys__raw`.
pub fn sr25519_public_keys__raw(id: KeyTypeId, out: &mut [sr25519::Public]) -> u32 {
	unsafe {
		ext_crypto_sr25519_public_keys_version_2(
			id.as_ref().as_ptr() as u32,
			pack_ptr_and_len(out.as_ptr() as u32, core::mem::size_of_val(out) as u32),
		)
	}
}

/// Native PolkaVM/JAM implementation of `sr25519_sign__raw`.
pub fn sr25519_sign__raw(
	id: KeyTypeId,
	pub_key: &sr25519::Public,
	msg: &[u8],
	out: &mut sr25519::Signature,
) -> Result<(), ()> {
	let pub_key_ref: &[u8] = (*pub_key).as_ref();
	let out_mut: &mut [u8] = (*out).as_mut();
	let result = unsafe {
		ext_crypto_sr25519_sign_version_2(
			id.as_ref().as_ptr() as u32,
			pub_key_ref.as_ptr() as u32,
			pack_ptr_and_len(msg.as_ptr() as u32, msg.len() as u32),
			out_mut.as_ptr() as u32,
		)
	};
	if result == 0 {
		Ok(())
	} else {
		Err(())
	}
}

/// Native PolkaVM/JAM implementation of `sr25519_verify`.
pub fn sr25519_verify(sig: &sr25519::Signature, msg: &[u8], pub_key: &sr25519::Public) -> bool {
	sr25519::Pair::verify(sig, msg, pub_key)
}

/// Native PolkaVM/JAM implementation of `ed25519_public_keys`.
pub fn ed25519_public_keys(id: KeyTypeId) -> Vec<ed25519::Public> {
	let key_size = core::mem::size_of::<ed25519::Public>();
	let num_keys = ed25519_public_keys__raw(id, &mut []) as usize / key_size;
	let mut keys = vec![ed25519::Public::default(); num_keys];
	let num_keys = ed25519_public_keys__raw(id, &mut keys) as usize / key_size;
	keys.truncate(num_keys);
	keys
}

/// Native PolkaVM/JAM implementation of `ed25519_generate`.
pub fn ed25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ed25519::Public {
	let mut public = ed25519::Public::default();
	ed25519_generate__raw(id, seed, &mut public);
	public
}

/// Native PolkaVM/JAM implementation of `ed25519_sign`.
pub fn ed25519_sign(
	id: KeyTypeId,
	pub_key: &ed25519::Public,
	message: &[u8],
) -> Option<ed25519::Signature> {
	let mut signature = ed25519::Signature::default();
	ed25519_sign__raw(id, pub_key, message, &mut signature).ok()?;
	Some(signature)
}

/// Native PolkaVM/JAM implementation of `sr25519_public_keys`.
pub fn sr25519_public_keys(id: KeyTypeId) -> Vec<sr25519::Public> {
	let key_size = core::mem::size_of::<sr25519::Public>();
	let num_keys = sr25519_public_keys__raw(id, &mut []) as usize / key_size;
	let mut keys = vec![sr25519::Public::default(); num_keys];
	let num_keys = sr25519_public_keys__raw(id, &mut keys) as usize / key_size;
	keys.truncate(num_keys);
	keys
}

/// Native PolkaVM/JAM implementation of `sr25519_generate`.
pub fn sr25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> sr25519::Public {
	let mut public = sr25519::Public::default();
	sr25519_generate__raw(id, seed, &mut public);
	public
}

/// Native PolkaVM/JAM implementation of `sr25519_sign`.
pub fn sr25519_sign(
	id: KeyTypeId,
	pub_key: &sr25519::Public,
	message: &[u8],
) -> Option<sr25519::Signature> {
	let mut signature = sr25519::Signature::default();
	sr25519_sign__raw(id, pub_key, message, &mut signature).ok()?;
	Some(signature)
}

/// Native PolkaVM/JAM implementation of `ecdsa_public_keys`.
pub fn ecdsa_public_keys(id: KeyTypeId) -> Vec<ecdsa::Public> {
	let key_size = core::mem::size_of::<ecdsa::Public>();
	let num_keys = ecdsa_public_keys__raw(id, &mut []) as usize / key_size;
	let mut keys = vec![ecdsa::Public::default(); num_keys];
	let num_keys = ecdsa_public_keys__raw(id, &mut keys) as usize / key_size;
	keys.truncate(num_keys);
	keys
}

/// Native PolkaVM/JAM implementation of `ecdsa_generate`.
pub fn ecdsa_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ecdsa::Public {
	let mut public = ecdsa::Public::default();
	ecdsa_generate__raw(id, seed, &mut public);
	public
}

/// Native PolkaVM/JAM implementation of `ecdsa_sign`.
pub fn ecdsa_sign(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	message: &[u8],
) -> Option<ecdsa::Signature> {
	let mut signature = ecdsa::Signature::default();
	ecdsa_sign__raw(id, pub_key, message, &mut signature).ok()?;
	Some(signature)
}

/// Native PolkaVM/JAM implementation of `ecdsa_sign_prehashed`.
pub fn ecdsa_sign_prehashed(
	id: KeyTypeId,
	pub_key: &ecdsa::Public,
	msg: &[u8; 32],
) -> Option<ecdsa::Signature> {
	let mut signature = ecdsa::Signature::default();
	ecdsa_sign_prehashed__raw(id, pub_key, msg, &mut signature).ok()?;
	Some(signature)
}

/// Native PolkaVM/JAM implementation of `secp256k1_ecdsa_recover`.
pub fn secp256k1_ecdsa_recover(
	signature: &[u8; 65],
	message: &[u8; 32],
) -> Result<[u8; 64], EcdsaVerifyError> {
	let mut public = Pubkey512([0u8; 64]);
	secp256k1_ecdsa_recover__raw(signature, message, &mut public)?;
	Ok(public.0)
}

/// Native PolkaVM/JAM implementation of `secp256k1_ecdsa_recover_compressed`.
pub fn secp256k1_ecdsa_recover_compressed(
	signature: &[u8; 65],
	message: &[u8; 32],
) -> Result<[u8; 33], EcdsaVerifyError> {
	let mut public = Pubkey264([0u8; 33]);
	secp256k1_ecdsa_recover_compressed__raw(signature, message, &mut public)?;
	Ok(public.0)
}
