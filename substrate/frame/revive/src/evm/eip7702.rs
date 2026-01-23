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
	address::AddressMapper,
	evm::api::{
		recover_eth_address_from_message, rlp, AuthorizationListEntry, SignedAuthorizationListEntry,
	},
	storage::AccountInfo,
	Config,
};
use alloc::vec::Vec;

use sp_core::{H160, U256};

use sp_runtime::SaturatedConversion;

/// EIP-7702: Magic value for authorization signature message
pub const EIP7702_MAGIC: u8 = 0x05;

/// EIP-7702: Base cost for processing each authorization tuple
pub const PER_AUTH_BASE_COST: u64 = 12500;

/// EIP-7702: Cost for empty account creation
pub const PER_EMPTY_ACCOUNT_COST: u64 = 25000;

/// Result of processing an authorization tuple
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationResult {
	/// Authorization was successfully processed
	Success {
		/// The authority address that was authorized
		authority: H160,
		/// Gas refund amount for existing accounts
		refund: u64,
	},
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
	authorization_list: &[SignedAuthorizationListEntry],
	chain_id: U256,
) -> u64 {
	let mut total_refund = 0u64;
	let mut last_valid_by_authority: alloc::collections::BTreeMap<H160, H160> =
		alloc::collections::BTreeMap::new();

	// First pass: collect all authorizations and track the last valid one per authority
	for auth in authorization_list.iter() {
		match process_single_authorization::<T>(auth, chain_id) {
			AuthorizationResult::Success { authority, refund } => {
				// Track the last valid authorization for this authority
				last_valid_by_authority.insert(authority, auth.address);
				total_refund = total_refund.saturating_add(refund);
			},
			AuthorizationResult::Failed => continue,
		}
	}

	// Second pass: apply only the last valid authorization per authority
	for (authority, target_address) in last_valid_by_authority.iter() {
		// Set the delegation or clear it
		if target_address.is_zero() {
			// Clear delegation (reset to EOA)
			let _ = AccountInfo::<T>::clear_delegation(authority);
		} else {
			// Get current nonce
			let nonce = frame_system::Pallet::<T>::account_nonce(&T::AddressMapper::to_account_id(
				authority,
			));

			// Set delegation indicator
			if let Err(e) = AccountInfo::<T>::set_delegation(authority, *target_address, nonce) {
				log::debug!(
					target: crate::LOG_TARGET,
					"Failed to set delegation for {authority:?}: {e:?}",
				);
				continue;
			}
		}

		// Increment the nonce
		frame_system::Pallet::<T>::inc_account_nonce(&T::AddressMapper::to_account_id(authority));
	}

	total_refund
}

/// Process a single authorization tuple
///
/// This validates the authorization and returns the authority address if successful.
/// Failures result in AuthorizationResult::Failed and processing continues to the next tuple.
fn process_single_authorization<T: Config>(
	auth: &SignedAuthorizationListEntry,
	chain_id: U256,
) -> AuthorizationResult {
	// 1. Verify chain ID is 0 or current chain
	if !auth.chain_id.is_zero() && auth.chain_id != chain_id {
		log::debug!(
			target: crate::LOG_TARGET,
			"Invalid chain_id in authorization: expected {chain_id:?} or 0, got {:?}",
			auth.chain_id
		);
		return AuthorizationResult::Failed;
	}

	// 2. Verify nonce is less than 2^64 - 1
	if auth.nonce >= U256::from(u64::MAX) {
		log::debug!(target: crate::LOG_TARGET, "Authorization nonce too large: {nonce:?}", nonce = auth.nonce);
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
	let current_nonce = frame_system::Pallet::<T>::account_nonce(&account_id);
	let expected_nonce = auth.nonce;

	if U256::from(current_nonce.saturated_into::<u64>()) != expected_nonce {
		log::debug!(
			target: crate::LOG_TARGET,
			"Nonce mismatch for {authority:?}: expected {current_nonce:?}, got {expected_nonce:?}",
		);
		return AuthorizationResult::Failed;
	}

	// 5. Verify code is empty or already delegated
	if AccountInfo::<T>::is_contract(&authority) {
		log::debug!(
			target: crate::LOG_TARGET,
			"Account {authority:?} has non-delegation code",
		);
		return AuthorizationResult::Failed;
	}

	// Calculate refund based on whether the account exists in frame_system
	// An account exists if it has been initialized in the substrate account system
	let account_exists = frame_system::Pallet::<T>::account_exists(&account_id);
	let refund =
		if account_exists { PER_EMPTY_ACCOUNT_COST.saturating_sub(PER_AUTH_BASE_COST) } else { 0 };

	AuthorizationResult::Success { authority, refund }
}

/// Recover the authority address from an authorization signature
///
/// Implements the EIP-7702 signature recovery:
/// - Message = keccak(MAGIC || rlp([chain_id, address, nonce]))
fn recover_authority(auth: &SignedAuthorizationListEntry) -> Result<H160, ()> {
	// Construct the message: MAGIC || rlp([chain_id, address, nonce])
	let mut message = Vec::new();
	message.push(EIP7702_MAGIC);

	// Create unsigned authorization tuple for RLP encoding
	let unsigned_auth = AuthorizationListEntry {
		chain_id: auth.chain_id,
		address: auth.address,
		nonce: auth.nonce,
	};

	// RLP encode the unsigned authorization tuple
	let rlp_encoded = rlp::encode(&unsigned_auth);
	message.extend_from_slice(&rlp_encoded);

	// Convert signature components to bytes and recover the address
	let signature = auth.signature();
	recover_eth_address_from_message(&message, &signature)
}

/// Calculate the intrinsic gas cost for processing authorizations
///
/// Each authorization costs PER_EMPTY_ACCOUNT_COST regardless of validity.
/// Refunds are processed separately during execution.
pub fn authorization_intrinsic_gas(authorization_count: usize) -> u64 {
	(authorization_count as u64).saturating_mul(PER_EMPTY_ACCOUNT_COST)
}
