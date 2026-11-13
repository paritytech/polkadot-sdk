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
//! Phase 1 iterates [`crate::CodeInfoOf`] and for each uploaded code records the uploader's
//! existing native `CodeUploadDepositReserve` contribution under [`crate::NativeDepositOf`],
//! keyed by the pallet's own account (the holder of that deposit). The native currency itself
//! stays where it is. It first clears any rows already held by the pallet account, so a chain
//! that enabled `PGasDeposit` before running this migration (and so already recorded some
//! native code deposits) ends up with each owner credited exactly once.
//!
//! [`crate::CodeInfoOf`] does not record which asset paid a code deposit, so the rebuild above is
//! only correct while every code deposit was paid in native. When the pallet account already
//! holds PGAS under `CodeUploadDepositReserve` (code uploaded in PGAS after `PGasDeposit` was
//! enabled), phase 1 instead handles the pallet account like phase 2 handles a contract: its
//! existing rows, written for native paid after the switch, stay as they are, and the native hold
//! they do not account for (code uploaded before the switch) is swapped to PGAS. Every code
//! deposit without a row is then refunded from PGAS, and the native hold matches the rows exactly.
//!
//! Phase 2 iterates [`crate::AccountInfoOf`] and for each contract burns the part of the native
//! `StorageDepositReserve` hold not recorded in the contract's [`crate::NativeDepositOf`] rows,
//! and replaces it with the same amount of PGAS minted into the contract and held under the same
//! reason. The recorded part was paid in native after `PGasDeposit` was enabled and stays native,
//! so those payers are still refunded in native.
//!
//! A contract, or the pallet account for code deposits, has one [`crate::NativeDepositOf`] row per
//! payer, and nothing bounds how many. A swap therefore reads the rows in chunks over as many
//! steps as they need: the cursor keeps the payer of the last row read and the native summed so
//! far ([`RowSum`]), and nothing is written until the last row is read. Each step is charged for
//! one row up front and reads further rows only while the weight meter has room for them.
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
use alloc::{collections::btree_map::BTreeMap, vec::Vec};

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
}

impl<AccountId, Balance: Zero> RowSum<AccountId, Balance> {
	/// A sum that has read no rows yet.
	fn new() -> Self {
		Self { last_payer: None, owed: Zero::zero() }
	}
}

