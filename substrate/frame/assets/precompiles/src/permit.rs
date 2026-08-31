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

//! ERC20Permit pallet for signature-based approvals (EIP-2612).
//!
//! This pallet stores permit-related state (nonces) and provides EIP-712
//! signature verification for gasless approvals.
//!
//! # Security Notes
//!
//! - **Nonce management**: Use `use_permit` (not `verify_permit`) to atomically verify and consume
//!   permits. This prevents replay attacks.
//! - **Deadline validation**: Permits are validated against UNIX timestamps.
//! - **Domain separation**: Each verifying contract has its own domain separator.
//! - **Signature malleability**: The `s` value is checked to be in the lower half of the secp256k1
//!   curve order to prevent signature malleability attacks.

use alloc::vec::Vec;
use frame_support::{pallet_prelude::*, traits::UnixTime};
use pallet_revive::precompiles::H160;
use sp_core::{H256, U256};
use sp_io::{crypto::secp256k1_ecdsa_recover, hashing::keccak_256};

pub use pallet::*;

/// EIP-712 type hash for the domain separator.
/// keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
pub(crate) const DOMAIN_TYPEHASH: [u8; 32] = const_crypto::sha3::Keccak256::new()
	.update(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
	.finalize();

/// EIP-712 type hash for Permit.
/// keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)")
///
/// Computed at compile time from the canonical string, eliminating any risk of a
/// copy-paste error in a hand-written byte array.
pub(crate) const PERMIT_TYPEHASH: [u8; 32] = const_crypto::sha3::Keccak256::new()
	.update(b"Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)")
	.finalize();

/// Half of the secp256k1 curve order (n/2).
/// Used to ensure `s` is in the lower half to prevent signature malleability.
/// n = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
/// n/2 = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0
///
/// TODO: Replace usages with `sp_core::ecdsa::is_signature_normalized` once
/// paritytech/polkadot-sdk#5841 lands.
pub(crate) const SECP256K1_N_DIV_2: [u8; 32] = [
	0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
	0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B, 0x20, 0xA0,
];

/// Encoded length constants for EIP-712 encoding.
/// Domain separator: typehash(32) + name_hash(32) + version_hash(32) + chainId(32) +
/// verifyingContract(32) = 160 bytes
pub(crate) const DOMAIN_SEPARATOR_ENCODED_LEN: usize = 32 * 5;
/// Permit struct: typehash(32) + owner(32) + spender(32) + value(32) + nonce(32) + deadline(32) =
/// 192 bytes
pub(crate) const PERMIT_STRUCT_ENCODED_LEN: usize = 32 * 6;
/// Digest prefix: \x19\x01(2) + domain_separator(32) + struct_hash(32) = 66 bytes
pub(crate) const DIGEST_PREFIX_LEN: usize = 2 + 32 + 32;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config {
		/// The chain ID used in EIP-712 domain separator.
		#[pallet::constant]
		type ChainId: Get<u64>;

		/// Weight information for permit operations.
		type WeightInfo: crate::weights::WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Nonces for permit signatures.
	/// Mapping: (verifying_contract, owner_address) => nonce
	///
	/// Uses Blake2_128Concat for the first key to prevent storage collision attacks
	/// when the verifying_contract address could be influenced by an attacker.
	///
	/// Note: EIP-2612 specifies uint256 nonce. We store as U256 for compatibility.
	#[pallet::storage]
	pub type Nonces<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		H160, // verifying contract address (precompile address)
		Blake2_128Concat,
		H160, // owner ethereum address
		U256, // nonce (EIP-2612 uses uint256)
		ValueQuery,
	>;

	/// Error types for the permit pallet.
	#[pallet::error]
	pub enum Error<T> {
		/// The permit signature is invalid.
		InvalidSignature,
		/// The signer does not match the owner.
		SignerMismatch,
		/// The permit has expired (deadline passed).
		PermitExpired,
		/// The signature's `s` value is too high (malleability protection).
		SignatureSValueTooHigh,
		/// The signature's `v` value is invalid.
		InvalidVValue,
		/// Nonce overflow - account has used too many permits.
		NonceOverflow,
		/// The owner address is invalid (e.g., zero address).
		InvalidOwner,
		/// The spender address is invalid (e.g., zero address).
		InvalidSpender,
	}

	impl<T: Config> Pallet<T> {
		/// Get the current nonce for an owner on a specific verifying contract.
		pub fn nonce(verifying_contract: &H160, owner: &H160) -> U256 {
			Nonces::<T>::get(verifying_contract, owner)
		}

		/// Increment the nonce for an owner on a specific verifying contract.
		/// Returns the new nonce value, or an error if overflow would occur.
		pub fn increment_nonce(verifying_contract: &H160, owner: &H160) -> Result<U256, Error<T>> {
			Nonces::<T>::try_mutate(verifying_contract, owner, |nonce| {
				*nonce = nonce.checked_add(U256::one()).ok_or(Error::<T>::NonceOverflow)?;
				Ok(*nonce)
			})
		}

		/// Compute the EIP-712 domain separator for a given verifying contract.
		///
		/// DOMAIN_SEPARATOR = keccak256(abi.encode(
		///   keccak256("EIP712Domain(string name,string version,uint256 chainId,address
		/// verifyingContract)"),
		///   keccak256(name),
		///   keccak256("1"),
		///   chainId,
		///   verifyingContract
		/// ))
		///
		/// The `name` parameter should be the token name per EIP-2612 specification.
		pub fn compute_domain_separator(verifying_contract: &H160, name: &[u8]) -> H256 {
			let name_hash = keccak_256(name);
			let version_hash = keccak_256(b"1");
			let chain_id = T::ChainId::get();

			// Encode: typehash || name_hash || version_hash || chainId || verifyingContract
			let mut data = Vec::with_capacity(DOMAIN_SEPARATOR_ENCODED_LEN);
			data.extend_from_slice(&DOMAIN_TYPEHASH);
			data.extend_from_slice(&name_hash);
			data.extend_from_slice(&version_hash);
			// Pad chain_id to 32 bytes (big-endian)
			data.extend_from_slice(&[0u8; 24]);
			data.extend_from_slice(&chain_id.to_be_bytes());
			// Pad address to 32 bytes
			data.extend_from_slice(&[0u8; 12]);
			data.extend_from_slice(verifying_contract.as_bytes());

			H256(keccak_256(&data))
		}

		/// Compute the EIP-712 struct hash for a permit.
		///
		/// structHash = keccak256(abi.encode(
		///   PERMIT_TYPEHASH,
		///   owner,
		///   spender,
		///   value,
		///   nonce,
		///   deadline
		/// ))
		pub fn permit_struct_hash(
			owner: &H160,
			spender: &H160,
			value: &[u8; 32], // U256 as bytes (big-endian)
			nonce: &U256,
			deadline: &[u8; 32], // U256 as bytes (big-endian)
		) -> H256 {
			let mut data = Vec::with_capacity(PERMIT_STRUCT_ENCODED_LEN);
			data.extend_from_slice(&PERMIT_TYPEHASH);
			// owner (padded to 32 bytes)
			data.extend_from_slice(&[0u8; 12]);
			data.extend_from_slice(owner.as_bytes());
			// spender (padded to 32 bytes)
			data.extend_from_slice(&[0u8; 12]);
			data.extend_from_slice(spender.as_bytes());
			// value (already 32 bytes)
			data.extend_from_slice(value);
			// nonce (convert U256 to 32 bytes big-endian)
			data.extend_from_slice(&nonce.to_big_endian());
			// deadline (already 32 bytes)
			data.extend_from_slice(deadline);

			H256(keccak_256(&data))
		}

		/// Compute the final EIP-712 digest to be signed.
		///
		/// digest = keccak256("\x19\x01" || domainSeparator || structHash)
		///
		/// The `name` parameter should be the token name per EIP-2612 specification.
		pub fn permit_digest(
			verifying_contract: &H160,
			name: &[u8],
			owner: &H160,
			spender: &H160,
			value: &[u8; 32],
			nonce: &U256,
			deadline: &[u8; 32],
		) -> [u8; 32] {
			let domain_separator = Self::compute_domain_separator(verifying_contract, name);
			let struct_hash = Self::permit_struct_hash(owner, spender, value, nonce, deadline);

			let mut data = Vec::with_capacity(DIGEST_PREFIX_LEN);
			data.extend_from_slice(&[0x19, 0x01]);
			data.extend_from_slice(domain_separator.as_bytes());
			data.extend_from_slice(struct_hash.as_bytes());

			keccak_256(&data)
		}

		/// Check if the signature's `s` value is in the lower half of the curve order.
		///
		/// This prevents signature malleability attacks where an attacker can
		/// create a second valid signature by flipping `s` to `n - s`.
		///
		/// TODO: Replace with `sp_core::ecdsa::is_signature_normalized` once
		/// paritytech/polkadot-sdk#5841 lands.
		fn is_s_value_valid(s: &[u8; 32]) -> bool {
			for i in 0..32 {
				if s[i] < SECP256K1_N_DIV_2[i] {
					return true;
				}
				if s[i] > SECP256K1_N_DIV_2[i] {
					return false;
				}
			}
			// s == SECP256K1_N_DIV_2, which is valid
			true
		}

		/// Recover the signer address from an ECDSA signature.
		///
		/// Returns `Ok(address)` if the signature is valid, `Err` otherwise.
		///
		/// This function also validates that the `s` value is in the lower half
		/// of the curve order to prevent signature malleability.
		pub fn ecrecover(
			digest: &[u8; 32],
			v: u8,
			r: &[u8; 32],
			s: &[u8; 32],
		) -> Result<H160, Error<T>> {
			// Check signature malleability: s must be in lower half of curve order
			if !Self::is_s_value_valid(s) {
				return Err(Error::<T>::SignatureSValueTooHigh);
			}

			// Convert v to recovery_id (Ethereum v is 27 or 28, recovery_id is 0 or 1)
			let recovery_id = v.checked_sub(27).ok_or(Error::<T>::InvalidVValue)?;
			if recovery_id > 1 {
				return Err(Error::<T>::InvalidVValue);
			}

			// Build signature in format expected by secp256k1_ecdsa_recover: [r, s, recovery_id]
			let mut sig = [0u8; 65];
			sig[0..32].copy_from_slice(r);
			sig[32..64].copy_from_slice(s);
			sig[64] = recovery_id;

			// Recover uncompressed public key (64 bytes)
			let pubkey =
				secp256k1_ecdsa_recover(&sig, digest).map_err(|_| Error::<T>::InvalidSignature)?;

			// Convert public key to Ethereum address: keccak256(pubkey)[12..]
			let hash = keccak_256(&pubkey);
			let mut address = H160::zero();
			address.0.copy_from_slice(&hash[12..]);

			Ok(address)
		}

		/// Verify a permit signature without consuming it.
		///
		/// **WARNING**: This function does NOT increment the nonce. Using this
		/// function alone will leave the permit vulnerable to replay attacks.
		/// Use `use_permit` instead for production code.
		///
		/// This function is provided for cases where you need to verify a permit
		/// in a read-only context or need to separate verification from consumption.
		///
		/// The `name` parameter should be the token name per EIP-2612 specification.
		fn do_verify_permit(
			verifying_contract: &H160,
			name: &[u8],
			owner: &H160,
			spender: &H160,
			value: &[u8; 32],
			deadline: &[u8; 32],
			v: u8,
			r: &[u8; 32],
			s: &[u8; 32],
		) -> Result<(), Error<T>> {
			// EIP-2612: owner and spender cannot be the zero address
			if owner.is_zero() {
				return Err(Error::<T>::InvalidOwner);
			}
			if spender.is_zero() {
				return Err(Error::<T>::InvalidSpender);
			}

			// Validate deadline against current timestamp.
			// EIP-2612 specifies deadlines in UNIX seconds. We use the `UnixTime`
			// trait which returns a `core::time::Duration` — its `as_secs()` method
			// gives us seconds regardless of pallet_timestamp's internal resolution
			// (which stores milliseconds, converted via `Duration::from_millis` in
			// pallet_timestamp's `UnixTime` implementation).
			let now_seconds = <pallet_timestamp::Pallet<T> as UnixTime>::now().as_secs();
			let deadline_u256 = U256::from_big_endian(deadline);
			let now_u256 = U256::from(now_seconds);

			if deadline_u256 < now_u256 {
				return Err(Error::<T>::PermitExpired);
			}

			let nonce = Self::nonce(verifying_contract, owner);
			let digest = Self::permit_digest(
				verifying_contract,
				name,
				owner,
				spender,
				value,
				&nonce,
				deadline,
			);

			let recovered = Self::ecrecover(&digest, v, r, s)?;

			if &recovered != owner {
				return Err(Error::<T>::SignerMismatch);
			}

			Ok(())
		}

		/// Verify and consume a permit signature atomically.
		///
		/// This is the recommended function for production use. It:
		/// 1. Validates the deadline against the current timestamp
		/// 2. Verifies the signature matches the owner
		/// 3. Increments the nonce to prevent replay attacks
		///
		/// The `name` parameter should be the token name per EIP-2612 specification.
		///
		/// After this function returns `Ok(())`, the permit cannot be used again.
		pub fn use_permit(
			verifying_contract: &H160,
			name: &[u8],
			owner: &H160,
			spender: &H160,
			value: &[u8; 32],
			deadline: &[u8; 32],
			v: u8,
			r: &[u8; 32],
			s: &[u8; 32],
		) -> Result<(), Error<T>> {
			// Verify the permit first
			Self::do_verify_permit(
				verifying_contract,
				name,
				owner,
				spender,
				value,
				deadline,
				v,
				r,
				s,
			)?;

			// Consume the permit by incrementing the nonce
			// This prevents the same permit from being used again
			Self::increment_nonce(verifying_contract, owner)?;

			Ok(())
		}
	}

	#[cfg(test)]
	impl<T: Config> Pallet<T> {
		/// Test-only entry point that exposes [`do_verify_permit`] without consuming the nonce.
		///
		/// Use this in unit tests to exercise signature verification in isolation.
		/// Production callers must use [`use_permit`], which atomically verifies and
		/// increments the nonce to prevent replay attacks.
		pub fn verify_permit(
			verifying_contract: &H160,
			name: &[u8],
			owner: &H160,
			spender: &H160,
			value: &[u8; 32],
			deadline: &[u8; 32],
			v: u8,
			r: &[u8; 32],
			s: &[u8; 32],
		) -> Result<(), Error<T>> {
			Self::do_verify_permit(
				verifying_contract,
				name,
				owner,
				spender,
				value,
				deadline,
				v,
				r,
				s,
			)
		}
	}
}
