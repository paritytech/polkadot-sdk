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

//! EIP-7702: Set EOA Account Code implementation
//!
//! This module implements the authorization processing for EIP-7702, which allows
//! Externally Owned Accounts (EOAs) to temporarily set code in their account via
//! authorization tuples attached to transactions.

use crate::{
	evm::api::{AuthorizationListEntry, rlp},
	storage::AccountInfo,
	AccountInfoOf, Config, EIP7702_MAGIC, DELEGATION_INDICATOR_PREFIX,
	PER_AUTH_BASE_COST, PER_EMPTY_ACCOUNT_COST,
};
use alloc::{collections::BTreeSet, vec::Vec};
use codec::Encode;
use frame_support::ensure;
use sp_core::{Get, H160, U256};
use sp_io::crypto::secp256k1_ecdsa_recover_compressed;
use sp_runtime::{traits::Zero, SaturatedConversion};

/// Result of processing an authorization tuple
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationResult {
	/// Authorization was successfully processed
	Success { authority: H160, refund: u64 },
	/// Authorization failed validation (continue to next)
	Failed,
}

/// Process a list of EIP-7702 authorization tuples
///
/// This function processes authorization tuples according to the EIP-7702 specification:
/// 1. Verifies the chain ID (must be 0 or current chain)
/// 2. Recovers the authority address from the signature
/// 3. Verifies the account nonce matches
/// 4. Verifies the account code is empty or already delegated
/// 5. Sets the delegation indicator (0xef0100 || address) or clears if address is 0x0
/// 6. Increments the authority's nonce
/// 7. Tracks accessed addresses
///
/// # Parameters
/// - `authorization_list`: List of authorization tuples to process
/// - `chain_id`: Current chain ID
/// - `accessed_addresses`: Set to track accessed addresses for gas accounting
///
/// # Returns
/// Total gas refund for accounts that already existed
pub fn process_authorizations<T: Config>(
	authorization_list: Vec<AuthorizationListEntry>,
	chain_id: U256,
	accessed_addresses: &mut BTreeSet<H160>,
) -> u64 {
	let mut total_refund = 0u64;
	let mut last_valid_by_authority: alloc::collections::BTreeMap<H160, H160> =
		alloc::collections::BTreeMap::new();

	// First pass: collect all authorizations and track the last valid one per authority
	for auth in authorization_list.iter() {
		match process_single_authorization::<T>(auth, chain_id) {
			AuthorizationResult::Success { authority, .. } => {
				// Track the last valid authorization for this authority
				last_valid_by_authority.insert(authority, auth.address);
			},
			AuthorizationResult::Failed => continue,
		}
	}

	// Second pass: apply only the last valid authorization per authority
	for (authority, target_address) in last_valid_by_authority.iter() {
		// Add authority to accessed addresses (EIP-2929)
		accessed_addresses.insert(*authority);

		// Check if account already exists
		let account_exists = AccountInfoOf::<T>::contains_key(authority);

		// Set the delegation or clear it
		if target_address.is_zero() {
			// Clear delegation (reset to EOA)
			let _ = AccountInfo::<T>::clear_delegation(authority);
		} else {
			// Get current nonce
			let nonce = frame_system::Pallet::<T>::account_nonce(
				&T::AddressMapper::to_account_id(authority),
			);

			// Set delegation indicator
			if let Err(e) = AccountInfo::<T>::set_delegation(authority, *target_address, nonce) {
				log::debug!(
					target: crate::LOG_TARGET,
					"Failed to set delegation for {:?}: {:?}",
					authority,
					e
				);
				continue;
			}
		}

		// Increment the nonce
		frame_system::Pallet::<T>::inc_account_nonce(
			&T::AddressMapper::to_account_id(authority),
		);

		// Calculate refund if account already existed
		if account_exists {
			let refund = PER_EMPTY_ACCOUNT_COST.saturating_sub(PER_AUTH_BASE_COST);
			total_refund = total_refund.saturating_add(refund);
		}
	}

	total_refund
}

