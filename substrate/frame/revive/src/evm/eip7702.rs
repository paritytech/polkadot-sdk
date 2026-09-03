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
	storage::{TransactionOutcome, with_transaction},
	traits::fungible::{Balanced as _, Inspect},
	weights::Weight,
};
use sp_core::{Get, H160, U256};
use sp_runtime::{
	SaturatedConversion,
	traits::{Saturating, Zero},
};

/// EIP-7702: Magic value for authorization signature message
const EIP7702_MAGIC: u8 = 0x05;

/// Result of processing EIP-7702 authorization tuples.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct AuthorizationResult<Balance: sp_runtime::traits::Zero> {
	/// Number of authorizations that created new accounts.
	pub new_accounts: u32,
	/// Number of authorizations that applied to existing accounts.
	pub existing_accounts: u32,
	/// Net deposit movement caused by authorization processing. `Charge` if more was charged than
	/// refunded (e.g. new-account ED + delegation deposits), `Refund` if revokes outweighed new
	/// charges (e.g. clearing delegations on existing accounts).
	pub deposit: StorageDeposit<Balance>,
	/// Weight to refund for authorizations that hit existing accounts.
	pub weight_refund: Weight,
}

/// Pre-dispatch worst-case weight for processing `n` EIP-7702 authorizations.
///
/// Must be used as the reservation in `#[pallet::weight(...)]` and as the baseline against
/// which the post-dispatch refund is computed, so that both expressions agree and the
/// refund accounting balances out to the actual cost.
///
/// Takes a component-wise `max` of the all-new and all-existing aggregations: on at least one
/// asset-hub runtime `process_existing_account_authorization` has a larger per-auth `proof_size`
/// than `process_new_account_authorization` (the populated trie node carries more witness data),
/// so neither dimension uniformly dominates.
pub fn worst_case_authorization_weight<T: Config>(n: u32) -> Weight {
	if n == 0 {
		return Weight::zero();
	}
	let all_new = <RuntimeCosts as metering::Token<T>>::weight(&RuntimeCosts::Delegations {
		new_accounts: n,
		existing_accounts: 0,
		invalid_accounts: 0,
	});
	let all_existing = <RuntimeCosts as metering::Token<T>>::weight(&RuntimeCosts::Delegations {
		new_accounts: 0,
		existing_accounts: n,
		invalid_accounts: 0,
	});
	all_new.max(all_existing)
}

