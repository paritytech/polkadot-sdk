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
//! Generic inflation drip and distribution engine.
//!
//! ## Key Responsibilities:
//!
//! - **Inflation Drip**: Mints new tokens on a configurable cadence (per-block or every N minutes)
//!   based on an [`sp_staking::InflationCurve`].
//! - **Budget Distribution**: Distributes minted inflation across registered
//!   [`sp_staking::BudgetRecipient`]s according to a governance-updatable
//!   `BoundedBTreeMap<BudgetKey, Perbill>`. Buffer (DAP's own account) absorbs the remainder.
//! - **Slash Collection**: Implements `OnUnbalanced` to collect slashed funds into the buffer
//!   account. Incoming funds are deactivated to exclude them from governance voting.
//!
//! DAP has no dependency on staking. It mints and distributes; what consumers do with their
//! share is their concern.

#![cfg_attr(not(feature = "std"), no_std)]

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
use sp_runtime::{
	traits::{Saturating, Zero},
	BoundedBTreeMap, Perbill, SaturatedConversion,
};
use sp_staking::{BudgetKey, BudgetRecipientList, InflationCurve};

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
	use frame_support::{sp_runtime::traits::AccountIdConversion, traits::StorageVersion};
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The currency type (new fungible traits).
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// The pallet ID used to derive the buffer account.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Inflation curve: computes how much to mint given total issuance and elapsed time.
		type InflationCurve: InflationCurve<BalanceOf<Self>>;

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

		/// Minimum elapsed time (ms) between inflation drips.
		///
		/// - `0` = drip every block
		/// - `60_000` = drip every minute
		///
		/// Should be small relative to era length.
		#[pallet::constant]
		type InflationCadence: Get<u64>;

		/// Safety ceiling: maximum tokens that can be minted in a single drip.
		///
		/// Prevents accidental over-minting from bugs or misconfiguration.
		#[pallet::constant]
		type MaxInflationPerDrip: Get<BalanceOf<Self>>;

		/// Origin that can update budget allocation percentages.
		type BudgetOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Inflation dripped and distributed to budget recipients.
		InflationDripped {
			/// Total amount minted in this drip.
			total_minted: BalanceOf<T>,
			/// Amount sent to the buffer (remainder after all allocations).
			buffer_amount: BalanceOf<T>,
			/// Elapsed time (ms) since last drip.
			elapsed_millis: u64,
		},
		/// Budget allocation was updated via governance.
		BudgetAllocationUpdated {
			/// The new budget allocation map.
			allocations: BudgetAllocationMap,
		},
		/// Inflation was capped at the safety ceiling.
		InflationCapped {
			/// The computed inflation before capping.
			computed: BalanceOf<T>,
			/// The ceiling that was applied.
			ceiling: BalanceOf<T>,
		},
		/// An unexpected/defensive event was triggered.
		Unexpected(UnexpectedKind),
	}

	/// Defensive/unexpected errors/events.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, DebugNoBound)]
	pub enum UnexpectedKind {
		/// Failed to mint inflation.
		MintFailed,
	}

	/// Budget allocation map: `BudgetKey -> Perbill`.
	///
	/// Keys must correspond to registered `BudgetRecipients`. Sum of values must be
	/// <= `Perbill::one()`. The remainder goes to the buffer account.
	#[pallet::storage]
	pub type BudgetAllocation<T> = StorageValue<_, BudgetAllocationMap, ValueQuery>;

	/// Timestamp (ms) of the last inflation drip.
	#[pallet::storage]
	pub type LastInflationTimestamp<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// A key in the budget allocation does not match any registered recipient.
		UnknownBudgetKey,
		/// Budget allocation percentages exceed 100%.
		BudgetExceedsTotal,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::drip_inflation()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the budget allocation map.
		///
		/// Each key must match a registered `BudgetRecipient`. The sum of all percentages
		/// must be <= 100%. The remainder (including rounding dust) goes to the buffer.
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().reads_writes(0, 1))]
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

			// Validate sum <= 100%. Use deconstruct() to avoid saturating_add capping at
			// one().
			let total_parts: u32 = new_allocations
				.values()
				.map(|p| p.deconstruct())
				.fold(0u32, |acc, p| acc.saturating_add(p));
			ensure!(total_parts <= Perbill::one().deconstruct(), Error::<T>::BudgetExceedsTotal);

			BudgetAllocation::<T>::put(new_allocations.clone());
			Self::deposit_event(Event::BudgetAllocationUpdated { allocations: new_allocations });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The DAP buffer account.
		///
		/// Collects: slashed funds, unclaimed rewards, and the remainder of each inflation drip.
		pub fn buffer_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Core inflation drip logic, called from `on_initialize`.
		pub(crate) fn drip_inflation() -> Weight {
			let now_moment = T::Time::now();
			let now: u64 = now_moment.saturated_into();
			let last = LastInflationTimestamp::<T>::get();
			let elapsed = now.saturating_sub(last);

			let cadence = T::InflationCadence::get();
			if cadence > 0 && elapsed < cadence {
				// Not time yet — cheap early return.
				return T::DbWeight::get().reads(2);
			}

			// First block after genesis: initialize timestamp, don't drip.
			if last == 0 {
				LastInflationTimestamp::<T>::put(now);
				return T::DbWeight::get().reads_writes(2, 1);
			}

			let total_issuance = T::Currency::total_issuance();
			let mut inflation = T::InflationCurve::inflation(total_issuance, elapsed);

			// Apply safety ceiling.
			let ceiling = T::MaxInflationPerDrip::get();
			if inflation > ceiling {
				Self::deposit_event(Event::InflationCapped { computed: inflation, ceiling });
				inflation = ceiling;
			}

			if inflation.is_zero() {
				LastInflationTimestamp::<T>::put(now);
				return T::DbWeight::get().reads_writes(3, 1);
			}

			// Distribute according to budget map.
			let budget = BudgetAllocation::<T>::get();
			let recipients = T::BudgetRecipients::recipients();
			let mut distributed = BalanceOf::<T>::zero();

			for (key, account) in &recipients {
				let perbill = budget.get(key).copied().unwrap_or(Perbill::zero());
				let amount = perbill.mul_floor(inflation);
				if !amount.is_zero() {
					if let Err(_) = T::Currency::mint_into(account, amount) {
						defensive!("Inflation mint should not fail");
						Self::deposit_event(Event::Unexpected(UnexpectedKind::MintFailed));
					} else {
						distributed = distributed.saturating_add(amount);
					}
				}
			}

			// Buffer absorbs remainder (rounding dust + unallocated percentage).
			let buffer_amount = inflation.saturating_sub(distributed);
			if !buffer_amount.is_zero() {
				let buffer = Self::buffer_account();
				if let Err(_) = T::Currency::mint_into(&buffer, buffer_amount) {
					defensive!("Buffer mint should not fail");
					Self::deposit_event(Event::Unexpected(UnexpectedKind::MintFailed));
				}
			}

			LastInflationTimestamp::<T>::put(now);

			Self::deposit_event(Event::InflationDripped {
				total_minted: inflation,
				buffer_amount,
				elapsed_millis: elapsed,
			});

			log::debug!(
				target: LOG_TARGET,
				"Inflation drip: total={inflation:?}, distributed={distributed:?}, \
				 buffer={buffer_amount:?}, elapsed={elapsed}ms"
			);

			// Weight: 2 reads (time + last) + 1 read (issuance) + 1 read (budget) +
			// N mints + 1 buffer mint + 1 write (timestamp)
			let recipient_count = recipients.len() as u64;
			T::DbWeight::get().reads_writes(4 + recipient_count, 2 + recipient_count)
		}
	}
}