/// Process a single authorization tuple
///
/// This validates the authorization and returns the authority address if successful.
/// Failures result in AuthorizationResult::Failed and processing continues to the next tuple.
fn process_single_authorization<T: Config>(
	auth: &AuthorizationListEntry,
	chain_id: U256,
) -> AuthorizationResult {
	// 1. Verify chain ID is 0 or current chain
	if !auth.chain_id.is_zero() && auth.chain_id != chain_id {
		log::debug!(
			target: crate::LOG_TARGET,
			"Invalid chain_id in authorization: expected {:?} or 0, got {:?}",
			chain_id,
			auth.chain_id
		);
		return AuthorizationResult::Failed;
	}

	// 2. Verify nonce is less than 2^64 - 1
	if auth.nonce >= U256::from(u64::MAX) {
		log::debug!(target: crate::LOG_TARGET, "Authorization nonce too large: {:?}", auth.nonce);
		return AuthorizationResult::Failed;
	}

	// 3. Recover the authority address from the signature
	let authority = match recover_authority(auth) {
		Ok(addr) => addr,
		Err(_) => {
			log::debug!(target: crate::LOG_TARGET, "Failed to recover authority from signature");
			return AuthorizationResult::Failed;
		},
	};

	// 4. Verify the nonce matches the account's current nonce
	let account_id = T::AddressMapper::to_account_id(&authority);
	let current_nonce: u64 = frame_system::Pallet::<T>::account_nonce(&account_id)
		.unique_saturated_into();
	let expected_nonce: u64 = auth.nonce.saturated_into();

	if current_nonce != expected_nonce {
		log::debug!(
			target: crate::LOG_TARGET,
			"Nonce mismatch for {:?}: expected {:?}, got {:?}",
			authority,
			current_nonce,
			expected_nonce
		);
		return AuthorizationResult::Failed;
	}

	// 5. Verify code is empty or already delegated
	if AccountInfo::<T>::is_contract(&authority) {
		if !AccountInfo::<T>::is_delegated(&authority) {
			log::debug!(
				target: crate::LOG_TARGET,
				"Account {:?} has non-delegation code",
				authority
			);
			return AuthorizationResult::Failed;
		}
	}

	// Calculate refund
	let account_exists = AccountInfoOf::<T>::contains_key(&authority);
	let refund = if account_exists {
		PER_EMPTY_ACCOUNT_COST.saturating_sub(PER_AUTH_BASE_COST)
	} else {
		0
	};

	AuthorizationResult::Success { authority, refund }
}

/// Recover the authority address from an authorization signature
///
/// Implements the EIP-7702 signature recovery:
/// - Message = keccak(MAGIC || rlp([chain_id, address, nonce]))
/// - Signature must use normalized s value (s <= secp256k1n/2) per EIP-2
fn recover_authority(auth: &AuthorizationListEntry) -> Result<H160, ()> {
	// Construct the message: MAGIC || rlp([chain_id, address, nonce])
	let mut message = Vec::new();
	message.push(EIP7702_MAGIC);

	// RLP encode [chain_id, address, nonce]
	let mut rlp_stream = rlp::RlpStream::new_list(3);
	rlp_stream.append(&auth.chain_id);
	rlp_stream.append(&auth.address);
	rlp_stream.append(&auth.nonce);
	let rlp_encoded = rlp_stream.out();
	message.extend_from_slice(&rlp_encoded);

	// Hash the message
	let message_hash = crate::keccak_256(&message);

	// Verify s is normalized (EIP-2)
	// secp256k1n = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
	// s must be <= secp256k1n/2
	let secp256k1n_half = U256::from_dec_str(
		"57896044618658097711785492504343953926418782139537452191302581570759080747168",
	)
	.unwrap();

	if auth.s > secp256k1n_half {
		log::debug!(target: crate::LOG_TARGET, "Invalid signature: s value too large");
		return Err(());
	}

	// Convert signature components to bytes
	let mut signature = [0u8; 65];
	let r_bytes = auth.r.to_big_endian();
	let s_bytes = auth.s.to_big_endian();
	signature[..32].copy_from_slice(&r_bytes);
	signature[32..64].copy_from_slice(&s_bytes);

	// recovery_id is y_parity for EIP-7702
	let recovery_id: u8 = auth.y_parity.saturated_into();
	if recovery_id > 1 {
		log::debug!(target: crate::LOG_TARGET, "Invalid y_parity: must be 0 or 1");
		return Err(());
	}
	signature[64] = recovery_id;

	// Recover the public key
	let pubkey = secp256k1_ecdsa_recover_compressed(&signature, &message_hash).map_err(|_| ())?;

	// Derive Ethereum address from public key
	// Address = last 20 bytes of keccak256(pubkey[1..])
	let pubkey_hash = crate::keccak_256(&pubkey[1..]);
	let mut address_bytes = [0u8; 20];
	address_bytes.copy_from_slice(&pubkey_hash[12..]);

	Ok(H160::from(address_bytes))
}

/// Calculate the intrinsic gas cost for processing authorizations
///
/// Each authorization costs PER_EMPTY_ACCOUNT_COST regardless of validity.
/// Refunds are processed separately during execution.
pub fn authorization_intrinsic_gas(authorization_count: usize) -> u64 {
	(authorization_count as u64).saturating_mul(PER_EMPTY_ACCOUNT_COST)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H160;

	#[test]
	fn test_delegation_indicator_size() {
		// Delegation indicator must be exactly 23 bytes
		let mut code = Vec::new();
		code.extend_from_slice(&DELEGATION_INDICATOR_PREFIX);
		code.extend_from_slice(&[0u8; 20]); // 20-byte address
		assert_eq!(code.len(), 23);
	}

	#[test]
	fn test_auth_gas_calculation() {
		assert_eq!(authorization_intrinsic_gas(0), 0);
		assert_eq!(authorization_intrinsic_gas(1), PER_EMPTY_ACCOUNT_COST);
		assert_eq!(authorization_intrinsic_gas(2), PER_EMPTY_ACCOUNT_COST * 2);
	}
}