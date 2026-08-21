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

//! Native PolkaVM/JAM implementations of the `hashing` interface.

use crate::*;
#[cfg(feature = "bandersnatch-experimental")]
use sp_core::bandersnatch;
#[cfg(feature = "bls-experimental")]
use sp_core::{bls381, ecdsa_bls381};
/// Native PolkaVM/JAM implementation of `blake2_128__raw`.
pub fn blake2_128__raw(data: &[u8], out: &mut [u8; 16]) {
	out.copy_from_slice(&sp_crypto_hashing::blake2_128(data));
}

/// Native PolkaVM/JAM implementation of `blake2_256__raw`.
pub fn blake2_256__raw(data: &[u8], out: &mut [u8; 32]) {
	out.copy_from_slice(&sp_crypto_hashing::blake2_256(data));
}

/// Native PolkaVM/JAM implementation of `keccak_256__raw`.
pub fn keccak_256__raw(data: &[u8], out: &mut [u8; 32]) {
	out.copy_from_slice(&sp_crypto_hashing::keccak_256(data));
}

/// Native PolkaVM/JAM implementation of `keccak_512__raw`.
pub fn keccak_512__raw(data: &[u8], out: &mut Hash512) {
	out.0.copy_from_slice(&sp_crypto_hashing::keccak_512(data));
}

/// Native PolkaVM/JAM implementation of `sha2_256__raw`.
pub fn sha2_256__raw(data: &[u8], out: &mut [u8; 32]) {
	out.copy_from_slice(&sp_crypto_hashing::sha2_256(data));
}

/// Native PolkaVM/JAM implementation of `twox_128__raw`.
pub fn twox_128__raw(data: &[u8], out: &mut [u8; 16]) {
	out.copy_from_slice(&sp_crypto_hashing::twox_128(data));
}

/// Native PolkaVM/JAM implementation of `twox_256__raw`.
pub fn twox_256__raw(data: &[u8], out: &mut [u8; 32]) {
	out.copy_from_slice(&sp_crypto_hashing::twox_256(data));
}

/// Native PolkaVM/JAM implementation of `twox_64__raw`.
pub fn twox_64__raw(data: &[u8], out: &mut [u8; 8]) {
	out.copy_from_slice(&sp_crypto_hashing::twox_64(data));
}

/// Native PolkaVM/JAM implementation of `keccak_256`.
pub fn keccak_256(data: &[u8]) -> [u8; 32] {
	let mut out = [0u8; 32];
	keccak_256__raw(data, &mut out);
	out
}

/// Native PolkaVM/JAM implementation of `keccak_512`.
pub fn keccak_512(data: &[u8]) -> [u8; 64] {
	let mut out = Hash512::default();
	keccak_512__raw(data, &mut out);
	out.0
}

/// Native PolkaVM/JAM implementation of `sha2_256`.
pub fn sha2_256(data: &[u8]) -> [u8; 32] {
	let mut out = [0u8; 32];
	sha2_256__raw(data, &mut out);
	out
}

/// Native PolkaVM/JAM implementation of `blake2_128`.
pub fn blake2_128(data: &[u8]) -> [u8; 16] {
	let mut out = [0u8; 16];
	blake2_128__raw(data, &mut out);
	out
}

/// Native PolkaVM/JAM implementation of `blake2_256`.
pub fn blake2_256(data: &[u8]) -> [u8; 32] {
	let mut out = [0u8; 32];
	blake2_256__raw(data, &mut out);
	out
}

/// Native PolkaVM/JAM implementation of `twox_256`.
pub fn twox_256(data: &[u8]) -> [u8; 32] {
	let mut out = [0u8; 32];
	twox_256__raw(data, &mut out);
	out
}

/// Native PolkaVM/JAM implementation of `twox_128`.
pub fn twox_128(data: &[u8]) -> [u8; 16] {
	let mut out = [0u8; 16];
	twox_128__raw(data, &mut out);
	out
}

/// Native PolkaVM/JAM implementation of `twox_64`.
pub fn twox_64(data: &[u8]) -> [u8; 8] {
	let mut out = [0u8; 8];
	twox_64__raw(data, &mut out);
	out
}
