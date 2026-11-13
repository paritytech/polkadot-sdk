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

//! # Multi-Block Migration v4
//!
//! Switches storage deposits from the native currency to PGAS.
//!
//! After the migration, for every account holding a deposit and its hold reason, the native part
//! on hold equals the sum of the account's [`crate::NativeDepositOf`] rows, and the total on hold
//! is unchanged. `refund_on_hold` refunds a payer's row from the native hold
//! and everything else from PGAS. `reconcile` burns the native
//! hold the rows do not account for and replace it with the same amount of PGAS held under the
//! same reason.
//!
//! Phase 1 `reconcile` the pallet account's `CodeUploadDepositReserve` hold, which carries every
//! code deposit. [`crate::CodeInfoOf`] does not record which asset paid a code deposit, so the
//! rows cannot be rebuilt from it in general. The one case where they can is a chain that has not
//! enabled `PGasDeposit` yet: the pallet account then has no rows and no PGAS on hold, every code
//! deposit was paid in native, and phase 1 records the uploader's deposit under
//! [`crate::NativeDepositOf`] for each code instead, leaving the native hold in place.
//!
//! Phase 2 iterates [`crate::AccountInfoOf`] and reconciles the `StorageDepositReserve` hold of
//! every contract and of every EIP-7702 delegated account. A deposit that has no row was paid in
//! native before `PGasDeposit` was enabled and is refunded in PGAS from then on.
//!
//! A PGAS hold needs the PGAS existential deposit free beside it, so every reconciled account is
//! given one, frozen, as a new contract is.
//!
//! A contract, or the pallet account for code deposits, has one [`crate::NativeDepositOf`] row per
//! payer, and nothing bounds how many. Every iteration of a step therefore does one item of work
//! and is charged its own benchmarked weight: a code entry, an account, one row, the swap once
//! the rows are summed, or a deletion-queue slot. The cursor keeps the payer of the last row read
//! and the native summed so far ([`RowSum`]), and nothing is written until the swap. Revive
//! refuses state changes while migrations run, so the rows cannot change between iterations.
//!
//! Phase 3 rewrites the [`crate::DeletionQueue`] entries from their old `TrieId` value into the
//! new [`crate::storage::DeletionQueueItem`] format. Entries already in the new format, queued by
//! contracts terminated after the runtime upgrade but before this migration ran, are left as
//! they are: they name the real contract account whose [`crate::NativeDepositOf`] rows the
//! deletion processor must clear.
//!
//! Phases 1 and 2 are skipped when [`crate::Config::Deposit`] does not support PGAS (i.e. the
//! default `()` backend); only phase 3 runs in that case.
//!
//! The migration never fails on chain: a failed multi-block migration is handled by the
//! runtime's `FailedMigrationHandler`, which commonly freezes the chain. A swap to PGAS that
//! fails (for example because the PGAS asset is not registered) rolls back, is logged, and leaves
//! that account on native. `pre_upgrade` rejects a missing PGAS asset and `post_upgrade` rejects
//! any account left on native, so `try-runtime` catches both before the runtime ships.

