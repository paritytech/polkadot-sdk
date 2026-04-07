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

//! # Dynamic Allocation Pool (DAP) Pallet
//!
//! Generic issuance drip and distribution engine.
//!
//! ## Key Responsibilities:
//!
//! - **Issuance Drip**: Mints new tokens on a configurable cadence (per-block or every N minutes)
//!   based on an [`IssuanceCurve`].
//! - **Budget Distribution**: Distributes minted issuance across registered
//!   [`sp_staking::budget::BudgetRecipient`]s according to a governance-updatable
//!   `BoundedBTreeMap<BudgetKey, Perbill>` that must sum to exactly 100%.
//! - **Burn Collection**: Implements `OnUnbalanced` to intercept any burn source wired to it
//!   (staking slashes, transaction fees, dust removal, EVM gas rounding, etc.) and redirect funds
//!   into the buffer account. Incoming funds are deactivated to exclude them from governance
//!   voting.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use alloc::vec::Vec;
use codec::DecodeWithMemTracking;
use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate, Unbalanced},
		Imbalance, OnUnbalanced, Time,
	},
	PalletId,
};
use sp_runtime::{traits::Zero, BoundedBTreeMap, Perbill, SaturatedConversion, Saturating};
use sp_staking::budget::{BudgetKey, BudgetRecipientList, IssuanceCurve};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap";