/// Process a list of EIP-7702 authorization tuples.
///
/// For new accounts the ED is drawn from the transaction fee via `FeeInfo::withdraw_txfee` and
/// resolved into the account; the delegation deposit itself is charged via
/// [`Pallet::charge_deposit`].
/// The pre-dispatch weight reservation comes from [`worst_case_authorization_weight`]; the
/// returned `weight_refund` is the gap between that baseline and the actual cost incurred.
///
/// Note: We process authorizations OUTSIDE the transaction context so delegation changes persist
/// even if the call fails.
///
/// Returns the aggregated `AuthorizationResult` directly — every per-auth failure (spec
/// validation step or post-validation rollback) is handled by `continue` inside the loop,
/// so this function is structurally infallible from the caller's perspective.
pub fn process_authorizations<T: Config>(
	authorization_list: &[AuthorizationListEntry],
	origin: &T::AccountId,
	exec_config: &ExecConfig<T>,
) -> AuthorizationResult<BalanceOf<T>> {
	if authorization_list.is_empty() {
		return Default::default();
	}

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

		// Notify any active tracer about this authority before its state is mutated, so
		// prestate-diff consumers see the pre-revocation/pre-delegation code and nonce. Without
		// this, an authority that isn't otherwise referenced by the EVM call would either be
		// missing from the trace entirely or have its "pre" captured post-mutation.
		crate::tracing::if_tracing(|t| t.watch_address(&authority));

		// EIP-7702 spec: "If any step above fails, immediately stop processing the tuple and
		// continue to the next tuple." Step 8 (set code) is one such step, so wrap the whole
		// per-auth state-changing block in a transaction and skip the tuple on any error —
		// ED transfer, delegation, deposit, and nonce bump all commit together or not at all.
		let outcome = with_transaction(
			|| -> TransactionOutcome<Result<StorageDeposit<BalanceOf<T>>, sp_runtime::DispatchError>> {
				let inner = (|| -> Result<StorageDeposit<BalanceOf<T>>, sp_runtime::DispatchError> {
					if !account_exists {
						let credit = <T as Config>::FeeInfo::withdraw_txfee(ed)
							.ok_or(Error::<T>::StorageDepositNotEnoughFunds)?;
						<T as Config>::Currency::resolve(&account_id, credit)
							.map_err(|_| Error::<T>::StorageDepositNotEnoughFunds)?;
					}

					// Authorizations can be relayed by anyone, so the account that paid when the
					// delegation was set and the account submitting the next set/clear can
					// differ. The payer field on `AccountType::DelegatedEOA` records who paid
					// last so the refund flows back to them rather than to the current
					// submitter (under `PGasDeposit` mis-routing also strands the
					// `NativeDepositOf[(authority, original_payer)]` entry, because the
					// refund-side lookup keys on the destination).
					let old_payer = AccountInfo::<T>::get_delegation_payer(&authority);
					let change = if auth.address.is_zero() {
						AccountInfo::<T>::clear_delegation(&authority)?
					} else {
						AccountInfo::<T>::set_delegation(&authority, auth.address)?
					};
					let (previous, current) = (change.previous, change.current);

					// Same-payer path applies a net diff (avoids round-tripping through
					// `T::Deposit` twice, which under `PGasDeposit` would burn
					// `1 - RefundPercent` of the PGAS-held portion on every revisit).
					// Payer-change path fully refunds the old payer and fully charges the
					// new payer.
					// `origin` is the sole payer when it set the current deposit, or when there
					// is no recorded payer (fresh delegation / zero deposit).
					let origin_is_sole_payer =
						old_payer.as_ref() == Some(origin) || old_payer.is_none();
					if origin_is_sole_payer {
						if current >= previous {
							let diff = current.saturating_sub(previous);
							if !diff.is_zero() {
								Pallet::<T>::charge_deposit(
									HoldReason::StorageDepositReserve,
									origin,
									&account_id,
									diff,
									exec_config,
								)?;
							}
						} else {
							let diff = previous.saturating_sub(current);
							Pallet::<T>::refund_deposit(
								HoldReason::StorageDepositReserve,
								&account_id,
								exec_config.funds(origin),
								diff,
							)?;
						}
					} else {
						let old_payer = old_payer
							.as_ref()
							.expect("old_payer is Some in this branch; qed");
						if !previous.is_zero() {
							// Refund the recorded payer directly to their balance. Under eth-tx
							// `exec_config.funds(old_payer)` collapses to `Funds::TxFee`, whose
							// native arm drops the recipient and returns the deposit to the fee pot
							// (→ the current submitter), so a relayed clear/redelegate would never
							// reach `old_payer`. A direct `Funds::Balance` transfer honours the payer.
							Pallet::<T>::refund_deposit(
								HoldReason::StorageDepositReserve,
								&account_id,
								crate::deposit_payment::Funds::Balance(old_payer),
								previous,
							)?;
						}
						if !current.is_zero() {
							Pallet::<T>::charge_deposit(
								HoldReason::StorageDepositReserve,
								origin,
								&account_id,
								current,
								exec_config,
							)?;
						}
					}

					AccountInfo::<T>::set_delegation_payer(
						&authority,
						if current.is_zero() { None } else { Some(origin.clone()) },
					);

					frame_system::Pallet::<T>::inc_account_nonce(&account_id);
					// The deposit reported upward feeds `origin`'s metering budget, so it must
					// reflect only what `origin` paid. When `origin` is the sole payer this is the
					// signed diff against its own prior deposit. On the payer-change path
					// `previous` was refunded to a *different* account (`old_payer`); folding it
					// into the net here would credit `origin` for money it never paid and inflate
					// its deposit budget, so the net is the full charge to `origin`.
					let net = if origin_is_sole_payer {
						if current >= previous {
							StorageDeposit::Charge(current.saturating_sub(previous))
						} else {
							StorageDeposit::Refund(previous.saturating_sub(current))
						}
					} else {
						StorageDeposit::Charge(current)
					};
					Ok(net)
				})();

				match inner {
					Ok(d) => TransactionOutcome::Commit(Ok(d)),
					Err(e) => TransactionOutcome::Rollback(Err(e)),
				}
			},
		);

		let Ok(deposit) = outcome else {
			log::debug!(target: LOG_TARGET, "Authorization for {authority:?} failed post-validation, skipping");
			continue;
		};

		// `account_exists` is captured pre-transaction. If the auth committed and the account
		// didn't exist before, it was just created here — count it as new.
		if !account_exists {
			result.deposit = result.deposit.saturating_add(&StorageDeposit::Charge(ed));
			result.new_accounts += 1;
		} else {
			result.existing_accounts += 1;
		}
		result.deposit = result.deposit.saturating_add(&deposit);
	}

	// Weight accounting:
	//   worst case = N * (sig recovery + new-account creation)
	//   actual     = sig-recovery for invalid tuples
	//              + sig-recovery + existing-account work for existing tuples
	//              + sig-recovery + new-account work for new tuples
	//   refund     = worst - actual
	// where `invalid` = tuples that applied no state change: chain-id mismatch, failed
	// signature recovery, bad nonce, non-EOA authority, or post-validation rollback.
	let total = authorization_list.len() as u32;
	let invalid = total
		.saturating_sub(result.new_accounts)
		.saturating_sub(result.existing_accounts);
	let worst_case_weight = worst_case_authorization_weight::<T>(total);
	let actual_weight = <RuntimeCosts as metering::Token<T>>::weight(&RuntimeCosts::Delegations {
		new_accounts: result.new_accounts,
		existing_accounts: result.existing_accounts,
		invalid_accounts: invalid,
	});
	result.weight_refund = worst_case_weight.saturating_sub(actual_weight);

	result
}

/// Build the EIP-7702 signing message: `MAGIC || rlp([chain_id, address, nonce])`
fn signing_message(auth: &AuthorizationListEntry) -> Vec<u8> {
	let mut message = Vec::with_capacity(1 + 64);
	message.push(EIP7702_MAGIC);
	message.extend_from_slice(&auth.rlp_encode_unsigned());
	message
}

/// Recover the authority address from an authorization signature.
///
/// EIP-7702 mandates `y_parity ∈ {0, 1}`. The shared `sp_io::crypto::secp256k1_ecdsa_recover`
/// primitive accepts the legacy Bitcoin/pre-EIP-155 `v ∈ {27, 28}` convention and silently
/// normalises it to `{0, 1}`, which would let those values pass through here as valid 7702
/// signatures (spec deviation). Filter strictly before recovery so the per-tuple skip path in
/// `process_authorizations` catches them.
fn recover_authority(auth: &AuthorizationListEntry) -> Result<H160, ()> {
	if auth.y_parity.bits() > 1 {
		return Err(());
	}
	recover_eth_address_from_message(&signing_message(auth), &auth.signature())
}

/// Sign an authorization entry
///
/// This is a helper function for benchmarks and tests.
#[cfg(any(feature = "runtime-benchmarks", test))]
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
