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

use sp_runtime_interface::{
	pass_by::{AllocateAndReturnPointer, PassFatPointerAndRead, PassPointerAndWrite},
	runtime_interface,
};

use crate::*;

/// Interface that provides functions for hashing with different algorithms.
#[runtime_interface]
pub trait Hashing {
	/// Conduct a 256-bit Keccak hash.
	fn keccak_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::keccak_256(data)
	}

	/// Conduct a 256-bit Keccak hash.
	#[version(2)]
	#[raw_api]
	fn keccak_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::keccak_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `keccak_256` host function.
	#[wrapper]
	fn keccak_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		keccak_256__raw(data, &mut out);
		out
	}

	/// Conduct a 512-bit Keccak hash.
	fn keccak_512(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 64], 64> {
		sp_crypto_hashing::keccak_512(data)
	}

	/// Conduct a 512-bit Keccak hash.
	#[version(2)]
	#[raw_api]
	fn keccak_512(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut Hash512, 64>) {
		out.0.copy_from_slice(&sp_crypto_hashing::keccak_512(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `keccak_512` host function.
	#[wrapper]
	fn keccak_512(data: &[u8]) -> [u8; 64] {
		let mut out = Hash512::default();
		keccak_512__raw(data, &mut out);
		out.0
	}

	/// Conduct a 256-bit Sha2 hash.
	fn sha2_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::sha2_256(data)
	}

	/// Conduct a 256-bit Sha2 hash.
	#[version(2)]
	#[raw_api]
	fn sha2_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::sha2_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `sha2_256` host function.
	#[wrapper]
	fn sha2_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		sha2_256__raw(data, &mut out);
		out
	}

	/// Conduct a 128-bit Blake2 hash.
	fn blake2_128(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 16], 16> {
		sp_crypto_hashing::blake2_128(data)
	}

	/// Conduct a 128-bit Blake2 hash.
	#[version(2)]
	#[raw_api]
	fn blake2_128(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 16], 16>) {
		out.copy_from_slice(&sp_crypto_hashing::blake2_128(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `blake2_128` host function.
	#[wrapper]
	fn blake2_128(data: &[u8]) -> [u8; 16] {
		let mut out = [0u8; 16];
		blake2_128__raw(data, &mut out);
		out
	}

	/// Conduct a 256-bit Blake2 hash.
	fn blake2_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::blake2_256(data)
	}

	/// Conduct a 256-bit Blake2 hash.
	#[version(2)]
	#[raw_api]
	fn blake2_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::blake2_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `blake2_256` host function.
	#[wrapper]
	fn blake2_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		blake2_256__raw(data, &mut out);
		out
	}

	/// Conduct four XX hashes to give a 256-bit result.
	fn twox_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::twox_256(data)
	}

	/// Conduct four XX hashes to give a 256-bit result.
	#[version(2)]
	#[raw_api]
	fn twox_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::twox_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `twox_256` host function.
	#[wrapper]
	fn twox_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		twox_256__raw(data, &mut out);
		out
	}

	/// Conduct two XX hashes to give a 128-bit result.
	fn twox_128(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 16], 16> {
		sp_crypto_hashing::twox_128(data)
	}

	/// Conduct two XX hashes to give a 128-bit result.
	#[version(2)]
	#[raw_api]
	fn twox_128(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 16], 16>) {
		out.copy_from_slice(&sp_crypto_hashing::twox_128(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `twox_128` host function.
	#[wrapper]
	fn twox_128(data: &[u8]) -> [u8; 16] {
		let mut out = [0u8; 16];
		twox_128__raw(data, &mut out);
		out
	}

	/// Conduct two XX hashes to give a 64-bit result.
	fn twox_64(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 8], 8> {
		sp_crypto_hashing::twox_64(data)
	}

	/// Conduct two XX hashes to give a 64-bit result.
	#[version(2)]
	#[raw_api]
	fn twox_64(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 8], 8>) {
		out.copy_from_slice(&sp_crypto_hashing::twox_64(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `twox_64` host function.
	#[wrapper]
	fn twox_64(data: &[u8]) -> [u8; 8] {
		let mut out = [0u8; 8];
		twox_64__raw(data, &mut out);
		out
	}
}
