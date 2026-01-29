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

/// Result of processing an authorization tuple
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationResult {
	/// Authorization was successfully processed
	Success {
		/// The authority address that was authorized
		authority: H160,
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
///
/// # Parameters
/// - `authorization_list`: List of authorization tuples to process
/// - `chain_id`: Current chain ID
pub fn process_authorizations<T: Config>(
	authorization_list: &[SignedAuthorizationListEntry],
	chain_id: U256,
) -> (usize, usize) {
	let mut last_valid_by_authority: alloc::collections::BTreeMap<H160, H160> =
		alloc::collections::BTreeMap::new();

	for auth in authorization_list.iter() {
		match process_single_authorization::<T>(auth, chain_id) {
			AuthorizationResult::Success { authority } => {
				last_valid_by_authority.insert(authority, auth.address);
			},
			AuthorizationResult::Failed => continue,
		}
	}

	apply_delegations::<T>(last_valid_by_authority)
}

/// Apply delegations for the given authority-to-target mappings
///
/// This function performs the actual delegation setting/clearing and nonce increments.
/// It returns the count of authorities whose accounts did not exist in frame_system
/// before processing and the count of authorities whose accounts already existed.
///
/// # Parameters
/// - `authorities`: Map of authority addresses to their target delegation addresses
///
/// # Returns
/// A tuple of (new_accounts, existing_accounts) where:
/// - new_accounts: The number of authority accounts that were newly created (did not exist before)
/// - existing_accounts: The number of authority accounts that already existed
pub(crate) fn apply_delegations<T: Config>(
	authorities: alloc::collections::BTreeMap<H160, H160>,
) -> (usize, usize) {
	let mut new_account_count = 0;
	let mut existing_account_count = 0;

	for (authority, target_address) in authorities.iter() {
		let account_id = T::AddressMapper::to_account_id(authority);

		let account_exists = frame_system::Account::<T>::contains_key(&account_id);
		if account_exists {
			existing_account_count += 1;
		} else {
			new_account_count += 1;
		}

		if target_address.is_zero() {
			let _ = AccountInfo::<T>::clear_delegation(authority);
		} else {
			if let Err(e) = AccountInfo::<T>::set_delegation(authority, *target_address) {
				log::debug!(
					target: crate::LOG_TARGET,
					"Failed to set delegation for {authority:?}: {e:?}",
				);
				continue;
			}
		}

		frame_system::Pallet::<T>::inc_account_nonce(&account_id);
	}

	(new_account_count, existing_account_count)
}

/// Process a single authorization tuple
///
/// This validates the authorization and returns the authority address if successful.
/// Failures result in AuthorizationResult::Failed and processing continues to the next tuple.
pub(crate) fn process_single_authorization<T: Config>(
	auth: &SignedAuthorizationListEntry,
	chain_id: U256,
) -> AuthorizationResult {
	if !auth.chain_id.is_zero() && auth.chain_id != chain_id {
		log::debug!(
			target: crate::LOG_TARGET,
			"Invalid chain_id in authorization: expected {chain_id:?} or 0, got {:?}",
			auth.chain_id
		);
		return AuthorizationResult::Failed;
	}

	if auth.nonce >= U256::from(u64::MAX) {
		log::debug!(target: crate::LOG_TARGET, "Authorization nonce too large: {nonce:?}", nonce = auth.nonce);
		return AuthorizationResult::Failed;
	}

	let authority = match recover_authority(auth) {
		Ok(addr) => addr,
		Err(_) => {
			log::debug!(target: crate::LOG_TARGET, "Failed to recover authority from signature");
			return AuthorizationResult::Failed;
		},
	};

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

	if AccountInfo::<T>::is_contract(&authority) {
		log::debug!(
			target: crate::LOG_TARGET,
			"Account {authority:?} has non-delegation code",
		);
		return AuthorizationResult::Failed;
	}

	AuthorizationResult::Success { authority }
}

/// Recover the authority address from an authorization signature
///
/// Implements the EIP-7702 signature recovery:
/// - Message = keccak(MAGIC || rlp([chain_id, address, nonce]))
fn recover_authority(auth: &SignedAuthorizationListEntry) -> Result<H160, ()> {
	let mut message = Vec::new();
	message.push(EIP7702_MAGIC);

	let unsigned_auth = AuthorizationListEntry {
		chain_id: auth.chain_id,
		address: auth.address,
		nonce: auth.nonce,
	};

	let rlp_encoded = rlp::encode(&unsigned_auth);
	message.extend_from_slice(&rlp_encoded);

	let signature = auth.signature();
	recover_eth_address_from_message(&message, &signature)
}