/// Type alias for credit (negative imbalance - funds that were slashed/removed).
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Implementation of OnUnbalanced for slash collection.
///
/// Slashed funds are deposited into the buffer account and deactivated
/// to exclude them from governance voting.
impl<T: Config> OnUnbalanced<CreditOf<T>> for Pallet<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let buffer = Self::buffer_account();
		let numeric_amount = amount.peek();

		let _ = T::Currency::resolve(&buffer, amount)
			.inspect_err(|_| {
				defensive!("Failed to deposit slash to DAP buffer - funds burned");
			})
			.inspect(|_| {
				<T::Currency as Unbalanced<T::AccountId>>::deactivate(numeric_amount);
				log::debug!(
					target: LOG_TARGET,
					"Deposited slash of {numeric_amount:?} to DAP buffer"
				);
			});
	}
}

/// DAP exposes its buffer as a budget recipient so it can receive an explicit
/// allocation share (in addition to the implicit remainder).
impl<T: Config> sp_staking::BudgetRecipient<T::AccountId> for Pallet<T> {
	fn budget_key() -> BudgetKey {
		BudgetKey::truncate_from(b"buffer".to_vec())
	}

	fn pot_account() -> T::AccountId {
		Self::buffer_account()
	}
}

impl<T: Config> sp_staking::UnclaimedRewardSink<T::AccountId> for Pallet<T> {
	fn unclaimed_reward_sink() -> T::AccountId {
		Self::buffer_account()
	}
}
