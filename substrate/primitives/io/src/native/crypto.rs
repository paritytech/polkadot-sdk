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
pub fn ecdsa_generate__raw(_id: KeyTypeId, _seed: Option<Vec<u8>>, _out: &mut ecdsa::Public) {
	panic!("`crypto::ecdsa_generate__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `ecdsa_public_keys__raw`.
pub fn ecdsa_public_keys__raw(_id: KeyTypeId, _out: &mut [ecdsa::Public]) -> u32 {
	panic!(
		"`crypto::ecdsa_public_keys__raw` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `ecdsa_sign__raw`.
pub fn ecdsa_sign__raw(
	_id: KeyTypeId,
	_pub_key: &ecdsa::Public,
	_msg: &[u8],
	_out: &mut ecdsa::Signature,
) -> Result<(), ()> {
	panic!("`crypto::ecdsa_sign__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `ecdsa_sign_prehashed__raw`.
pub fn ecdsa_sign_prehashed__raw(
	_id: KeyTypeId,
	_pub_key: &ecdsa::Public,
	_msg: &[u8; 32],
	_out: &mut ecdsa::Signature,
) -> Result<(), ()> {
	panic!("`crypto::ecdsa_sign_prehashed__raw` needs node-side state and has no in-blob implementation")
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
pub fn ed25519_generate__raw(_id: KeyTypeId, _seed: Option<Vec<u8>>, _out: &mut ed25519::Public) {
	panic!(
		"`crypto::ed25519_generate__raw` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `ed25519_public_keys__raw`.
pub fn ed25519_public_keys__raw(_id: KeyTypeId, _out: &mut [ed25519::Public]) -> u32 {
	panic!("`crypto::ed25519_public_keys__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `ed25519_sign__raw`.
pub fn ed25519_sign__raw(
	_id: KeyTypeId,
	_pub_key: &ed25519::Public,
	_msg: &[u8],
	_out: &mut ed25519::Signature,
) -> Result<(), ()> {
	panic!("`crypto::ed25519_sign__raw` needs node-side state and has no in-blob implementation")
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
pub fn sr25519_generate__raw(_id: KeyTypeId, _seed: Option<Vec<u8>>, _out: &mut sr25519::Public) {
	panic!(
		"`crypto::sr25519_generate__raw` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `sr25519_public_keys__raw`.
pub fn sr25519_public_keys__raw(_id: KeyTypeId, _out: &mut [sr25519::Public]) -> u32 {
	panic!("`crypto::sr25519_public_keys__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `sr25519_sign__raw`.
pub fn sr25519_sign__raw(
	_id: KeyTypeId,
	_pub_key: &sr25519::Public,
	_msg: &[u8],
	_out: &mut sr25519::Signature,
) -> Result<(), ()> {
	panic!("`crypto::sr25519_sign__raw` needs node-side state and has no in-blob implementation")
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