/// Maximum number of budget recipients.
pub const MAX_BUDGET_RECIPIENTS: u32 = 16;

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Type alias for the budget allocation map.
pub type BudgetAllocationMap = BoundedBTreeMap<BudgetKey, Perbill, ConstU32<MAX_BUDGET_RECIPIENTS>>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;
	use frame_support::{sp_runtime::traits::AccountIdConversion, traits::StorageVersion};
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The currency type (new fungible traits).
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ Unbalanced<Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// The pallet ID used to derive the buffer account.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Issuance curve: computes how much to mint given total issuance and elapsed time.
		type IssuanceCurve: IssuanceCurve<BalanceOf<Self>>;

		/// Registered budget recipients. Each element provides a unique key and pot account.
		///
		/// Wired in the runtime as a tuple, e.g.:
		/// ```ignore
		/// type BudgetRecipients = (Dap, StakerRewardRecipient, ValidatorIncentiveRecipient);
		/// ```
		type BudgetRecipients: BudgetRecipientList<Self::AccountId>;

		/// Time provider (typically `pallet_timestamp`).
		///
		/// `Moment` must represent milliseconds.
		type Time: Time;

		/// Minimum elapsed time (ms) between issuance drips.
		///
		/// - `0` = drip every block
		/// - `60_000` = drip every minute (Recommended)
		///
		/// Should be small relative to era length.
		#[pallet::constant]
		type IssuanceCadence: Get<u64>;

		/// Safety ceiling: maximum elapsed time (ms) considered in a single drip.
		///
		/// If more time has passed than this, elapsed is clamped to this value.
		/// Prevents accidental over-minting from bugs, misconfiguration, or long
		/// periods without blocks.
		#[pallet::constant]
		type MaxElapsedPerDrip: Get<u64>;

		/// Origin that can update budget allocation percentages.
		type BudgetOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: crate::weights::WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Inflation dripped and distributed to budget recipients.
		IssuanceMinted {
			/// Total amount minted in this drip.
			total_minted: BalanceOf<T>,
			/// Elapsed time (ms) since last drip.
			elapsed_millis: u64,
		},
		/// Budget allocation was updated via governance.
		BudgetAllocationUpdated {
			/// The new budget allocation map.
			allocations: BudgetAllocationMap,
		},
		/// An unexpected/defensive event was triggered.
		Unexpected(UnexpectedKind),
	}

	/// Defensive/unexpected errors/events.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, DebugNoBound)]
	pub enum UnexpectedKind {
		/// Failed to mint issuance.
		MintFailed,
		/// Elapsed time was clamped at the safety ceiling.
		ElapsedClamped {
			/// The actual elapsed time in milliseconds.
			actual_elapsed: u64,
			/// The ceiling that was applied.
			ceiling: u64,
		},
	}

	/// Budget allocation map: `BudgetKey -> Perbill`.
	///
	/// Keys must correspond to registered `BudgetRecipients`. Sum of values must be
	/// exactly `Perbill::one()` (100%). Recipients not included receive nothing.
	#[pallet::storage]
	pub type BudgetAllocation<T> = StorageValue<_, BudgetAllocationMap, ValueQuery>;

	/// Timestamp (ms) of the last issuance drip.
	///
	/// On existing chains, this must be seeded via
	/// [`migrations::MigrateV1ToV2`] to prevent incorrect minting on the first drip.
	#[pallet::storage]
	pub type LastIssuanceTimestamp<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// A key in the budget allocation does not match any registered recipient.
		UnknownBudgetKey,
		/// Budget allocation percentages do not sum to exactly 100%.
		BudgetNotExact,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::drip_issuance()
		}

		fn integrity_test() {
			assert!(
				T::MaxElapsedPerDrip::get() > T::IssuanceCadence::get(),
				"MaxElapsedPerDrip must be greater than IssuanceCadence, \
				 otherwise every drip would be clamped below the cadence threshold."
			);

			// Ensure BudgetRecipients have no duplicate keys.
			let mut keys: Vec<_> =
				T::BudgetRecipients::recipients().into_iter().map(|(k, _)| k).collect();
			keys.sort();
			assert!(
				keys.windows(2).all(|w| w[0] != w[1]),
				"Duplicate BudgetRecipient key detected"
			);
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			// TODO(ank4n): Re-enable after this migration is included in runtime.
			// Self::do_try_state()
			Ok(())
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the budget allocation map.
		///
		/// Each key must match a registered `BudgetRecipient`. The sum of all percentages
		/// must be exactly 100%. Recipients not included in the map receive nothing.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_budget_allocation())]
		pub fn set_budget_allocation(
			origin: OriginFor<T>,
			new_allocations: BudgetAllocationMap,
		) -> DispatchResult {
			T::BudgetOrigin::ensure_origin(origin)?;

			// Validate all keys are registered recipients.
			let registered: Vec<_> =
				T::BudgetRecipients::recipients().into_iter().map(|(k, _)| k).collect();
			for key in new_allocations.keys() {
				ensure!(registered.contains(key), Error::<T>::UnknownBudgetKey);
			}

			// Validate sum == 100%. Use u64 to avoid overflow when summing deconstructed Perbills.
			let total_parts: u64 = new_allocations.values().map(|p| p.deconstruct() as u64).sum();
			ensure!(total_parts == Perbill::one().deconstruct() as u64, Error::<T>::BudgetNotExact);

			BudgetAllocation::<T>::put(new_allocations.clone());
			Self::deposit_event(Event::BudgetAllocationUpdated { allocations: new_allocations });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The DAP buffer account.
		///
		/// Collects any burn source wired to it (staking slashes, unclaimed rewards, etc.)
		/// and its explicit budget allocation share.
		pub(crate) fn buffer_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Deactivate funds on buffer inflow.
		pub(crate) fn deactivate_buffer_funds(amount: BalanceOf<T>) {
			<T::Currency as Unbalanced<T::AccountId>>::deactivate(amount);
		}

		/// Core issuance drip logic, called from `on_initialize`.
		pub(crate) fn drip_issuance() -> Weight {
			let now_moment = T::Time::now();
			let now: u64 = now_moment.saturated_into();
			let last = LastIssuanceTimestamp::<T>::get();
			let mut elapsed = now.saturating_sub(last);

			let cadence = T::IssuanceCadence::get();
			if cadence > 0 && elapsed < cadence {
				return T::DbWeight::get().reads(2);
			}

			// First block after genesis: initialize timestamp, don't drip.
			// For existing chains, use `migrations::MigrateV1ToV2` to seed this
			// value from ActiveEra.start so this branch is never hit post-upgrade.
			if last == 0 {
				LastIssuanceTimestamp::<T>::put(now);
				return T::DbWeight::get().reads_writes(2, 2);
			}

			// Apply safety ceiling on elapsed time.
			let max_elapsed = T::MaxElapsedPerDrip::get();
			if elapsed > max_elapsed {
				Self::deposit_event(Event::Unexpected(UnexpectedKind::ElapsedClamped {
					actual_elapsed: elapsed,
					ceiling: max_elapsed,
				}));
				elapsed = max_elapsed;
			}

			let total_issuance = T::Currency::total_issuance();
			let issuance = T::IssuanceCurve::issue(total_issuance, elapsed);
			// Always advance the clock so elapsed time doesn't accumulate across skipped drips.
			LastIssuanceTimestamp::<T>::put(now);

			if issuance.is_zero() {
				return T::DbWeight::get().reads_writes(3, 3);
			}

			// Distribute according to budget map.
			let budget = BudgetAllocation::<T>::get();
			if budget.is_empty() {
				// TODO: Add defensive! panic once budget is always configured.
				log::warn!(
					target: LOG_TARGET,
					"BudgetAllocation is empty — no issuance will be distributed"
				);
				return T::DbWeight::get().reads_writes(4, 4);
			}
			let recipients = T::BudgetRecipients::recipients();
			let mut total_minted = BalanceOf::<T>::zero();

			let buffer = Self::buffer_account();
			for (key, account) in &recipients {
				let perbill = budget.get(key).copied().unwrap_or(Perbill::zero());
				let amount = perbill.mul_floor(issuance);
				if !amount.is_zero() {
					if let Err(_) = T::Currency::mint_into(account, amount) {
						Self::deposit_event(Event::Unexpected(UnexpectedKind::MintFailed));
						defensive!("Issuance mint should not fail");
					} else {
						total_minted = total_minted.saturating_add(amount);
						if *account == buffer {
							Self::deactivate_buffer_funds(amount);
						}
					}
				}
			}

			// Rounding dust from Perbill::mul_floor is not minted.

			Self::deposit_event(Event::IssuanceMinted { total_minted, elapsed_millis: elapsed });

			log::debug!(
				target: LOG_TARGET,
				"Issuance drip: total={issuance:?}, elapsed={elapsed}ms"
			);

			T::WeightInfo::drip_issuance()
		}
	}

	#[cfg(any(test, feature = "try-runtime"))]
	impl<T: Config> Pallet<T> {
		#[allow(dead_code)]
		pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			Self::check_budget_allocation()
		}

		/// Checks that `BudgetAllocation` is consistent:
		/// - Every key in `BudgetAllocation` must be a registered recipient.
		/// - Allocation percentages must sum to exactly 100%.
		fn check_budget_allocation() -> Result<(), sp_runtime::TryRuntimeError> {
			let allocation = BudgetAllocation::<T>::get();

			ensure!(!allocation.is_empty(), "BudgetAllocation is empty");

			let registered: Vec<BudgetKey> =
				T::BudgetRecipients::recipients().into_iter().map(|(k, _)| k).collect();

			// Every allocation key must be a registered recipient.
			for key in allocation.keys() {
				ensure!(
					registered.contains(key),
					"BudgetAllocation contains key not in BudgetRecipients"
				);
			}

			// Allocation must sum to exactly 100%.
			let total_parts: u64 = allocation.values().map(|p| p.deconstruct() as u64).sum();
			ensure!(
				total_parts == Perbill::one().deconstruct() as u64,
				"BudgetAllocation does not sum to 100%"
			);

			Ok(())
		}
	}
}

