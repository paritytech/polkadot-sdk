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
	evm::api::{recover_eth_address_from_message, AuthorizationListEntry},
	storage::AccountInfo,
	weights::WeightInfo,
	Config,
};
use alloc::vec::Vec;
use frame_support::{dispatch::DispatchResult, weights::WeightMeter};
use sp_core::{H160, U256};
use sp_runtime::SaturatedConversion;

/// EIP-7702: Magic value for authorization signature message
pub const EIP7702_MAGIC: u8 = 0x05;

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
/// Weight is charged progressively:
/// - First charges for checking if account exists
/// - Then charges for processing the authorization (existing or new account)
///
/// # Parameters
/// - `authorization_list`: List of authorization tuples to process
/// - `chain_id`: Current chain ID
/// - `meter`: Weight meter to charge weight from
///
/// # Returns
/// `Ok(())` on success, or `Err` if out of weight
pub fn process_authorizations<T: Config>(
	authorization_list: &[AuthorizationListEntry],
	chain_id: U256,
	meter: &mut WeightMeter,
) -> DispatchResult {
	for auth in authorization_list.iter() {
		// Charge weight for validation before processing
		meter
			.try_consume(T::WeightInfo::validate_authorization())
			.map_err(|_| crate::Error::<T>::OutOfGas)?;

		// Validate the authorization (also checks if account is new)
		let Some((authority, is_new_account)) = validate_authorization::<T>(auth, chain_id) else {
			continue;
		};

		// Charge weight for applying delegation based on account existence
		meter
			.try_consume(T::WeightInfo::apply_delegation(is_new_account as u32))
			.map_err(|_| crate::Error::<T>::OutOfGas)?;

		// Apply delegation
		apply_delegation::<T>(&authority, auth.address);
	}

	Ok(())
}

/// Validate a single authorization tuple
///
/// Returns the authority address and whether it's a new account if validation succeeds,
/// None otherwise. This is exposed for benchmarking purposes.
pub(crate) fn validate_authorization<T: Config>(
	auth: &AuthorizationListEntry,
	chain_id: U256,
) -> Option<(H160, bool)> {
	// Validate chain_id
	if !auth.chain_id.is_zero() && auth.chain_id != chain_id {
		log::debug!(
			target: crate::LOG_TARGET,
			"Invalid chain_id in authorization: expected {chain_id:?} or 0, got {:?}",
			auth.chain_id
		);
		return None;
	}

	// Validate nonce is within bounds
	if auth.nonce >= U256::from(u64::MAX) {
		log::debug!(
			target: crate::LOG_TARGET,
			"Authorization nonce too large: {:?}",
			auth.nonce
		);
		return None;
	}

	// Recover authority address from signature
	let authority = match recover_authority(auth) {
		Ok(addr) => addr,
		Err(_) => {
			log::debug!(target: crate::LOG_TARGET, "Failed to recover authority from signature");
			return None;
		},
	};

	// Verify nonce matches and check if account exists
	let account_id = T::AddressMapper::to_account_id(&authority);
	let is_new_account = !frame_system::Account::<T>::contains_key(&account_id);
	let current_nonce = frame_system::Pallet::<T>::account_nonce(&account_id);
	let expected_nonce = auth.nonce;

	if U256::from(current_nonce.saturated_into::<u64>()) != expected_nonce {
		log::debug!(
			target: crate::LOG_TARGET,
			"Nonce mismatch for {authority:?}: expected {current_nonce:?}, got {expected_nonce:?}",
		);
		return None;
	}

	// Verify account is not a contract (but delegated accounts are allowed)
	if AccountInfo::<T>::is_contract(&authority) {
		log::debug!(
			target: crate::LOG_TARGET,
			"Account {authority:?} has non-delegation code",
		);
		return None;
	}

	Some((authority, is_new_account))
}

/// Apply a delegation for a single authority
///
/// This is exposed for benchmarking purposes.
pub(crate) fn apply_delegation<T: Config>(authority: &H160, target_address: H160) {
	let account_id = T::AddressMapper::to_account_id(authority);

	// Apply delegation
	if target_address.is_zero() {
		let _ = AccountInfo::<T>::clear_delegation(authority);
	} else {
		if let Err(e) = AccountInfo::<T>::set_delegation(authority, target_address) {
			log::debug!(
				target: crate::LOG_TARGET,
				"Failed to set delegation for {authority:?}: {e:?}",
			);
			return;
		}
	}

	// Increment nonce
	frame_system::Pallet::<T>::inc_account_nonce(&account_id);
}

/// Recover the authority address from an authorization signature
///
/// Implements the EIP-7702 signature recovery:
/// - Message = keccak(MAGIC || rlp([chain_id, address, nonce]))
fn recover_authority(auth: &AuthorizationListEntry) -> Result<H160, ()> {
	let mut message = Vec::new();
	message.push(EIP7702_MAGIC);
	message.extend_from_slice(&auth.rlp_encode_unsigned());

	let signature = auth.signature();
	recover_eth_address_from_message(&message, &signature)
}

/// Sign an authorization entry
///
/// This is a helper function for benchmarks and tests.
///
/// # Parameters
/// - `signing_key`: The k256 signing key to sign with
/// - `chain_id`: Chain ID for the authorization
/// - `address`: Target address to delegate to
/// - `nonce`: Nonce for the authorization
#[cfg(any(test, feature = "runtime-benchmarks"))]
pub fn sign_authorization(
	signing_key: &k256::ecdsa::SigningKey,
	chain_id: U256,
	address: H160,
	nonce: U256,
) -> AuthorizationListEntry {
	use sp_core::keccak_256;

	// Create unsigned entry for RLP encoding
	let unsigned = AuthorizationListEntry {
		chain_id,
		address,
		nonce,
		y_parity: U256::zero(),
		r: U256::zero(),
		s: U256::zero(),
	};

	let mut message = Vec::new();
	message.push(EIP7702_MAGIC);
	message.extend_from_slice(&unsigned.rlp_encode_unsigned());

	let hash = keccak_256(&message);
	let (signature, recovery_id) =
		signing_key.sign_prehash_recoverable(&hash).expect("signing succeeds");

	AuthorizationListEntry {
		chain_id,
		address,
		nonce,
		y_parity: U256::from(recovery_id.to_byte()),
		r: U256::from_big_endian(&signature.r().to_bytes()),
		s: U256::from_big_endian(&signature.s().to_bytes()),
	}
}