use super::PALLET_MIGRATIONS_ID;
use crate::{
	AccountInfoOf, BalanceOf, CodeInfoOf, Config, DeletionQueue, HoldReason, LOG_TARGET,
	NativeDepositOf, Pallet, TrieId,
	address::AddressMapper,
	deposit_payment::Deposit,
	storage::{AccountType, DeletionQueueItem},
	weights::WeightInfo,
};
use codec::{Decode, DecodeAll, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{
	Twox64Concat,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	storage_alias,
	weights::WeightMeter,
};
use scale_info::TypeInfo;
use sp_core::{H160, H256};
use sp_runtime::traits::{Get, Saturating, TrailingZeroInput, Zero};

extern crate alloc;

#[cfg(feature = "try-runtime")]
use alloc::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

/// The [`NativeDepositOf`] rows of one account that a swap has summed so far.
///
/// An account can have one row per payer, so the sum is carried across steps instead of reading
/// every row in one.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Debug)]
pub struct RowSum<AccountId, Balance> {
	/// Payer of the last row summed. The next step resumes after it.
	last_payer: Option<AccountId>,
	/// Native owed by the rows summed so far.
	owed: Balance,
	/// Whether no rows are left, so the next iteration swaps.
	complete: bool,
}

impl<AccountId, Balance: Zero> RowSum<AccountId, Balance> {
	/// A sum that has read no rows yet.
	pub fn new() -> Self {
		Self { last_payer: None, owed: Zero::zero(), complete: false }
	}
}

/// Cursor: code deposits, contracts and delegated accounts, and the deletion-queue rewrite.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Debug)]
pub enum Cursor<AccountId, Balance> {
	/// Last code hash processed in phase 1 (`CodeInfoOf` iteration).
	CodeUpload(H256),
	/// Last account address processed in phase 2 (`AccountInfoOf` iteration).
	///
	/// `None` is the transition sentinel from phase 1 to phase 2.
	Contract(Option<H160>),
	/// Last deletion-queue index processed in phase 3 (`DeletionQueue` rewrite).
	///
	/// `None` is the transition sentinel from phase 2 to phase 3.
	DeletionQueue(Option<u32>),
	/// Summing the pallet account's [`NativeDepositOf`] rows in phase 1, before swapping its
	/// unrecorded native code deposits.
	CodeRows(RowSum<AccountId, Balance>),
	/// Summing the [`NativeDepositOf`] rows of the account at this address in phase 2, before
	/// swapping its unrecorded native storage deposit.
	ContractRows(H160, RowSum<AccountId, Balance>),
}

/// The [`Cursor`] for a runtime's account id and balance.
pub type CursorOf<T> = Cursor<<T as frame_system::Config>::AccountId, BalanceOf<T>>;

/// Switches native storage deposits over to PGAS.
pub struct Migration<T>(PhantomData<T>);

impl<T: Config> SteppedMigration for Migration<T> {
	type Cursor = CursorOf<T>;
	type Identifier = MigrationId<17>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 3, version_to: 4 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = Self::required_weight();
		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if meter.try_consume(Self::iteration_weight(&cursor)).is_err() {
				break;
			}
			cursor = Self::step_once(cursor);
			if cursor.is_none() {
				break;
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use crate::deposit_payment::Deposit;

		// The pallet account's rows expected after phase 1, whether phase 1 swaps its unrecorded
		// native instead of rebuilding them, and its code-deposit hold.
		let pallet_account = Pallet::<T>::account_id();
		let code_hold =
			T::Deposit::total_on_hold(HoldReason::CodeUploadDepositReserve, &pallet_account);
		let mut per_owner: BTreeMap<T::AccountId, BalanceOf<T>> = BTreeMap::new();
		let mut swap_code = false;
		if T::Deposit::SUPPORTS_PGAS {
			frame_support::ensure!(
				T::Deposit::pgas_asset_exists(),
				"v4: the PGAS asset is not registered; phase 2 would convert nothing",
			);
			swap_code = Self::code_deposits_need_reconcile();
			if swap_code {
				per_owner = NativeDepositOf::<T>::iter_prefix(&pallet_account).collect();
			} else {
				for (_hash, info) in CodeInfoOf::<T>::iter() {
					let entry = per_owner.entry(info.owner().clone()).or_default();
					*entry = entry.saturating_add(info.deposit());
				}
			}
		}

		// Per contract and delegated account: total deposit on hold and the native amount owed by
		// its rows. Without PGAS support phase 2 does not run and there is nothing to check.
		let mut per_contract: BTreeMap<H160, (BalanceOf<T>, BalanceOf<T>)> = BTreeMap::new();
		for (addr, info) in AccountInfoOf::<T>::iter() {
			if !T::Deposit::SUPPORTS_PGAS || matches!(info.account_type, AccountType::EOA) {
				continue;
			}
			let contract = T::AddressMapper::to_account_id(&addr);
			let total = T::Deposit::total_on_hold(HoldReason::StorageDepositReserve, &contract);
			per_contract.insert(addr, (total, Self::recorded_native(&contract)));
		}

		// Per slot: the item expected after phase 3. Legacy slots get the zero placeholder;
		// slots already in the new format must keep their contract account.
		let deletion_queue: BTreeMap<u32, DeletionQueueItem<T>> =
			old::DeletionQueue::<T>::iter_keys()
				.filter_map(|key| match Self::legacy_trie_id(key) {
					Some(trie_id) => {
						Some((key, DeletionQueueItem::new(trie_id, Self::zero_account())))
					},
					None => DeletionQueue::<T>::get(key).map(|item| (key, item)),
				})
				.collect();

		Ok((per_owner, swap_code, code_hold, per_contract, deletion_queue).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use crate::deposit_payment::Deposit;

		let (per_owner, swap_code, code_hold, per_contract, deletion_queue) =
			<(
				BTreeMap<T::AccountId, BalanceOf<T>>,
				bool,
				BalanceOf<T>,
				BTreeMap<H160, (BalanceOf<T>, BalanceOf<T>)>,
				BTreeMap<u32, DeletionQueueItem<T>>,
			)>::decode(&mut &prev[..])
			.expect("Failed to decode pre_upgrade state");

		// Without PGAS support phases 1 and 2 do not run, so only phase 3 is checked.
		if T::Deposit::SUPPORTS_PGAS {
			Self::check_deposits(per_owner, swap_code, code_hold, per_contract)?;
		}

		for (key, expected) in deletion_queue {
			let got = DeletionQueue::<T>::get(key);
			if got.as_ref() != Some(&expected) {
				log::error!(
					target: LOG_TARGET,
					"v4: DeletionQueue[{key}] = {got:?}, expected {expected:?}",
				);
				return Err("v4: DeletionQueue entry does not match the expected item".into());
			}
		}
		Ok(())
	}
}

/// Pre-v4 storage layouts.
pub(crate) mod old {
	use super::*;

	/// Pre-v4 layout: a single [`TrieId`] per slot, no associated contract account. We only
	/// iterate it; new entries are written via the live [`crate::DeletionQueue`].
	#[storage_alias]
	pub(crate) type DeletionQueue<T: Config> = StorageMap<Pallet<T>, Twox64Concat, u32, TrieId>;
}

impl<T: Config> Migration<T> {
	/// The weight of the most expensive iteration, which a block must have room for.
	fn required_weight() -> frame_support::weights::Weight {
		let first = Self::iteration_weight(&None);
		[
			<T as Config>::WeightInfo::v4_account_step(),
			<T as Config>::WeightInfo::v4_row_step(),
			<T as Config>::WeightInfo::v4_swap_step(),
			<T as Config>::WeightInfo::v4_deletion_queue_step(),
		]
		.into_iter()
		.fold(first, |max, weight| max.max(weight))
	}

	/// The weight of the next iteration from `cursor`.
	fn iteration_weight(cursor: &Option<CursorOf<T>>) -> frame_support::weights::Weight {
		match cursor {
			// Decides between rebuilding and reconciling, then does the first code entry.
			None => <T as Config>::WeightInfo::v4_code_upload_step()
				.saturating_add(T::DbWeight::get().reads(3)),
			Some(Cursor::CodeUpload(_)) => <T as Config>::WeightInfo::v4_code_upload_step(),
			Some(Cursor::Contract(_)) => <T as Config>::WeightInfo::v4_account_step(),
			Some(Cursor::CodeRows(sum)) | Some(Cursor::ContractRows(_, sum)) if sum.complete => {
				<T as Config>::WeightInfo::v4_swap_step()
			},
			Some(Cursor::CodeRows(_)) | Some(Cursor::ContractRows(..)) => {
				<T as Config>::WeightInfo::v4_row_step()
			},
			Some(Cursor::DeletionQueue(_)) => <T as Config>::WeightInfo::v4_deletion_queue_step(),
		}
	}

	/// Sum of the [`NativeDepositOf`] rows recorded against `holder`. Reads every row, so it is
	/// only for `try-runtime` checks and tests, never for a migration step.
	#[cfg(any(feature = "try-runtime", test))]
	fn recorded_native(holder: &T::AccountId) -> BalanceOf<T> {
		NativeDepositOf::<T>::iter_prefix_values(holder)
			.fold(BalanceOf::<T>::zero(), |sum, v| sum.saturating_add(v))
	}

	/// The deposit half of `post_upgrade`: every holder v4 visited holds exactly the native its
	/// rows owe and the same total as before, and no holder was left unvisited.
	#[cfg(feature = "try-runtime")]
	fn check_deposits(
		per_owner: BTreeMap<T::AccountId, BalanceOf<T>>,
		swap_code: bool,
		code_hold: BalanceOf<T>,
		per_contract: BTreeMap<H160, (BalanceOf<T>, BalanceOf<T>)>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		// Phase 1 either rebuilds the pallet account's rows from `CodeInfoOf` alone or, when it
		// swaps the unrecorded native, keeps the rows it found.
		let pallet_account = Pallet::<T>::account_id();
		for (owner, got) in NativeDepositOf::<T>::iter_prefix(&pallet_account) {
			if !per_owner.contains_key(&owner) {
				log::error!(
					target: LOG_TARGET,
					"v4: NativeDepositOf[pallet][{owner:?}] = {got:?} left without code in CodeInfoOf",
				);
				return Err("v4: pallet-account NativeDepositOf row left without code".into());
			}
		}
		for (owner, expected) in per_owner {
			let got = NativeDepositOf::<T>::get(&pallet_account, &owner);
			if got != expected {
				log::error!(
					target: LOG_TARGET,
					"v4: NativeDepositOf[pallet][{owner:?}] = {got:?}, expected {expected:?}",
				);
				return Err("v4: pallet-account NativeDepositOf row does not match".into());
			}
		}
		{
			use frame_support::traits::fungible::InspectHold;

			let reason = HoldReason::CodeUploadDepositReserve;
			let total = T::Deposit::total_on_hold(reason, &pallet_account);
			if total != code_hold {
				log::error!(
					target: LOG_TARGET,
					"v4: pallet account code deposits changed: {total:?} != pre-migration {code_hold:?}",
				);
				return Err("v4: pallet account code deposits changed".into());
			}
			// `remove_code` refunds these rows from the native hold with `Precision::Exact`.
			// After a swap the native hold is exactly the rows; a skipped swap leaves more.
			let credited = Self::recorded_native(&pallet_account);
			let native = T::Currency::balance_on_hold(&reason.into(), &pallet_account);
			if credited > native || (swap_code && credited != native) {
				log::error!(
					target: LOG_TARGET,
					"v4: pallet account credits {credited:?} native code deposits but holds {native:?}",
				);
				return Err("v4: pallet account native code deposits do not match its rows".into());
			}
		}

		for (addr, (expected, owed_native)) in &per_contract {
			let (expected, owed_native) = (*expected, *owed_native);
			use frame_support::traits::fungible::InspectHold;

			let contract = T::AddressMapper::to_account_id(&addr);
			let total = T::Deposit::total_on_hold(HoldReason::StorageDepositReserve, &contract);
			if total != expected {
				log::error!(
					target: LOG_TARGET,
					"v4: contract {addr:?} total_on_hold changed: {total:?} != pre-migration {expected:?}",
				);
				return Err("v4: contract total_on_hold changed".into());
			}
			let rows = Self::recorded_native(&contract);
			if rows != owed_native {
				log::error!(
					target: LOG_TARGET,
					"v4: contract {addr:?} NativeDepositOf rows changed: {rows:?} != {owed_native:?}",
				);
				return Err("v4: contract NativeDepositOf rows changed".into());
			}
			// Only the unrecorded part is swapped, so what stays native is exactly what the
			// rows say is owed in native. A skipped swap leaves more native than that.
			let native =
				T::Currency::balance_on_hold(&HoldReason::StorageDepositReserve.into(), &contract);
			if native != owed_native {
				log::error!(
					target: LOG_TARGET,
					"v4: contract {addr:?} native hold {native:?} != recorded native {owed_native:?}",
				);
				return Err(
					"v4: contract native hold does not match its NativeDepositOf rows".into()
				);
			}
		}

		// Every account that can hold a storage deposit was visited.
		for (addr, info) in AccountInfoOf::<T>::iter() {
			if !matches!(info.account_type, AccountType::EOA) && !per_contract.contains_key(&addr) {
				log::error!(target: LOG_TARGET, "v4: account {addr:?} was not visited");
				return Err("v4: account with a storage deposit was not visited".into());
			}
		}
		// Rows only exist on holders v4 visited, or on terminated contracts awaiting deletion.
		let visited: BTreeSet<T::AccountId> = per_contract
			.keys()
			.map(T::AddressMapper::to_account_id)
			.chain(DeletionQueue::<T>::iter_values().map(|item| item.account_id))
			.chain(core::iter::once(pallet_account))
			.collect();
		for (holder, _, _) in NativeDepositOf::<T>::iter() {
			if !visited.contains(&holder) {
				log::error!(target: LOG_TARGET, "v4: NativeDepositOf rows on unvisited {holder:?}");
				return Err("v4: NativeDepositOf rows on an account v4 did not visit".into());
			}
		}
		Ok(())
	}

	/// PGAS the pallet account holds under `CodeUploadDepositReserve`, i.e. code deposits that
	/// were paid in PGAS.
	fn pgas_code_deposits() -> BalanceOf<T> {
		use frame_support::traits::fungible::InspectHold;

		let reason = HoldReason::CodeUploadDepositReserve;
		let pallet_account = Pallet::<T>::account_id();
		let native = T::Currency::balance_on_hold(&reason.into(), &pallet_account);
		T::Deposit::total_on_hold(reason, &pallet_account).saturating_sub(native)
	}

	/// Whether phase 1 has to reconcile the pallet account's code deposits instead of rebuilding
	/// their rows from [`CodeInfoOf`].
	///
	/// Rebuilding is only right when every code deposit was paid in native and none has a row yet,
	/// which is the case until `PGasDeposit` is enabled.
	fn code_deposits_need_reconcile() -> bool {
		!Self::pgas_code_deposits().is_zero() ||
			NativeDepositOf::<T>::iter_key_prefix(&Pallet::<T>::account_id())
				.next()
				.is_some()
	}

	/// Run a single iteration of the migration's inner loop, returning the next cursor or
	/// `None` if the migration is complete.
	fn step_once(cursor: Option<CursorOf<T>>) -> Option<CursorOf<T>> {
		// Without PGAS support phases 1 and 2 are no-ops, so skip straight to phase 3.
		if !T::Deposit::SUPPORTS_PGAS {
			return match cursor {
				Some(Cursor::DeletionQueue(last)) => {
					Self::step_3_deletion_queue(last).map(|next| Cursor::DeletionQueue(Some(next)))
				},
				_ => Some(Cursor::DeletionQueue(None)),
			};
		}

		match cursor {
			None if Self::code_deposits_need_reconcile() => Some(Cursor::CodeRows(RowSum::new())),
			None => Self::step_1_code_upload(None),
			Some(Cursor::CodeUpload(last)) => Self::step_1_code_upload(Some(last)),
			Some(Cursor::CodeRows(sum)) => Some(
				match Self::reconcile_once(
					HoldReason::CodeUploadDepositReserve,
					&Pallet::<T>::account_id(),
					sum,
				) {
					Some(sum) => Cursor::CodeRows(sum),
					None => Cursor::Contract(None),
				},
			),
			Some(Cursor::Contract(last)) => Some(Self::step_2_account(last)),
			Some(Cursor::ContractRows(address, sum)) => Some(
				match Self::reconcile_once(
					HoldReason::StorageDepositReserve,
					&T::AddressMapper::to_account_id(&address),
					sum,
				) {
					Some(sum) => Cursor::ContractRows(address, sum),
					None => Cursor::Contract(Some(address)),
				},
			),
			Some(Cursor::DeletionQueue(last)) => {
				Self::step_3_deletion_queue(last).map(|next| Cursor::DeletionQueue(Some(next)))
			},
		}
	}

	/// One reconcile iteration for `who`'s `reason` hold: sum the next [`NativeDepositOf`] row
	/// into `sum`, or once none is left, swap. Returns the sum to continue with, or `None` once
	/// the swap is done.
	pub fn reconcile_once(
		reason: HoldReason,
		who: &T::AccountId,
		mut sum: RowSum<T::AccountId, BalanceOf<T>>,
	) -> Option<RowSum<T::AccountId, BalanceOf<T>>> {
		if sum.complete {
			Self::swap_unrecorded_native(reason, who, sum.owed);
			return None;
		}

		let mut rows = match &sum.last_payer {
			Some(last) => NativeDepositOf::<T>::iter_prefix_from(
				who,
				NativeDepositOf::<T>::hashed_key_for(who, last),
			),
			None => NativeDepositOf::<T>::iter_prefix(who),
		};
		match rows.next() {
			Some((payer, amount)) => {
				sum.owed = sum.owed.saturating_add(amount);
				sum.last_payer = Some(payer);
			},
			None => sum.complete = true,
		}
		Some(sum)
	}

	/// Swap the part of `who`'s native hold under `reason` that `owed_native`, the sum of its
	/// [`NativeDepositOf`] rows, does not account for to PGAS.
	///
	/// The rows were written by `PGasDeposit` for native paid after it was enabled, and
	/// `refund_on_hold` refunds them from the native hold with `Precision::Exact`; swapping that
	/// native would leave those payers unable to be refunded. A failed swap rolls back and is
	/// logged, leaving `who` on native; `post_upgrade` reports it.
	pub fn swap_unrecorded_native(
		reason: HoldReason,
		who: &T::AccountId,
		owed_native: BalanceOf<T>,
	) {
		use frame_support::traits::fungible::InspectHold;

		let held = T::Currency::balance_on_hold(&reason.into(), who);
		if owed_native > held {
			log::warn!(
				target: LOG_TARGET,
				"v4: {who:?} records {owed_native:?} native owed under {reason:?} but holds {held:?}",
			);
		}
		let to_swap = held.saturating_sub(owed_native);
		if let Err(err) = T::Deposit::migrate_native_to_pgas(reason, who, to_swap) {
			log::error!(
				target: LOG_TARGET,
				"v4: failed to migrate native -> PGAS {reason:?} deposit of {who:?}: {err:?}",
			);
		}
	}

	/// Phase 1: credit the next `CodeInfoOf` entry's owner in [`NativeDepositOf`]. Returns
	/// `Some(Cursor::Contract(None))` when phase 1 is exhausted.
	pub fn step_1_code_upload(last: Option<H256>) -> Option<CursorOf<T>> {
		let mut iter = match last {
			Some(last) => CodeInfoOf::<T>::iter_from(CodeInfoOf::<T>::hashed_key_for(last)),
			None => CodeInfoOf::<T>::iter(),
		};

		let Some((hash, info)) = iter.next() else { return Some(Cursor::Contract(None)) };

		let pallet_account = Pallet::<T>::account_id();
		NativeDepositOf::<T>::mutate(&pallet_account, info.owner(), |entitlement| {
			*entitlement = entitlement.saturating_add(info.deposit());
		});
		Some(Cursor::CodeUpload(hash))
	}

	/// Phase 2: move to the next account after `last`. A contract or delegated account continues
	/// with its rows, anything else is skipped. Returns phase 3 once there are no accounts left.
	pub fn step_2_account(last: Option<H160>) -> CursorOf<T> {
		let mut iter = match last {
			Some(last) => AccountInfoOf::<T>::iter_from(AccountInfoOf::<T>::hashed_key_for(last)),
			None => AccountInfoOf::<T>::iter(),
		};

		let Some((addr, info)) = iter.next() else { return Cursor::DeletionQueue(None) };
		if matches!(info.account_type, AccountType::EOA) {
			Cursor::Contract(Some(addr))
		} else {
			Cursor::ContractRows(addr, RowSum::new())
		}
	}

	/// Phase 3: rewrite the next [`DeletionQueue`] slot from the old `TrieId`-only layout
	/// into the new [`DeletionQueueItem`] format. Pre-v4 entries had no [`NativeDepositOf`]
	/// rows, so the recorded `account_id` is a zero placeholder; phase 1 of the deletion
	/// processor will clear an empty prefix on it. Slots already in the new format are left
	/// unchanged. Returns `None` when phase 3 finishes.
	pub fn step_3_deletion_queue(last: Option<u32>) -> Option<u32> {
		let mut keys = match last {
			Some(last) => old::DeletionQueue::<T>::iter_keys_from(
				old::DeletionQueue::<T>::hashed_key_for(last),
			),
			None => old::DeletionQueue::<T>::iter_keys(),
		};

		let key = keys.next()?;
		if let Some(trie_id) = Self::legacy_trie_id(key) {
			// Same physical slot as `old::DeletionQueue`; the insert overwrites the legacy value
			// with the new encoding.
			DeletionQueue::<T>::insert(
				key,
				DeletionQueueItem::<T>::new(trie_id, Self::zero_account()),
			);
		}
		Some(key)
	}

	/// The [`TrieId`] stored at `DeletionQueue[key]` if the slot is still in the legacy layout,
	/// or `None` if it is already a [`DeletionQueueItem`] (or holds neither).
	///
	/// The value is decoded with `decode_all`. A plain `decode` as `TrieId` accepts a new-format
	/// item and drops its trailing account. The layouts cannot be confused: a legacy value is
	/// one `AccountId` short of a `DeletionQueueItem`, and a `DeletionQueueItem` leaves the
	/// account bytes over when decoded as a `TrieId`.
	fn legacy_trie_id(key: u32) -> Option<TrieId> {
		let raw =
			frame_support::storage::unhashed::get_raw(&DeletionQueue::<T>::hashed_key_for(key))?;
		if DeletionQueueItem::<T>::decode_all(&mut &raw[..]).is_ok() {
			return None;
		}
		let trie_id = TrieId::decode_all(&mut &raw[..]);
		if trie_id.is_err() {
			log::error!(target: LOG_TARGET, "v4: DeletionQueue[{key}] decodes in neither layout");
		}
		trie_id.ok()
	}

	/// The placeholder account recorded for legacy deletion-queue entries.
	fn zero_account() -> T::AccountId {
		T::AccountId::decode(&mut TrailingZeroInput::zeroes())
			.expect("zero input decodes to a valid AccountId; qed")
	}
}

#[cfg(any(feature = "runtime-benchmarks", feature = "try-runtime", test))]
impl<T: Config> Migration<T> {
	/// Drive the migration to completion. Test/benchmark helper.
	pub fn run_to_completion() {
		let mut cursor: Option<CursorOf<T>> = None;
		let mut meter = WeightMeter::new();
		while let Ok(Some(next)) = <Self as SteppedMigration>::step(cursor, &mut meter) {
			cursor = Some(next);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		CodeInfo, FreezeReason,
		storage::{AccountInfo, ContractInfo},
		tests::{
			Assets, AssetsFreezer, AssetsHolder, ExtBuilder, PGasAssetId, RuntimeOrigin, Test,
		},
	};
	use frame_support::{
		assert_ok,
		traits::fungible::{Inspect as _, InspectHold as _, Mutate as _, MutateHold as _},
		weights::Weight,
	};
	use sp_runtime::AccountId32;

	type V4 = Migration<Test>;

	fn seed_code_upload(hash: H256, owner: AccountId32, deposit: u128) {
		let pallet_account = Pallet::<Test>::account_id();
		let ed = <Test as Config>::Currency::minimum_balance();
		<Test as Config>::Currency::mint_into(&pallet_account, ed).unwrap();
		<Test as Config>::Currency::mint_into(&pallet_account, deposit).unwrap();
		<Test as Config>::Currency::hold(
			&HoldReason::CodeUploadDepositReserve.into(),
			&pallet_account,
			deposit,
		)
		.unwrap();
		CodeInfoOf::<Test>::insert(hash, CodeInfo::<Test>::new_with_deposit(owner, deposit));
	}

	fn seed_contract(address: H160, code_hash: H256, storage_deposit: u128) {
		let contract_account = <Test as Config>::AddressMapper::to_account_id(&address);
		let info = ContractInfo::<Test>::new(&address, 0u32.into(), code_hash).unwrap();
		AccountInfoOf::<Test>::insert(
			address,
			AccountInfo::<Test> { account_type: AccountType::Contract(info), dust: 0 },
		);

		let ed = <Test as Config>::Currency::minimum_balance();
		<Test as Config>::Currency::mint_into(&contract_account, ed).unwrap();
		<Test as Config>::Currency::mint_into(&contract_account, storage_deposit).unwrap();
		<Test as Config>::Currency::hold(
			&HoldReason::StorageDepositReserve.into(),
			&contract_account,
			storage_deposit,
		)
		.unwrap();
	}

	/// Seeds the pallet account as a chain that enabled `PGasDeposit` before v4 leaves it: code
	/// paid in native before the switch (no row), in native after it (row), and in PGAS after it
	/// (no row, held in PGAS).
	fn seed_code_uploads_paid_in_both(
		legacy_owner: &AccountId32,
		native_owner: &AccountId32,
		pgas_owner: &AccountId32,
	) {
		use frame_support::traits::tokens::fungibles::{
			Inspect as _, Mutate as _, MutateHold as _,
		};

		let pallet_account = Pallet::<Test>::account_id();
		seed_code_upload(H256::repeat_byte(0xAA), legacy_owner.clone(), 1_000);
		seed_code_upload(H256::repeat_byte(0xAB), native_owner.clone(), 500);
		NativeDepositOf::<Test>::insert(&pallet_account, native_owner, 500);

		let pgas_ed = Assets::minimum_balance(PGasAssetId::get());
		Assets::mint_into(PGasAssetId::get(), &pallet_account, pgas_ed + 400).unwrap();
		AssetsHolder::hold(
			PGasAssetId::get(),
			&HoldReason::CodeUploadDepositReserve.into(),
			&pallet_account,
			400,
		)
		.unwrap();
		CodeInfoOf::<Test>::insert(
			H256::repeat_byte(0xBB),
			CodeInfo::<Test>::new_with_deposit(pgas_owner.clone(), 400),
		);
	}

	/// `CodeInfoOf` cannot tell code paid in PGAS from code paid in native before the switch, so
	/// rebuilding the rows would credit PGAS-paid code as native. Instead v4 keeps the existing
	/// rows and swaps the unrecorded native to PGAS, and every owner can remove their code.
	#[test]
	fn phase_one_swaps_unrecorded_code_deposits_when_some_were_paid_in_pgas() {
		use frame_support::traits::tokens::fungibles::InspectHold;

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let legacy_owner = AccountId32::new([1; 32]);
			let native_owner = AccountId32::new([2; 32]);
			let pgas_owner = AccountId32::new([3; 32]);
			for owner in [&legacy_owner, &native_owner, &pgas_owner] {
				<Test as Config>::Currency::set_balance(owner, 1_000_000);
			}
			seed_code_uploads_paid_in_both(&legacy_owner, &native_owner, &pgas_owner);

			V4::run_to_completion();

			let reason = HoldReason::CodeUploadDepositReserve;
			assert_eq!(NativeDepositOf::<Test>::iter_prefix(&pallet_account).count(), 1);
			assert_eq!(NativeDepositOf::<Test>::get(&pallet_account, &native_owner), 500);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(&reason.into(), &pallet_account),
				500,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(PGasAssetId::get(), &reason.into(), &pallet_account),
				1_400,
			);

			// Every owner can remove their code; the native owner is refunded in native.
			let free_before = <Test as Config>::Currency::balance(&native_owner);
			for (owner, hash) in [
				(&legacy_owner, H256::repeat_byte(0xAA)),
				(&pgas_owner, H256::repeat_byte(0xBB)),
				(&native_owner, H256::repeat_byte(0xAB)),
			] {
				assert_ok!(Pallet::<Test>::remove_code(RuntimeOrigin::signed(owner.clone()), hash));
			}
			assert_eq!(<Test as Config>::Currency::balance(&native_owner), free_before + 500);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(&reason.into(), &pallet_account),
				0,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(PGasAssetId::get(), &reason.into(), &pallet_account),
				0,
			);
		});
	}

	/// Run with the minimum weight per block, phase 1 resumes from its cursor and still credits
	/// each code deposit once.
	#[test]
	fn phase_one_resumes_across_blocks() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner_a = AccountId32::new([1; 32]);
			let owner_b = AccountId32::new([2; 32]);
			seed_code_upload(H256::repeat_byte(0xAA), owner_a.clone(), 1_000);
			seed_code_upload(H256::repeat_byte(0xAB), owner_a.clone(), 500);
			seed_code_upload(H256::repeat_byte(0xBB), owner_b.clone(), 2_000);

			let required = match V4::step(None, &mut WeightMeter::with_limit(Weight::zero())) {
				Err(SteppedMigrationError::InsufficientWeight { required }) => required,
				other => panic!("expected InsufficientWeight, got {other:?}"),
			};
			let mut cursor = None;
			let mut blocks = 0;
			loop {
				blocks += 1;
				let mut meter = WeightMeter::with_limit(required);
				match V4::step(cursor, &mut meter).unwrap() {
					Some(next) => cursor = Some(next),
					None => break,
				}
			}

			assert!(blocks > 1, "the migration should span several blocks");
			assert_eq!(NativeDepositOf::<Test>::get(&pallet_account, &owner_a), 1_500);
			assert_eq!(NativeDepositOf::<Test>::get(&pallet_account, &owner_b), 2_000);
		});
	}

	/// If the PGAS mint fails (here: the PGAS asset is not registered), the native hold must stay
	/// in place. Phase 2 only logs the error, so a partial swap would be committed.
	#[test]
	fn migrate_native_to_pgas_keeps_native_hold_when_pgas_mint_fails() {
		use crate::{PGasDeposit, tests::PGasRefundPercent};
		use frame_support::traits::ConstU32;

		type UnregisteredPGas = PGasDeposit<
			Test,
			Assets,
			AssetsHolder,
			AssetsFreezer,
			ConstU32<999>,
			PGasRefundPercent,
		>;

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let address = H160::repeat_byte(0x10);
			seed_contract(address, H256::repeat_byte(0xCC), 700);
			let contract = <Test as Config>::AddressMapper::to_account_id(&address);
			let total_issuance = <Test as Config>::Currency::total_issuance();

			assert!(
				<UnregisteredPGas as Deposit<Test>>::migrate_native_to_pgas(
					HoldReason::StorageDepositReserve,
					&contract,
					700,
				)
				.is_err()
			);

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				700,
			);
			assert_eq!(<Test as Config>::Currency::total_issuance(), total_issuance);
		});
	}

	/// On a registered PGAS asset the ED is minted and frozen before the native burn. If the burn
	/// then fails, the ED mint and freeze must roll back with it.
	#[test]
	fn migrate_native_to_pgas_rolls_back_ed_when_native_burn_fails() {
		use frame_support::traits::tokens::fungibles::{Inspect, InspectFreeze};

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let address = H160::repeat_byte(0x10);
			seed_contract(address, H256::repeat_byte(0xCC), 500);
			let contract = <Test as Config>::AddressMapper::to_account_id(&address);
			let total_issuance = <Test as Config>::Currency::total_issuance();
			let pgas_issuance = Assets::total_issuance(PGasAssetId::get());
			assert!(Assets::minimum_balance(PGasAssetId::get()) > 0);

			assert!(
				<<Test as Config>::Deposit as Deposit<Test>>::migrate_native_to_pgas(
					HoldReason::StorageDepositReserve,
					&contract,
					700,
				)
				.is_err()
			);

			assert_eq!(Assets::balance(PGasAssetId::get(), &contract), 0);
			assert_eq!(
				AssetsFreezer::balance_frozen(
					PGasAssetId::get(),
					&FreezeReason::PGasMinBalance.into(),
					&contract,
				),
				0,
			);
			assert_eq!(Assets::total_issuance(PGasAssetId::get()), pgas_issuance);
			assert_eq!(<Test as Config>::Currency::total_issuance(), total_issuance);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				500,
			);
		});
	}

	/// Running v4 before the PGAS asset is registered does not fail on chain, which could freeze
	/// it: every swap rolls back and is logged, v4 completes, and every contract keeps its native
	/// storage deposit with no currency burned. `pre_upgrade` rejects this state instead.
	#[test]
	fn phase_two_without_pgas_asset_completes_and_keeps_native_deposits() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			pallet_assets::Asset::<Test>::remove(PGasAssetId::get());

			let owner = AccountId32::new([1; 32]);
			let hash = H256::repeat_byte(0xCC);
			seed_code_upload(hash, owner, 0);
			let c1 = H160::repeat_byte(0x10);
			let c2 = H160::repeat_byte(0x20);
			seed_contract(c1, hash, 700);
			seed_contract(c2, hash, 1_300);
			let total_issuance = <Test as Config>::Currency::total_issuance();

			let mut cursor = None;
			let result = loop {
				match V4::step(cursor.clone(), &mut WeightMeter::new()) {
					Ok(Some(next)) => cursor = Some(next),
					other => break other,
				}
			};
			assert_eq!(result, Ok(None));

			for (address, deposit) in [(c1, 700), (c2, 1_300)] {
				let contract = <Test as Config>::AddressMapper::to_account_id(&address);
				assert_eq!(
					<Test as Config>::Currency::balance_on_hold(
						&HoldReason::StorageDepositReserve.into(),
						&contract,
					),
					deposit,
				);
			}
			assert_eq!(<Test as Config>::Currency::total_issuance(), total_issuance);
		});
	}

	/// An account that holds a deposit: the pallet account for code, a contract or a delegated
	/// account.
	#[derive(Clone, Copy, Debug)]
	enum Holder {
		Code,
		Contract,
		Delegated,
	}

	/// How a holder's deposit was paid before v4 runs. Every history has native paid before the
	/// switch (`LEGACY`, no row); some add native paid after it (`NATIVE`, a row for `payer`) or
	/// PGAS paid after it (`PGAS`).
	#[derive(Clone, Copy, Debug)]
	struct History {
		native_after: bool,
		pgas_after: bool,
	}

	const LEGACY: u128 = 1_000;
	const NATIVE: u128 = 500;
	const PGAS: u128 = 400;

	/// Seeds `holder` with `history` and returns its account and hold reason.
	fn seed_holder(
		holder: Holder,
		history: History,
		legacy: &AccountId32,
		payer: &AccountId32,
		pgas_payer: &AccountId32,
	) -> (AccountId32, HoldReason) {
		use crate::storage::ContractInfo;
		use frame_support::traits::tokens::fungibles::{
			Inspect as _, Mutate as _, MutateHold as _,
		};

		let native = LEGACY + if history.native_after { NATIVE } else { 0 };
		let (account, reason) = match holder {
			Holder::Code => {
				seed_code_upload(H256::repeat_byte(0xAA), legacy.clone(), LEGACY);
				if history.native_after {
					seed_code_upload(H256::repeat_byte(0xAB), payer.clone(), NATIVE);
				}
				(Pallet::<Test>::account_id(), HoldReason::CodeUploadDepositReserve)
			},
			Holder::Contract => {
				let address = H160::repeat_byte(0x10);
				seed_contract(address, H256::repeat_byte(0xCC), native);
				(
					<Test as Config>::AddressMapper::to_account_id(&address),
					HoldReason::StorageDepositReserve,
				)
			},
			Holder::Delegated => {
				let address = H160::repeat_byte(0x30);
				AccountInfoOf::<Test>::insert(
					address,
					AccountInfo::<Test> {
						account_type: AccountType::DelegatedEOA {
							delegate_target: Some(H160::repeat_byte(0x31)),
							contract_info: ContractInfo::<Test>::new_for_delegation(
								&address,
								H256::zero(),
							),
							payer: Some(legacy.clone()),
						},
						dust: 0,
					},
				);
				let account = <Test as Config>::AddressMapper::to_account_id(&address);
				let ed = <Test as Config>::Currency::minimum_balance();
				<Test as Config>::Currency::mint_into(&account, ed + native).unwrap();
				<Test as Config>::Currency::hold(
					&HoldReason::StorageDepositReserve.into(),
					&account,
					native,
				)
				.unwrap();
				(account, HoldReason::StorageDepositReserve)
			},
		};
		if history.native_after {
			NativeDepositOf::<Test>::insert(&account, payer, NATIVE);
		}
		if history.pgas_after {
			let pgas_ed = Assets::minimum_balance(PGasAssetId::get());
			Assets::mint_into(PGasAssetId::get(), &account, pgas_ed + PGAS).unwrap();
			AssetsHolder::hold(PGasAssetId::get(), &reason.into(), &account, PGAS).unwrap();
			if matches!(holder, Holder::Code) {
				CodeInfoOf::<Test>::insert(
					H256::repeat_byte(0xBB),
					CodeInfo::<Test>::new_with_deposit(pgas_payer.clone(), PGAS),
				);
			}
		}
		(account, reason)
	}

	/// v4 leaves every holder, whatever its history, holding exactly the native its rows owe and
	/// the same total, and every payer can then be refunded: native for a row, `RefundPercent` in
	/// PGAS for anything else. The one exception is code on a chain that never enabled
	/// `PGasDeposit` (no rows, no PGAS), whose rows are rebuilt from `CodeInfoOf` so uploaders
	/// keep a native refund.
	///
	/// Refunds go through `Deposit::refund_on_hold`, which `remove_code`, contract termination and
	/// clearing a delegation all use.
	#[test]
	fn reconcile_by_holder_and_history() {
		use crate::deposit_payment::Funds;
		use frame_support::traits::tokens::fungibles::{Inspect, InspectFreeze, InspectHold};

		let histories = [
			History { native_after: false, pgas_after: false },
			History { native_after: true, pgas_after: false },
			History { native_after: false, pgas_after: true },
			History { native_after: true, pgas_after: true },
		];
		for holder in [Holder::Code, Holder::Contract, Holder::Delegated] {
			for history in histories {
				let case = format!("{holder:?} {history:?}");
				ExtBuilder::default().genesis_config(None).build().execute_with(|| {
					let legacy = AccountId32::new([1; 32]);
					let payer = AccountId32::new([2; 32]);
					let pgas_payer = AccountId32::new([3; 32]);
					for who in [&legacy, &payer, &pgas_payer] {
						<Test as Config>::Currency::set_balance(who, 1_000_000);
					}
					let (account, reason) =
						seed_holder(holder, history, &legacy, &payer, &pgas_payer);
					let native_hold =
						|| <Test as Config>::Currency::balance_on_hold(&reason.into(), &account);
					let pgas_hold = || {
						AssetsHolder::balance_on_hold(PGasAssetId::get(), &reason.into(), &account)
					};
					let total = native_hold() + pgas_hold();
					let issuance = <Test as Config>::Currency::total_issuance();

					V4::run_to_completion();

					// Only code on a chain that never enabled `PGasDeposit` is rebuilt.
					let rebuilt = matches!(holder, Holder::Code) &&
						!history.native_after &&
						!history.pgas_after;
					let native = if history.native_after { NATIVE } else { 0 };
					let swapped = if rebuilt { 0 } else { LEGACY };
					assert_eq!(native_hold() + pgas_hold(), total, "{case}: total changed");
					assert_eq!(native_hold(), native + LEGACY - swapped, "{case}: native hold");
					assert_eq!(V4::recorded_native(&account), native_hold(), "{case}: rows");
					assert_eq!(
						issuance - <Test as Config>::Currency::total_issuance(),
						swapped,
						"{case}: native burned",
					);
					// A reconciled holder gets the frozen PGAS ED its PGAS hold needs.
					let frozen = AssetsFreezer::balance_frozen(
						PGasAssetId::get(),
						&FreezeReason::PGasMinBalance.into(),
						&account,
					);
					let pgas_ed = Assets::minimum_balance(PGasAssetId::get());
					assert_eq!(frozen, if rebuilt { 0 } else { pgas_ed }, "{case}: frozen ED");

					// Refund every payer.
					let refund = |who: &AccountId32, amount: u128| {
						let native = <Test as Config>::Currency::balance(who);
						let pgas = Assets::balance(PGasAssetId::get(), who);
						assert_ok!(<Test as Config>::Deposit::refund_on_hold(
							reason,
							&account,
							Funds::Balance(who),
							amount,
						));
						(
							<Test as Config>::Currency::balance(who) - native,
							Assets::balance(PGasAssetId::get(), who) - pgas,
						)
					};
					if history.native_after {
						assert_eq!(refund(&payer, NATIVE), (NATIVE, 0), "{case}: row payer");
					}
					let legacy_refund = if rebuilt { (LEGACY, 0) } else { (0, LEGACY / 10) };
					assert_eq!(refund(&legacy, LEGACY), legacy_refund, "{case}: legacy payer");
					if history.pgas_after {
						assert_eq!(refund(&pgas_payer, PGAS), (0, PGAS / 10), "{case}: PGAS payer");
					}
					assert_eq!(native_hold() + pgas_hold(), 0, "{case}: hold left over");
				});
			}
		}
	}

	/// Runs v4 with `limit` per block until it completes and returns the number of blocks and
	/// whether any block ended with a cursor for which `in_rows` holds. No block may use more
	/// than `limit`, which a step that charges its row reads after the fact would break.
	fn run_with_limit(limit: Weight, in_rows: impl Fn(&CursorOf<Test>) -> bool) -> (u32, bool) {
		let mut cursor = None;
		let (mut blocks, mut seen_rows) = (0, false);
		loop {
			blocks += 1;
			let mut meter = WeightMeter::with_limit(limit);
			let next = V4::step(cursor, &mut meter).unwrap();
			assert!(meter.consumed().all_lte(limit), "a block used more than its limit");
			match next {
				Some(next) => {
					seen_rows |= in_rows(&next);
					cursor = Some(next);
				},
				None => return (blocks, seen_rows),
			}
		}
	}

	/// A contract with many payers has a row per payer. They are summed across blocks, so no
	/// block reads more rows than its weight pays for, and the swap still leaves exactly the
	/// recorded native.
	#[test]
	fn phase_two_sums_many_payers_across_blocks() {
		use crate::deposit_payment::Funds;
		use frame_support::traits::tokens::fungibles::InspectHold;

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let owner = AccountId32::new([1; 32]);
			let hash = H256::repeat_byte(0xCC);
			seed_code_upload(hash, owner, 0);

			let address = H160::repeat_byte(0x10);
			seed_contract(address, hash, 1_000);
			let contract = <Test as Config>::AddressMapper::to_account_id(&address);
			let payers = 40u8;
			for i in 0..payers {
				NativeDepositOf::<Test>::insert(&contract, AccountId32::new([i + 10; 32]), 5);
			}

			let required = match V4::step(None, &mut WeightMeter::with_limit(Weight::zero())) {
				Err(SteppedMigrationError::InsufficientWeight { required }) => required,
				other => panic!("expected InsufficientWeight, got {other:?}"),
			};
			let (blocks, seen_rows) =
				run_with_limit(required, |c| matches!(c, Cursor::ContractRows(..)));

			assert!(seen_rows, "the contract's rows should span several steps");
			assert!(blocks > 1, "the migration should span several blocks");
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				u128::from(payers) * 5,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				1_000 - u128::from(payers) * 5,
			);
			// Every recorded payer can still be refunded in native.
			let payer = AccountId32::new([10; 32]);
			let free_before = <Test as Config>::Currency::balance(&payer);
			assert_ok!(<Test as Config>::Deposit::refund_on_hold(
				HoldReason::StorageDepositReserve,
				&contract,
				Funds::Balance(&payer),
				5,
			));
			assert_eq!(<Test as Config>::Currency::balance(&payer), free_before + 5);
		});
	}

	/// The same for the pallet account, which has a row per code uploader.
	#[test]
	fn phase_one_sums_many_uploaders_across_blocks() {
		use frame_support::traits::tokens::fungibles::InspectHold;

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owners = [AccountId32::new([1; 32]), AccountId32::new([2; 32])];
			seed_code_uploads_paid_in_both(&owners[0], &owners[1], &AccountId32::new([3; 32]));
			let uploaders = 30u8;
			for i in 0..uploaders {
				NativeDepositOf::<Test>::insert(&pallet_account, AccountId32::new([i + 10; 32]), 2);
			}
			let owed = 500 + u128::from(uploaders) * 2;

			let required = match V4::step(None, &mut WeightMeter::with_limit(Weight::zero())) {
				Err(SteppedMigrationError::InsufficientWeight { required }) => required,
				other => panic!("expected InsufficientWeight, got {other:?}"),
			};
			let (blocks, seen_rows) =
				run_with_limit(required, |c| matches!(c, Cursor::CodeRows(_)));

			assert!(seen_rows, "the pallet account's rows should span several steps");
			assert!(blocks > 1, "the migration should span several blocks");
			let reason = HoldReason::CodeUploadDepositReserve;
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(&reason.into(), &pallet_account),
				owed,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(PGasAssetId::get(), &reason.into(), &pallet_account),
				1_900 - owed,
			);
		});
	}

	/// With room for more than the minimum, a step reads several rows and the migration needs
	/// fewer blocks, still within the limit.
	#[test]
	fn phase_two_reads_more_rows_per_block_when_the_meter_allows() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let hash = H256::repeat_byte(0xCC);
			seed_code_upload(hash, AccountId32::new([1; 32]), 0);
			let address = H160::repeat_byte(0x10);
			seed_contract(address, hash, 1_000);
			let contract = <Test as Config>::AddressMapper::to_account_id(&address);
			for i in 0..40u8 {
				NativeDepositOf::<Test>::insert(&contract, AccountId32::new([i + 10; 32]), 5);
			}

			let required = match V4::step(None, &mut WeightMeter::with_limit(Weight::zero())) {
				Err(SteppedMigrationError::InsufficientWeight { required }) => required,
				other => panic!("expected InsufficientWeight, got {other:?}"),
			};
			let roomy = required.saturating_add(
				<<Test as Config>::WeightInfo as WeightInfo>::v4_row_step().saturating_mul(20),
			);
			let (blocks, _) = run_with_limit(roomy, |_| false);
			assert!(blocks < 40, "{blocks} blocks for 40 rows with room for 20 extra per block");
		});
	}

	#[test]
	fn phase_three_rewrites_legacy_deletion_queue_entries() {
		use crate::{
			DeletionQueueCounter,
			storage::{DeletionQueueItem, DeletionQueueManager},
		};

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let trie_a: TrieId = vec![0xAA; 16].try_into().unwrap();
			let trie_b: TrieId = vec![0xBB; 24].try_into().unwrap();
			old::DeletionQueue::<Test>::insert(0u32, trie_a.clone());
			old::DeletionQueue::<Test>::insert(1u32, trie_b.clone());
			let mut q = DeletionQueueManager::<Test>::from_test_values(2, 0);
			DeletionQueueCounter::<Test>::set(q.clone());
			let _ = &mut q;

			V4::run_to_completion();

			let zero = AccountId32::new([0u8; 32]);
			assert_eq!(
				DeletionQueue::<Test>::get(0u32),
				Some(DeletionQueueItem::<Test>::new(trie_a, zero.clone())),
			);
			assert_eq!(
				DeletionQueue::<Test>::get(1u32),
				Some(DeletionQueueItem::<Test>::new(trie_b, zero)),
			);
		});
	}

	/// A contract terminated after the runtime upgrade but before v4 queues a new-format item
	/// naming its account. Phase 3 must keep that account, or the contract's `NativeDepositOf`
	/// rows are never cleared and its address can never be instantiated again.
	#[test]
	fn phase_three_keeps_new_format_deletion_queue_entries() {
		use crate::{
			DeletionQueueCounter,
			storage::{DeletionQueueItem, DeletionQueueManager},
		};

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let legacy: TrieId = vec![0xAA; 16].try_into().unwrap();
			let fresh: TrieId = vec![0xBB; 24].try_into().unwrap();
			let contract = AccountId32::new([7; 32]);
			let fresh_item = DeletionQueueItem::<Test>::new(fresh, contract.clone());
			old::DeletionQueue::<Test>::insert(0u32, legacy.clone());
			DeletionQueue::<Test>::insert(1u32, fresh_item.clone());
			NativeDepositOf::<Test>::insert(&contract, AccountId32::new([8; 32]), 100);
			DeletionQueueCounter::<Test>::set(DeletionQueueManager::<Test>::from_test_values(2, 0));

			V4::run_to_completion();

			assert_eq!(
				DeletionQueue::<Test>::get(0u32),
				Some(DeletionQueueItem::<Test>::new(legacy, AccountId32::new([0u8; 32]))),
			);
			assert_eq!(DeletionQueue::<Test>::get(1u32), Some(fresh_item));
			assert!(NativeDepositOf::<Test>::iter_prefix(&contract).next().is_some());
		});
	}

	#[test]
	fn eoa_accounts_are_skipped() {
		use crate::test_utils::{ALICE, ALICE_ADDR, BOB, BOB_ADDR};
		use frame_support::traits::tokens::fungibles::InspectHold;

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let _ = <Test as Config>::Currency::mint_into(&ALICE, Pallet::<Test>::min_balance());
			let _ = <Test as Config>::Currency::mint_into(&BOB, Pallet::<Test>::min_balance());
			AccountInfoOf::<Test>::insert(
				ALICE_ADDR,
				AccountInfo::<Test> { account_type: AccountType::EOA, dust: 0 },
			);

			let owner = AccountId32::new([1; 32]);
			let hash = H256::repeat_byte(0xDD);
			seed_code_upload(hash, owner.clone(), 0);
			seed_contract(BOB_ADDR, hash, 400);

			V4::run_to_completion();

			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&BOB,
				),
				400,
			);
		});
	}

	/// `pre_upgrade` and `post_upgrade` pass on a chain that enabled `PGasDeposit` before v4, and
	/// `post_upgrade` rejects a pallet-account row that was not there before and rows on an
	/// account v4 did not visit.
	#[cfg(feature = "try-runtime")]
	#[test]
	fn try_runtime_checks_pass_on_late_switch_and_catch_extra_row() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner = AccountId32::new([1; 32]);
			let payer = AccountId32::new([2; 32]);

			// Code uploaded before and after the switch; the latter already has its row.
			let hash = H256::repeat_byte(0xAA);
			seed_code_upload(hash, owner.clone(), 1_000);
			seed_code_upload(H256::repeat_byte(0xAB), owner.clone(), 500);
			NativeDepositOf::<Test>::insert(&pallet_account, &owner, 500);
			// A contract whose deposit was partly paid in native after the switch.
			let address = H160::repeat_byte(0x10);
			seed_contract(address, hash, 700);
			let contract = <Test as Config>::AddressMapper::to_account_id(&address);
			NativeDepositOf::<Test>::insert(&contract, &payer, 300);
			// A legacy queue slot and one queued in the new format after the switch.
			let legacy: TrieId = vec![0xAA; 16].try_into().unwrap();
			old::DeletionQueue::<Test>::insert(0u32, legacy);
			DeletionQueue::<Test>::insert(
				1u32,
				DeletionQueueItem::<Test>::new(
					vec![0xBB; 24].try_into().unwrap(),
					AccountId32::new([7; 32]),
				),
			);

			let state = V4::pre_upgrade().unwrap();
			V4::run_to_completion();
			assert_ok!(V4::post_upgrade(state.clone()));

			let extra = AccountId32::new([3; 32]);
			NativeDepositOf::<Test>::insert(&pallet_account, &extra, 100);
			assert_eq!(
				V4::post_upgrade(state.clone()),
				Err("v4: pallet-account NativeDepositOf row left without code".into()),
			);
			NativeDepositOf::<Test>::remove(&pallet_account, &extra);

			// Rows on an account that is neither visited nor queued for deletion.
			NativeDepositOf::<Test>::insert(AccountId32::new([9; 32]), &payer, 100);
			assert_eq!(
				V4::post_upgrade(state),
				Err("v4: NativeDepositOf rows on an account v4 did not visit".into()),
			);
		});
	}

	/// `pre_upgrade` and `post_upgrade` pass when the pallet account already holds PGAS code
	/// deposits, and `post_upgrade` rejects native code deposits beyond what the rows owe.
	#[cfg(feature = "try-runtime")]
	#[test]
	fn try_runtime_checks_pass_with_pgas_code_deposits_and_catch_extra_native() {
		use frame_support::traits::tokens::{Fortitude, Precision, fungibles::MutateHold as _};

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			seed_code_uploads_paid_in_both(
				&AccountId32::new([1; 32]),
				&AccountId32::new([2; 32]),
				&AccountId32::new([3; 32]),
			);

			let state = V4::pre_upgrade().unwrap();
			V4::run_to_completion();
			assert_ok!(V4::post_upgrade(state.clone()));

			// Turn 100 of the PGAS hold back into native, as a skipped swap would leave it: the
			// total is unchanged, but the native hold is more than the rows owe.
			let reason = HoldReason::CodeUploadDepositReserve;
			AssetsHolder::burn_held(
				PGasAssetId::get(),
				&reason.into(),
				&pallet_account,
				100,
				Precision::Exact,
				Fortitude::Polite,
			)
			.unwrap();
			<Test as Config>::Currency::mint_into(&pallet_account, 100).unwrap();
			<Test as Config>::Currency::hold(&reason.into(), &pallet_account, 100).unwrap();
			assert_eq!(
				V4::post_upgrade(state),
				Err("v4: pallet account native code deposits do not match its rows".into()),
			);
		});
	}
}