/// Type alias for credit (negative imbalance - funds that were slashed/removed).
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Implementation of OnUnbalanced for the fungible::Balanced trait.
/// Example: use as `type Slash = Dap` in staking-async config.
///
/// Only the new fungible `Credit` type is supported. An `OnUnbalanced<NegativeImbalance>` impl
/// for the old `Currency` trait is not provided because there are no consumers.
impl<T: Config> OnUnbalanced<CreditOf<T>> for Pallet<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let buffer = Self::buffer_account();
		let numeric_amount = amount.peek();

		// Resolve should never fail because:
		// - can_deposit on destination succeeds since buffer exists (created with provider at
		//   genesis/runtime upgrade so no ED issue)
		// - amount is guaranteed non-zero by the trait method signature
		// The only failure would be overflow on destination.
		let _ = T::Currency::resolve(&buffer, amount)
			.inspect_err(|_| {
				defensive!(
					"🚨 Failed to deposit slash to DAP buffer - funds burned, it should never happen!"
				);
			})
			.inspect(|_| {
				// Deactivate on success; if resolve failed, tokens were burned.
				Self::deactivate_buffer_funds(numeric_amount);
				log::debug!(
					target: LOG_TARGET,
					"💸 Deposited slash of {numeric_amount:?} to DAP buffer"
				);
			});
	}
}

/// DAP exposes its buffer as a budget recipient so it can receive an explicit
/// allocation share (in addition to the implicit remainder).
impl<T: Config> sp_staking::budget::BudgetRecipient<T::AccountId> for Pallet<T> {
	fn budget_key() -> BudgetKey {
		BudgetKey::truncate_from(b"buffer".to_vec())
	}

	fn pot_account() -> T::AccountId {
		Self::buffer_account()
	}
}
