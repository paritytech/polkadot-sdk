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

//! Migration to convert public-proposal deposits from `Currency` reserves to `fungible` holds.

use crate::*;
use frame_support::traits::{fungible::MutateHold, ReservableCurrency, UncheckedOnRuntimeUpgrade};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// The log target.
const TARGET: &str = "runtime::democracy::migration::v2";

/// Converts every reserved public-proposal deposit (proposer + seconders, tracked in
/// [`DepositOf`]) into an equivalent hold under [`HoldReason::Proposal`].
///
/// The set of deposits is bounded by `MaxProposals * MaxDeposits`, so this is a single-block
/// migration.
pub struct VersionUncheckedMigrateToV2<T>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV2<T> {
	fn on_runtime_upgrade() -> Weight {
		let reason: T::RuntimeHoldReason = HoldReason::Proposal.into();
		let mut reads: u64 = 1;
		let mut writes: u64 = 0;

		for (_prop_index, (depositors, amount)) in DepositOf::<T>::iter() {
			reads = reads.saturating_add(1);
			for who in depositors.iter() {
				let leftover = T::OldCurrency::unreserve(who, amount);
				if !leftover.is_zero() {
					log::warn!(
						target: TARGET,
						"could not fully unreserve a legacy proposal deposit; some balance remained reserved",
					);
				}
				if let Err(e) = T::Currency::hold(&reason, who, amount) {
					log::error!(
						target: TARGET,
						"failed to hold a migrated proposal deposit: {:?}",
						e,
					);
				}
				reads = reads.saturating_add(2);
				writes = writes.saturating_add(2);
			}
		}

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// Record the total expected hold per account across all of their proposal deposits.
		let mut expected: Vec<(T::AccountId, BalanceOf<T>)> = Vec::new();
		for (_prop_index, (depositors, amount)) in DepositOf::<T>::iter() {
			for who in depositors.into_iter() {
				match expected.iter_mut().find(|(a, _)| a == &who) {
					Some((_, total)) => *total = total.saturating_add(amount),
					None => expected.push((who, amount)),
				}
			}
		}
		Ok(expected.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::traits::fungible::InspectHold;

		let expected = Vec::<(T::AccountId, BalanceOf<T>)>::decode(&mut &state[..])
			.expect("pre_upgrade provides a valid state; qed");
		let reason: T::RuntimeHoldReason = HoldReason::Proposal.into();
		for (who, total) in expected {
			ensure!(
				T::Currency::balance_on_hold(&reason, &who) >= total,
				"democracy::v2: a proposal deposit was not migrated to a hold"
			);
		}
		Ok(())
	}
}

/// [`VersionUncheckedMigrateToV2`] wrapped in a [`frame_support::migrations::VersionedMigration`],
/// ensuring it only runs when the on-chain storage version is 1.
pub type MigrateToV2<T> = frame_support::migrations::VersionedMigration<
	1,
	2,
	VersionUncheckedMigrateToV2<T>,
	crate::pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
