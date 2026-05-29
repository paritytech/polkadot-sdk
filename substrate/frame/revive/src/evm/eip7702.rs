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
	BalanceOf, Config, Error, ExecConfig, HoldReason, LOG_TARGET, Pallet, RuntimeCosts,
	address::AddressMapper,
	evm::{
		api::{AuthorizationListEntry, recover_eth_address_from_message},
		fees::InfoT as _,
	},
	metering,
	primitives::StorageDeposit,
	storage::AccountInfo,
};
use alloc::vec::Vec;
use frame_support::{
	traits::fungible::{Balanced as _, Inspect},
	weights::Weight,
};
use sp_core::{Get, H160, U256};
use sp_runtime::{SaturatedConversion, Saturating};

/// EIP-7702: Magic value for authorization signature message
const EIP7702_MAGIC: u8 = 0x05;

/// Result of processing EIP-7702 authorization tuples.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct AuthorizationResult<Balance: Default> {
	/// Number of authorizations that created new accounts.
	pub new_accounts: u32,
	/// Number of authorizations that applied to existing accounts.
	pub existing_accounts: u32,
	/// Total deposit charged from the origin during authorization processing.
	pub deposit: Balance,
	/// Weight to refund for authorizations that hit existing accounts.
	pub weight_refund: Weight,
}

/// Process a list of EIP-7702 authorization tuples.
///
/// For new accounts the ED is charged from `origin` via [`Pallet::charge_deposit`].
/// The pre-dispatch weight assumes all authorizations create new accounts (worst case).
/// The returned `weight_refund` accounts for authorizations that hit existing accounts.
///
/// Note: We process authorizations OUTSIDE the transaction context so delegation changes persist
/// even if the call fails.
pub fn process_authorizations<T: Config>(
	authorization_list: &[AuthorizationListEntry],
	origin: &T::AccountId,
	exec_config: &ExecConfig<T>,
) -> Result<AuthorizationResult<BalanceOf<T>>, sp_runtime::DispatchError> {
	let chain_id = U256::from(T::ChainId::get());
	let ed = <T::Currency as Inspect<T::AccountId>>::minimum_balance();
	let mut result: AuthorizationResult<BalanceOf<T>> = Default::default();

	for auth in authorization_list.iter() {
		if !auth.chain_id.is_zero() && auth.chain_id != chain_id {
			log::debug!(target: LOG_TARGET, "Invalid chain_id in authorization: expected {chain_id:?} or 0, got {:?}", auth.chain_id);
			continue;
		}

		let Ok(authority) = recover_authority(auth) else {
			log::debug!(target: LOG_TARGET, "Failed to recover authority from signature");
			continue;
		};
		let account_id = T::AddressMapper::to_account_id(&authority);

		let current_nonce: u64 =
			frame_system::Pallet::<T>::account_nonce(&account_id).saturated_into();
		let Ok::<u64, _>(expected_nonce) = auth.nonce.try_into() else {
			log::debug!(target: LOG_TARGET, "Authorization nonce too large: {:?}", auth.nonce);
			continue;
		};

		if current_nonce != expected_nonce {
			log::debug!(target: LOG_TARGET, "Nonce mismatch for {authority:?}: expected {expected_nonce:?}, got {current_nonce:?}");
			continue;
		}

		if AccountInfo::<T>::is_contract(&authority) {
			log::debug!(target: LOG_TARGET, "Account {authority:?} has non-delegation code");
			continue;
		}

		let account_exists = frame_system::Account::<T>::contains_key(&account_id);
		if auth.address.is_zero() && !account_exists {
			log::debug!(target: LOG_TARGET, "Skipping clear delegation for non-existent account {authority:?}");
			continue;
		}

		if !account_exists {
			// Transfer ED to the new authority account without placing a hold:
			// the ED must remain as transferable balance so the account exists.
			// Funded from the tx fee pool (process_authorizations only runs from
			// eth-tx contexts).
			let credit =
				<T as Config>::FeeInfo::withdraw_txfee(ed).ok_or(Error::<T>::StorageDepositNotEnoughFunds)?;
			<T as Config>::Currency::resolve(&account_id, credit)
				.map_err(|_| Error::<T>::StorageDepositNotEnoughFunds)?;
			result.deposit.saturating_accrue(ed);
			result.new_accounts += 1;
		} else {
			result.existing_accounts += 1;
		}

		// Apply delegation
		let deposit = if auth.address.is_zero() {
			AccountInfo::<T>::clear_delegation(&authority)
		} else {
			AccountInfo::<T>::set_delegation(&authority, auth.address)
		};

		let Ok(deposit) = deposit else {
			log::debug!(target: LOG_TARGET, "Delegation failed for {authority:?}, skipping");
			continue;
		};

		match deposit {
			StorageDeposit::Charge(amount) => {
				Pallet::<T>::charge_deposit(
					HoldReason::StorageDepositReserve,
					origin,
					&account_id,
					amount,
					exec_config,
				)?;
				result.deposit.saturating_accrue(amount);
			},
			StorageDeposit::Refund(amount) => {
				Pallet::<T>::refund_deposit(
					HoldReason::StorageDepositReserve,
					&account_id,
					exec_config.funds(origin),
					amount,
				)?;
				result.deposit = result.deposit.saturating_sub(amount);
			},
		}

		frame_system::Pallet::<T>::inc_account_nonce(&account_id);
	}

	let worst_case_weight =
		<RuntimeCosts as metering::Token<T>>::weight(&RuntimeCosts::Delegations {
			new_accounts: authorization_list.len() as u32,
			existing_accounts: 0,
		});
	let actual_weight = <RuntimeCosts as metering::Token<T>>::weight(&RuntimeCosts::Delegations {
		new_accounts: result.new_accounts,
		existing_accounts: result.existing_accounts,
	});
	result.weight_refund = worst_case_weight.saturating_sub(actual_weight);

	Ok(result)
}