/// Cursor: clearing the pallet account's code credits, then code uploads, contracts, and the
/// deletion-queue rewrite.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Debug)]
pub enum Cursor<AccountId, Balance> {
	/// Last code hash processed in phase 1 (`CodeInfoOf` iteration).
	CodeUpload(H256),
	/// Last contract address processed in phase 2 (`AccountInfoOf` iteration).
	///
	/// `None` is the transition sentinel from phase 1 to phase 2.
	Contract(Option<H160>),
	/// Last deletion-queue index processed in phase 3 (`DeletionQueue` rewrite).
	///
	/// `None` is the transition sentinel from phase 2 to phase 3.
	DeletionQueue(Option<u32>),
	/// Clearing the pallet account's existing [`NativeDepositOf`] rows before phase 1. A fresh
	/// run starts here.
	///
	/// Holds the owner of the last row removed. Resuming after it, rather than from the start of
	/// the prefix, keeps each step from walking over the rows already removed in the same block.
	ClearCodeCredits(Option<AccountId>),
	/// Summing the pallet account's [`NativeDepositOf`] rows in phase 1, before swapping its
	/// unrecorded native code deposits.
	CodeRows(RowSum<AccountId, Balance>),
	/// Summing the [`NativeDepositOf`] rows of the contract at this address in phase 2, before
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
		let code_step = <T as Config>::WeightInfo::v4_code_upload_step();
		// A clearing step removes one row or, once none are left, runs the first phase 1 step.
		let clear_step =
			<T as Config>::WeightInfo::v4_clear_code_credit_step().saturating_add(code_step);
		// A swap step reads at least one `NativeDepositOf` row, which the benchmark does not
		// include, and as many more as the meter still has room for.
		let row = Self::native_row_reads(1);
		let swap_step = <T as Config>::WeightInfo::v4_contract_step().saturating_add(row);
		let deletion_queue_step = <T as Config>::WeightInfo::v4_deletion_queue_step();
		// The first step reads the pallet account's two code-deposit holds to pick between
		// rebuilding its rows and swapping its unrecorded native, which costs a swap step.
		let first_step = clear_step.max(swap_step).saturating_add(T::DbWeight::get().reads(2));
		let required = first_step.max(deletion_queue_step);
		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			let step_weight = match &cursor {
				None => first_step,
				Some(Cursor::ClearCodeCredits(_)) => clear_step,
				Some(Cursor::CodeUpload(_)) => code_step,
				Some(Cursor::CodeRows(_)) |
				Some(Cursor::Contract(_)) |
				Some(Cursor::ContractRows(..)) => swap_step,
				Some(Cursor::DeletionQueue(_)) => deletion_queue_step,
			};
			if meter.try_consume(step_weight).is_err() {
				break;
			}
			// The step weight covers the first row; allow as many more as still fit.
			let max_rows = Self::rows_within(meter.remaining(), row).saturating_add(1);
			let (next, rows_read) = Self::step_once_counting_rows(cursor, max_rows);
			// `rows_read <= max_rows`, so the rows beyond the first fit in what was left.
			meter.consume(Self::native_row_reads(rows_read.saturating_sub(1)));
			cursor = next;
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
			swap_code = !Self::pgas_code_deposits().is_zero();
			if swap_code {
				per_owner = NativeDepositOf::<T>::iter_prefix(&pallet_account).collect();
			} else {
				for (_hash, info) in CodeInfoOf::<T>::iter() {
					let entry = per_owner.entry(info.owner().clone()).or_default();
					*entry = entry.saturating_add(info.deposit());
				}
			}
		}

		// Per contract: total deposit on hold and the native amount owed by its rows.
		let mut per_contract: BTreeMap<H160, (BalanceOf<T>, BalanceOf<T>)> = BTreeMap::new();
		for (addr, info) in AccountInfoOf::<T>::iter() {
			if !matches!(info.account_type, AccountType::Contract(_)) {
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

		for (addr, (expected, owed_native)) in per_contract {
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
	/// Run a single iteration of the migration's inner loop, returning the next cursor or
	/// `None` if the migration is complete.
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn step_once(cursor: Option<CursorOf<T>>) -> Option<CursorOf<T>> {
		Self::step_once_counting_rows(cursor, u32::MAX).0
	}

	/// [`Self::step_once`] reading at most `max_rows` [`NativeDepositOf`] rows, which must not be
	/// zero. Also returns how many rows a swap read.
	fn step_once_counting_rows(
		cursor: Option<CursorOf<T>>,
		max_rows: u32,
	) -> (Option<CursorOf<T>>, u32) {
		let mut rows_read = 0;
		let next = Self::step_once_inner(cursor, max_rows, &mut rows_read);
		(next, rows_read)
	}

	/// How many [`NativeDepositOf`] rows, each costing `row`, fit in `weight`.
	fn rows_within(
		weight: frame_support::weights::Weight,
		row: frame_support::weights::Weight,
	) -> u32 {
		weight
			.checked_div_per_component(&row)
			.map_or(u32::MAX, |rows| u32::try_from(rows).unwrap_or(u32::MAX))
	}

	/// Weight of reading `rows` [`NativeDepositOf`] rows, on top of the phase 2 step weight.
	fn native_row_reads(rows: u32) -> frame_support::weights::Weight {
		use frame_support::traits::StorageInfoTrait;

		let row_proof = NativeDepositOf::<T>::storage_info()
			.first()
			.and_then(|info| info.max_size)
			.unwrap_or_default();
		T::DbWeight::get().reads(rows.into()).saturating_add(
			frame_support::weights::Weight::from_parts(
				0,
				u64::from(row_proof).saturating_mul(rows.into()),
			),
		)
	}

	/// Sum of the [`NativeDepositOf`] rows recorded against `holder`. Reads every row, so it is
	/// only for `try-runtime` checks, never for a migration step.
	#[cfg(feature = "try-runtime")]
	fn recorded_native(holder: &T::AccountId) -> BalanceOf<T> {
		NativeDepositOf::<T>::iter_prefix_values(holder)
			.fold(BalanceOf::<T>::zero(), |sum, v| sum.saturating_add(v))
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

	fn step_once_inner(
		cursor: Option<CursorOf<T>>,
		max_rows: u32,
		rows_read: &mut u32,
	) -> Option<CursorOf<T>> {
		// Without PGAS support phases 1 and 2 are no-ops, so skip straight to phase 3.
		// Forced on under `runtime-benchmarks` so per-phase weights are still measured.
		if !T::Deposit::SUPPORTS_PGAS && !cfg!(feature = "runtime-benchmarks") {
			return match cursor {
				None |
				Some(Cursor::ClearCodeCredits(_)) |
				Some(Cursor::CodeUpload(_)) |
				Some(Cursor::CodeRows(_)) |
				Some(Cursor::Contract(_)) |
				Some(Cursor::ContractRows(..)) => Some(Cursor::DeletionQueue(None)),
				Some(Cursor::DeletionQueue(last)) => match Self::step_3_deletion_queue(last) {
					Some(next) => Some(Cursor::DeletionQueue(Some(next))),
					None => None,
				},
			};
		}

		match cursor {
			None if !Self::pgas_code_deposits().is_zero() => {
				Some(Self::step_1_code_rows(RowSum::new(), max_rows, rows_read))
			},
			Some(Cursor::CodeRows(sum)) => Some(Self::step_1_code_rows(sum, max_rows, rows_read)),
			None | Some(Cursor::ClearCodeCredits(None)) => {
				match Self::clear_one_code_credit(None) {
					Some(owner) => Some(Cursor::ClearCodeCredits(Some(owner))),
					None => Self::step_1_code_upload(None),
				}
			},
			Some(Cursor::ClearCodeCredits(last)) => match Self::clear_one_code_credit(last) {
				Some(owner) => Some(Cursor::ClearCodeCredits(Some(owner))),
				None => Self::step_1_code_upload(None),
			},
			Some(Cursor::CodeUpload(last)) => Self::step_1_code_upload(Some(last)),
			Some(Cursor::Contract(last)) => Some(Self::step_2_contract(last, max_rows, rows_read)),
			Some(Cursor::ContractRows(contract, sum)) => {
				Some(Self::step_2_contract_rows(contract, sum, max_rows, rows_read))
			},
			Some(Cursor::DeletionQueue(last)) => match Self::step_3_deletion_queue(last) {
				Some(next) => Some(Cursor::DeletionQueue(Some(next))),
				None => None,
			},
		}
	}

	/// Swap the part of `who`'s native hold under `reason` that its [`NativeDepositOf`] rows do
	/// not account for to PGAS, leaving the rows and the native they are owed in place.
	///
	/// The rows were written by `PGasDeposit` for native paid after it was enabled, and
	/// `refund_on_hold` refunds them from the native hold with `Precision::Exact`; swapping that
	/// native would leave those payers unable to be refunded. A failed swap rolls back and is
	/// logged, leaving `who` on native; `post_upgrade` reports it.
	///
	/// `who` can have a row per payer, so at most `max_rows` rows are read per call. `sum` is the
	/// progress of the previous calls, and `rows_read` is increased by the number of rows read.
	/// Returns the progress to resume from if more rows may remain, or `None` once the swap is
	/// done. Nothing is written until the last row is read, so the hold does not change between
	/// calls. `max_rows` must not be zero, or no call makes progress.
	fn swap_unrecorded_native(
		reason: HoldReason,
		who: &T::AccountId,
		mut sum: RowSum<T::AccountId, BalanceOf<T>>,
		max_rows: u32,
		rows_read: &mut u32,
	) -> Option<RowSum<T::AccountId, BalanceOf<T>>> {
		use frame_support::traits::fungible::InspectHold;

		let rows = match &sum.last_payer {
			Some(last) => NativeDepositOf::<T>::iter_prefix_from(
				who,
				NativeDepositOf::<T>::hashed_key_for(who, last),
			),
			None => NativeDepositOf::<T>::iter_prefix(who),
		};
		let mut read = 0u32;
		for (payer, amount) in rows.take(max_rows as usize) {
			sum.owed = sum.owed.saturating_add(amount);
			sum.last_payer = Some(payer);
			read = read.saturating_add(1);
		}
		*rows_read = rows_read.saturating_add(read);
		if read >= max_rows {
			// The budget ran out, so rows may remain. A call that finds none finishes the swap.
			return Some(sum);
		}

		let held = T::Currency::balance_on_hold(&reason.into(), who);
		let owed_native = sum.owed;
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
		None
	}

	/// Phase 1 when the pallet account already holds PGAS code deposits.
	///
	/// [`CodeInfoOf`] does not record which asset paid, so the rows cannot be rebuilt from it.
	/// The existing rows are correct (written for native code deposits paid after the switch),
	/// and every code deposit without a row was paid either in native before the switch or in
	/// PGAS after it. Swapping the unrecorded native to PGAS refunds all of those from PGAS.
	///
	/// The pallet account has a row per uploader, so this is spread over as many steps as the
	/// rows need, see [`Self::swap_unrecorded_native`]. Returns the cursor to continue from.
	fn step_1_code_rows(
		sum: RowSum<T::AccountId, BalanceOf<T>>,
		max_rows: u32,
		rows_read: &mut u32,
	) -> CursorOf<T> {
		match Self::swap_unrecorded_native(
			HoldReason::CodeUploadDepositReserve,
			&Pallet::<T>::account_id(),
			sum,
			max_rows,
			rows_read,
		) {
			Some(sum) => Cursor::CodeRows(sum),
			None => Cursor::Contract(None),
		}
	}

	/// Before phase 1: remove the pallet account's next [`NativeDepositOf`] row after `last`.
	/// Returns the removed row's owner, or `None` once there are none left.
	///
	/// Iterating from `last` instead of from the start of the prefix matters within a block:
	/// rows removed earlier in the block are still in the trie and only marked deleted in the
	/// overlay, so starting from the prefix would step over all of them again.
	fn clear_one_code_credit(last: Option<T::AccountId>) -> Option<T::AccountId> {
		let pallet_account = Pallet::<T>::account_id();
		let owner = match last {
			Some(last) => NativeDepositOf::<T>::iter_key_prefix_from(
				&pallet_account,
				NativeDepositOf::<T>::hashed_key_for(&pallet_account, &last),
			)
			.next(),
			None => NativeDepositOf::<T>::iter_key_prefix(&pallet_account).next(),
		}?;
		NativeDepositOf::<T>::remove(&pallet_account, &owner);
		Some(owner)
	}

	/// Phase 1: credit the next `CodeInfoOf` entry's owner in [`NativeDepositOf`]. Returns
	/// `Some(Cursor::Contract(None))` when phase 1 is exhausted.
	fn step_1_code_upload(last: Option<H256>) -> Option<CursorOf<T>> {
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

	/// Phase 2: swap the next contract's unrecorded native storage deposit to PGAS with
	/// [`Self::swap_unrecorded_native`]. EOAs are skipped but still advance the cursor. Returns
	/// the cursor to continue from, which is phase 3 once there are no accounts left.
	fn step_2_contract(last: Option<H160>, max_rows: u32, rows_read: &mut u32) -> CursorOf<T> {
		let mut iter = match last {
			Some(last) => AccountInfoOf::<T>::iter_from(AccountInfoOf::<T>::hashed_key_for(last)),
			None => AccountInfoOf::<T>::iter(),
		};

		let Some((addr, info)) = iter.next() else { return Cursor::DeletionQueue(None) };
		if matches!(info.account_type, AccountType::Contract(_)) {
			return Self::step_2_contract_rows(addr, RowSum::new(), max_rows, rows_read);
		}
		Cursor::Contract(Some(addr))
	}

	/// Phase 2 for the contract at `address`, continuing from the rows already summed in `sum`.
	fn step_2_contract_rows(
		address: H160,
		sum: RowSum<T::AccountId, BalanceOf<T>>,
		max_rows: u32,
		rows_read: &mut u32,
	) -> CursorOf<T> {
		let contract = T::AddressMapper::to_account_id(&address);
		match Self::swap_unrecorded_native(
			HoldReason::StorageDepositReserve,
			&contract,
			sum,
			max_rows,
			rows_read,
		) {
			Some(sum) => Cursor::ContractRows(address, sum),
			None => Cursor::Contract(Some(address)),
		}
	}

	/// Phase 3: rewrite the next [`DeletionQueue`] slot from the old `TrieId`-only layout
	/// into the new [`DeletionQueueItem`] format. Pre-v4 entries had no [`NativeDepositOf`]
	/// rows, so the recorded `account_id` is a zero placeholder; phase 1 of the deletion
	/// processor will clear an empty prefix on it. Slots already in the new format are left
	/// unchanged. Returns `None` when phase 3 finishes.
	fn step_3_deletion_queue(last: Option<u32>) -> Option<u32> {
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

	/// A chain that enabled `PGasDeposit` before running v4 already has rows for code uploaded
	/// since the switch. Phase 1 must credit each code deposit once, so the pre-switch deposit
	/// becomes refundable and the post-switch one is not counted twice.
	#[test]
	fn phase_one_run_late_credits_each_code_deposit_once() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner = AccountId32::new([1; 32]);
			<Test as Config>::Currency::set_balance(&owner, 1_000_000);

			// Uploaded before the switch: no credit recorded.
			let legacy = H256::repeat_byte(0xAA);
			seed_code_upload(legacy, owner.clone(), 1_000);
			// Uploaded after the switch: `PGasDeposit` recorded the native contribution.
			seed_code_upload(H256::repeat_byte(0xAB), owner.clone(), 500);
			NativeDepositOf::<Test>::insert(&pallet_account, &owner, 500);

			V4::run_to_completion();

			assert_eq!(NativeDepositOf::<Test>::get(&pallet_account, &owner), 1_500);

			let free_before = <Test as Config>::Currency::balance(&owner);
			assert_ok!(Pallet::<Test>::remove_code(RuntimeOrigin::signed(owner.clone()), legacy));
			assert_eq!(<Test as Config>::Currency::balance(&owner), free_before + 1_000);
			assert_eq!(NativeDepositOf::<Test>::get(&pallet_account, &owner), 500);
		});
	}

	/// A pallet-account row whose owner has no code left in `CodeInfoOf` is removed.
	#[test]
	fn phase_one_removes_rows_without_code() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner = AccountId32::new([1; 32]);
			let stale = AccountId32::new([2; 32]);
			seed_code_upload(H256::repeat_byte(0xAA), owner.clone(), 1_000);
			NativeDepositOf::<Test>::insert(&pallet_account, &stale, 300);

			V4::run_to_completion();

			assert!(!NativeDepositOf::<Test>::contains_key(&pallet_account, &stale));
			assert_eq!(NativeDepositOf::<Test>::get(&pallet_account, &owner), 1_000);
		});
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

	/// Run with the minimum weight per block, the clearing step and phase 1 resume from their
	/// cursors and still credit each code deposit once.
	#[test]
	fn phase_one_resumes_across_blocks() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner_a = AccountId32::new([1; 32]);
			let owner_b = AccountId32::new([2; 32]);
			seed_code_upload(H256::repeat_byte(0xAA), owner_a.clone(), 1_000);
			seed_code_upload(H256::repeat_byte(0xAB), owner_a.clone(), 500);
			seed_code_upload(H256::repeat_byte(0xBB), owner_b.clone(), 2_000);
			NativeDepositOf::<Test>::insert(&pallet_account, &owner_a, 500);
			NativeDepositOf::<Test>::insert(&pallet_account, &owner_b, 2_000);

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

	#[test]
	fn phase_one_populates_native_deposit_for_code_upload() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let pallet_account = Pallet::<Test>::account_id();
			let owner_a = AccountId32::new([1; 32]);
			let owner_b = AccountId32::new([2; 32]);
			seed_code_upload(H256::repeat_byte(0xAA), owner_a.clone(), 1_000);
			seed_code_upload(H256::repeat_byte(0xAB), owner_a.clone(), 500);
			seed_code_upload(H256::repeat_byte(0xBB), owner_b.clone(), 2_000);

			V4::run_to_completion();

			assert_eq!(
				NativeDepositOf::<Test>::get(&pallet_account, &owner_a),
				1_500,
				"owner_a sum of code deposits"
			);
			assert_eq!(
				NativeDepositOf::<Test>::get(&pallet_account, &owner_b),
				2_000,
				"owner_b sum of code deposits"
			);

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::CodeUploadDepositReserve.into(),
					&pallet_account,
				),
				3_500,
			);
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

	#[test]
	fn phase_two_burns_native_and_mints_pgas_on_contracts() {
		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let owner = AccountId32::new([1; 32]);
			let hash = H256::repeat_byte(0xCC);
			seed_code_upload(hash, owner.clone(), 0);

			let c1 = H160::repeat_byte(0x10);
			let c2 = H160::repeat_byte(0x20);
			seed_contract(c1, hash, 700);
			seed_contract(c2, hash, 1_300);

			let c1_acc = <Test as Config>::AddressMapper::to_account_id(&c1);
			let c2_acc = <Test as Config>::AddressMapper::to_account_id(&c2);

			let total_issuance_before = <Test as Config>::Currency::total_issuance();

			V4::run_to_completion();

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&c1_acc,
				),
				0,
			);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&c2_acc,
				),
				0,
			);

			assert_eq!(
				total_issuance_before - <Test as Config>::Currency::total_issuance(),
				700 + 1_300,
			);

			use frame_support::traits::tokens::fungibles::{Inspect, InspectHold};
			let pgas_ed = Assets::minimum_balance(PGasAssetId::get());
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&c1_acc,
				),
				700,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&c2_acc,
				),
				1_300,
			);
			// Each migrated contract also gets the PGAS ED minted into its free balance and
			// frozen under `FreezeReason::PGasMinBalance`, matching the post-`init_contract`
			// invariant.
			assert_eq!(Assets::balance(PGasAssetId::get(), &c1_acc), pgas_ed);
			assert_eq!(Assets::balance(PGasAssetId::get(), &c2_acc), pgas_ed);
			use frame_support::traits::tokens::fungibles::InspectFreeze;
			assert_eq!(
				AssetsFreezer::balance_frozen(
					PGasAssetId::get(),
					&FreezeReason::PGasMinBalance.into(),
					&c1_acc,
				),
				pgas_ed,
			);
			assert_eq!(
				AssetsFreezer::balance_frozen(
					PGasAssetId::get(),
					&FreezeReason::PGasMinBalance.into(),
					&c2_acc,
				),
				pgas_ed,
			);
		});
	}

	/// A chain that enabled `PGasDeposit` before running v4 recorded native storage deposits
	/// paid since the switch in `NativeDepositOf[contract][payer]`. Phase 2 must leave that part
	/// native, so the payer can still free storage, and swap only the unrecorded rest.
	#[test]
	fn phase_two_keeps_recorded_native_deposits() {
		use crate::deposit_payment::Funds;
		use frame_support::traits::tokens::fungibles::InspectHold;

		ExtBuilder::default().genesis_config(None).build().execute_with(|| {
			let owner = AccountId32::new([1; 32]);
			let payer = AccountId32::new([2; 32]);
			let legacy_payer = AccountId32::new([3; 32]);
			<Test as Config>::Currency::set_balance(&payer, 1_000_000);
			<Test as Config>::Currency::set_balance(&legacy_payer, 1_000_000);
			let hash = H256::repeat_byte(0xCC);
			seed_code_upload(hash, owner, 0);

			// 700 paid before the switch (no row), 300 paid in native after it (row).
			let address = H160::repeat_byte(0x10);
			seed_contract(address, hash, 1_000);
			let contract = <Test as Config>::AddressMapper::to_account_id(&address);
			NativeDepositOf::<Test>::insert(&contract, &payer, 300);

			V4::run_to_completion();

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				300,
			);
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				700,
			);
			assert_eq!(NativeDepositOf::<Test>::get(&contract, &payer), 300);

			// A partial refund to the recorded payer is paid in native and lowers the row.
			let free_before = <Test as Config>::Currency::balance(&payer);
			assert_ok!(<Test as Config>::Deposit::refund_on_hold(
				HoldReason::StorageDepositReserve,
				&contract,
				Funds::Balance(&payer),
				100,
			));
			assert_eq!(<Test as Config>::Currency::balance(&payer), free_before + 100);
			assert_eq!(NativeDepositOf::<Test>::get(&contract, &payer), 200);

			// A payer without a row is settled from the swapped PGAS.
			assert_ok!(<Test as Config>::Deposit::refund_on_hold(
				HoldReason::StorageDepositReserve,
				&contract,
				Funds::Balance(&legacy_payer),
				100,
			));
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGasAssetId::get(),
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				600,
			);
		});
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
			let roomy = required.saturating_add(V4::native_row_reads(20));
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
	/// `post_upgrade` rejects a pallet-account row whose owner has no code left.
	#[cfg(feature = "try-runtime")]
	#[test]
	fn try_runtime_checks_pass_on_late_switch_and_catch_stale_row() {
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

			NativeDepositOf::<Test>::insert(&pallet_account, AccountId32::new([3; 32]), 100);
			assert!(V4::post_upgrade(state).is_err());
		});
	}

	/// `pre_upgrade` and `post_upgrade` pass when the pallet account already holds PGAS code
	/// deposits, and `post_upgrade` rejects native code deposits beyond what the rows owe.
	#[cfg(feature = "try-runtime")]
	#[test]
	fn try_runtime_checks_pass_with_pgas_code_deposits_and_catch_extra_native() {
		use frame_support::traits::fungible::{Mutate as _, MutateHold as _};

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

			// Native held for code beyond what the rows owe.
			<Test as Config>::Currency::mint_into(&pallet_account, 100).unwrap();
			<Test as Config>::Currency::hold(
				&HoldReason::CodeUploadDepositReserve.into(),
				&pallet_account,
				100,
			)
			.unwrap();
			assert!(V4::post_upgrade(state).is_err());
		});
	}
}
