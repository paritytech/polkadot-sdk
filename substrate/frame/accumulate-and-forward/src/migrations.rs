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

//! Runtime upgrade migrations for `pallet-accumulate-and-forward`.

extern crate alloc;

use crate::{Config, Pallet, LOG_TARGET};
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Inspect},
		tokens::{Fortitude, Precision, Preservation},
		OnRuntimeUpgrade, OnUnbalanced,
	},
	PalletId,
};
use sp_runtime::traits::AccountIdConversion;

/// Legacy treasury `PalletId` (`py/trsry`).
const LEGACY_TREASURY_PALLET_ID: PalletId = PalletId(*b"py/trsry");

/// Drain the reducible balance of the legacy `py/trsry`-derived account into the accumulation
/// account.
///
/// The PalletId is hardcoded so the migration cannot be misconfigured to drain the wrong
/// account. Use this for chains where `pallet_treasury` has been removed or where a residual
/// balance is orphaned after relay-treasury XCM payouts.
///
/// Idempotent: early-returns with 1 read if the reducible balance is zero.
/// Runs on every runtime upgrade until removed from the related migrations tuple.
pub struct DrainLegacyTreasuryToAccumulationAccount<T>(PhantomData<T>);

impl<T> OnRuntimeUpgrade for DrainLegacyTreasuryToAccumulationAccount<T>
where
	T: Config,
	T::Currency: Balanced<T::AccountId>,
{
	fn on_runtime_upgrade() -> Weight {
		let source: T::AccountId = LEGACY_TREASURY_PALLET_ID.into_account_truncating();
		// No further inflows to the legacy account are expected, but since this migration
		// runs on every runtime upgrade we use `Preserve` as a safeguard.
		// Worst case we just keep a "dead" account with only ED.
		let amount = <T::Currency as Inspect<T::AccountId>>::reducible_balance(
			&source,
			Preservation::Preserve,
			Fortitude::Polite,
		);
		if amount.is_zero() {
			log::info!(
				target: LOG_TARGET,
				"DrainLegacyTreasuryToAccumulationAccount: nothing to withdraw (reducible balance is zero)."
			);
			return T::DbWeight::get().reads(1);
		}

		match <T::Currency as Balanced<T::AccountId>>::withdraw(
			&source,
			amount,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		) {
			Ok(credit) => {
				<Pallet<T> as OnUnbalanced<_>>::on_unbalanced(credit);
				log::info!(
					target: LOG_TARGET,
					"DrainLegacyTreasuryToAccumulationAccount: swept {amount:?} to accumulation account."
				);
			},
			Err(_) => {
				frame_support::defensive!(
					"DrainLegacyTreasuryToAccumulationAccount: failed to withdraw from legacy treasury account"
				);
			},
		}

		// Distinct storage keys touched: source Account (balances + system),
		// accumulation Account (balances + system) = 4 reads and 4 writes.
		T::DbWeight::get().reads_writes(4, 4)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		let source: T::AccountId = LEGACY_TREASURY_PALLET_ID.into_account_truncating();
		let legacy_pre = <T::Currency as Inspect<T::AccountId>>::reducible_balance(
			&source,
			Preservation::Preserve,
			Fortitude::Polite,
		);
		let accum_pre = <T::Currency as Inspect<T::AccountId>>::reducible_balance(
			&Pallet::<T>::accumulation_account(),
			Preservation::Preserve,
			Fortitude::Polite,
		);
		log::info!(
			target: LOG_TARGET,
			"DrainLegacyTreasuryToAccumulationAccount: pre-upgrade legacy reducible = {legacy_pre:?}, \
			 accumulation reducible = {accum_pre:?}"
		);
		Ok((legacy_pre, accum_pre).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		type Balance<T> =
			<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
		let (legacy_pre, accum_pre): (Balance<T>, Balance<T>) =
			Decode::decode(&mut &state[..]).expect("pre_upgrade encoded (legacy_pre, accum_pre)");

		let source: T::AccountId = LEGACY_TREASURY_PALLET_ID.into_account_truncating();
		let legacy_post = <T::Currency as Inspect<T::AccountId>>::reducible_balance(
			&source,
			Preservation::Preserve,
			Fortitude::Polite,
		);
		frame_support::ensure!(
			legacy_post.is_zero(),
			"Legacy treasury reducible balance should be zero after migration"
		);

		let accumulation_account = Pallet::<T>::accumulation_account();
		let accum_post = <T::Currency as Inspect<T::AccountId>>::reducible_balance(
			&accumulation_account,
			Preservation::Preserve,
			Fortitude::Polite,
		);
		frame_support::ensure!(
			Some(accum_post) == accum_pre.checked_add(&legacy_pre),
			"Accumulation account balance should have increased by exactly the drained amount"
		);

		log::info!(
			target: LOG_TARGET,
			"DrainLegacyTreasuryToAccumulationAccount: post-upgrade OK. \
			 Legacy treasury reducible: {legacy_post:?}, accumulation reducible: {accum_post:?}"
		);
		Ok(())
	}
}