/// Build the EIP-7702 signing message: `MAGIC || rlp([chain_id, address, nonce])`
fn signing_message(auth: &AuthorizationListEntry) -> Vec<u8> {
	let mut message = Vec::with_capacity(1 + 64);
	message.push(EIP7702_MAGIC);
	message.extend_from_slice(&auth.rlp_encode_unsigned());
	message
}

/// Recover the authority address from an authorization signature
fn recover_authority(auth: &AuthorizationListEntry) -> Result<H160, ()> {
	recover_eth_address_from_message(&signing_message(auth), &auth.signature())
}

/// Sign an authorization entry
///
/// This is a helper function for benchmarks and tests.
#[cfg(any(feature = "std", feature = "runtime-benchmarks"))]
pub fn sign_authorization(
	key: &k256::ecdsa::SigningKey,
	chain_id: U256,
	address: H160,
	nonce: U256,
) -> AuthorizationListEntry {
	let unsigned = AuthorizationListEntry { chain_id, address, nonce, ..Default::default() };
	let hash = sp_io::hashing::keccak_256(&signing_message(&unsigned));
	let (signature, recovery_id) =
		key.sign_prehash_recoverable(&hash).expect("signing success; qed");

	let sig_bytes = signature.to_bytes();
	AuthorizationListEntry {
		chain_id,
		address,
		nonce,
		y_parity: U256::from(recovery_id.to_byte()),
		r: U256::from_big_endian(&sig_bytes[..32]),
		s: U256::from_big_endian(&sig_bytes[32..64]),
	}
}

/// Derive the Ethereum address from a signing key.
///
/// This is a helper function for benchmarks and tests.
#[cfg(any(feature = "runtime-benchmarks", test))]
pub fn eth_address(key: &k256::ecdsa::SigningKey) -> H160 {
	let public_key = key.verifying_key();
	let encoded = public_key.to_encoded_point(false);
	// Skip the 0x04 prefix byte to get the uncompressed public key
	H160::from_slice(&sp_io::hashing::keccak_256(&encoded.as_bytes()[1..])[12..])
}
