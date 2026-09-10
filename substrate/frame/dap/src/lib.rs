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
//! - **Buffer Draws**: Pays registered consumers out of the buffer through [`BufferDraw`], capped
//!   by an absolute per-drip-period budget. DAP deactivates every inflow, so it also owns the
//!   matching reactivation on outflow.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

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
		tokens::{Fortitude, Preservation},
		Currency, Defensive, Imbalance, OnUnbalanced, Time,
	},
	weights::WeightMeter,
	PalletId,
};
use sp_runtime::{traits::Zero, BoundedBTreeMap, Perbill, SaturatedConversion, Saturating};
use sp_staking::budget::{BudgetKey, BudgetRecipientList, IssuanceCurve, PaymentSource};

pub use pallet::*;

pub use sp_dap::DAP_PALLET_ID;

const LOG_TARGET: &str = "runtime::dap";

/// Maximum number of budget recipients.
pub const MAX_BUDGET_RECIPIENTS: u32 = 16;

/// Maximum number of registered draws on the buffer.
pub const MAX_BUFFER_DRAWS: u32 = 16;

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Type alias for the budget allocation map.
pub type BudgetAllocationMap = BoundedBTreeMap<BudgetKey, Perbill, ConstU32<MAX_BUDGET_RECIPIENTS>>;

/// Type alias for the registry of draws on the buffer.
pub type BufferDrawMap<T> =
	BoundedBTreeMap<BudgetKey, DrawBudget<BalanceOf<T>>, ConstU32<MAX_BUFFER_DRAWS>>;

/// The spending budget of one registered draw on the DAP buffer.
///
/// Unlike [`BudgetAllocation`]'s `Perbill` share of a drip, a draw is capped by an absolute
/// `limit` that refreshes every drip period: the buffer also holds burns and slashes, so a share
/// of issuance is not a meaningful bound on it.
#[derive(
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Default,
	PartialEq,
	Eq,
	Debug,
)]
pub struct DrawBudget<Balance> {
	/// Maximum that may be drawn per drip period. Set by [`Config::BudgetOrigin`].
	pub limit: Balance,
	/// Amount already drawn during the drip period identified by `period`.
	pub spent: Balance,
	/// The [`LastIssuanceTimestamp`] that `spent` is accounted against. A stale one reads as
	/// zero spending, so the budget refreshes without any drip having to reset counters.
	pub period: u64,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;
	use frame_support::{sp_runtime::traits::AccountIdConversion, traits::StorageVersion};
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

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
		/// Funds were drained from the staging account into the DAP buffer.
		StagingDrained {
			/// Amount drained.
			amount: BalanceOf<T>,
		},
		/// The per-drip-period draw budget of a buffer draw was set, updated or removed.
		DrawBudgetUpdated {
			/// The draw that was configured.
			key: BudgetKey,
			/// The new per-drip-period limit, or `None` if the draw was deregistered.
			limit: Option<BalanceOf<T>>,
		},
		/// A registered draw was paid out of the buffer.
		BufferDrawn {
			/// The draw the payment was charged against.
			key: BudgetKey,
			/// Who was paid.
			beneficiary: T::AccountId,
			/// Amount paid.
			amount: BalanceOf<T>,
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

	/// Registry of authorised draws on the buffer: `BudgetKey -> DrawBudget`.
	///
	/// The single place where outflows from the buffer are authorised and capped. Keys are
	/// registered by [`Pallet::set_draw_budget`].
	#[pallet::storage]
	pub type BufferDraws<T: Config> = StorageValue<_, BufferDrawMap<T>, ValueQuery>;

	/// Genesis configuration for the buffer draw registry.
	///
	/// Existing chains seed it with [`migrations::MigrateV2ToV3`] instead.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Draws to register, as `(key, per-drip-period limit)`.
		pub buffer_draws: Vec<(BudgetKey, BalanceOf<T>)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let mut draws = BufferDrawMap::<T>::new();
			for (key, limit) in &self.buffer_draws {
				draws
					.try_insert(key.clone(), DrawBudget { limit: *limit, ..Default::default() })
					.expect("more genesis buffer draws than MAX_BUFFER_DRAWS");
			}
			BufferDraws::<T>::put(draws);
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A key in the budget allocation does not match any registered recipient.
		UnknownBudgetKey,
		/// Budget allocation percentages do not sum to exactly 100%.
		BudgetNotExact,
		/// No draw is registered under this key, so nothing may be paid on its behalf.
		UnregisteredDraw,
		/// The payment would take the draw over its budget for the current drip period.
		DrawBudgetExceeded,
		/// The buffer draw registry is full; deregister a draw before adding another.
		TooManyDraws,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::drip_issuance()
		}

		fn on_idle(_block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(remaining_weight);

			// Need at least one read (staging account balance).
			if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
				return meter.consumed();
			}

			let staging_account = Self::staging_account();
			let available = T::Currency::reducible_balance(
				&staging_account,
				Preservation::Preserve,
				Fortitude::Polite,
			);

			if available.is_zero() {
				return meter.consumed();
			}

			// Need 1 read and 2 writes for the transfer, plus 1 read and 1 write for
			// deactivate (InactiveIssuance) and 1 read for TotalIssuance.
			if meter.try_consume(T::DbWeight::get().reads_writes(3, 3)).is_err() {
				return meter.consumed();
			}

			let buffer = Self::buffer_account();
			if T::Currency::transfer(&staging_account, &buffer, available, Preservation::Preserve)
				.is_err()
			{
				defensive!("DAP: staging account transfer to buffer failed");
				return meter.consumed();
			}

			Self::deactivate_buffer_funds(available);
			Self::deposit_event(Event::StagingDrained { amount: available });

			log::debug!(
				target: LOG_TARGET,
				"DAP: drained {available:?} from staging account to DAP buffer"
			);

			meter.consumed()
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

		/// Register a draw on the buffer under `key`, capped at `limit` per drip period, or
		/// deregister it with `limit == None`.
		///
		/// A new limit applies immediately but keeps what the draw already spent this period, so
		/// it cannot be used to refresh a depleted budget early.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_draw_budget())]
		pub fn set_draw_budget(
			origin: OriginFor<T>,
			key: BudgetKey,
			limit: Option<BalanceOf<T>>,
		) -> DispatchResult {
			T::BudgetOrigin::ensure_origin(origin)?;

			BufferDraws::<T>::try_mutate(|draws| -> DispatchResult {
				match limit {
					Some(limit) => {
						let spent_so_far = draws.get(&key).map(|d| (d.spent, d.period));
						let (spent, period) = spent_so_far.unwrap_or((Zero::zero(), 0));
						draws
							.try_insert(key.clone(), DrawBudget { limit, spent, period })
							.map_err(|_| Error::<T>::TooManyDraws)?;
					},
					None => {
						draws.remove(&key);
					},
				}
				Ok(())
			})?;

			Self::deposit_event(Event::DrawBudgetUpdated { key, limit });

			Ok(())
		}
	}

	#[pallet::view_functions]
	impl<T: Config> Pallet<T> {
		/// All registered budget recipients with their current allocation shares.
		///
		/// The `Perbill` is taken from `BudgetAllocation`; recipients absent from
		/// the map appear with `Perbill::zero()`.
		pub fn budget_recipients() -> Vec<(BudgetKey, T::AccountId, Perbill)> {
			let allocation = BudgetAllocation::<T>::get();

			T::BudgetRecipients::recipients()
				.into_iter()
				.map(|(key, account)| {
					let share = allocation.get(&key).copied().unwrap_or(Perbill::zero());

					(key, account, share)
				})
				.collect()
		}

		/// Account that holds burned/slashed funds before they are drained into
		/// the DAP buffer by `on_idle`. Exposed to clients so they don't have to
		/// re-derive the sub-account themselves.
		pub fn staging() -> T::AccountId {
			Self::staging_account()
		}

		/// All registered buffer draws as `(key, per-period limit, still drawable this period)`.
		pub fn buffer_draws() -> Vec<(BudgetKey, BalanceOf<T>, BalanceOf<T>)> {
			let period = LastIssuanceTimestamp::<T>::get();

			BufferDraws::<T>::get()
				.into_iter()
				.map(|(key, draw)| {
					let remaining = draw.limit.saturating_sub(Self::spent_in(&draw, period));
					(key, draw.limit, remaining)
				})
				.collect()
		}
	}

	impl<T: Config> Pallet<T> {
		/// The DAP buffer account.
		///
		/// Collects any burn source wired to it (staking slashes, unclaimed rewards, etc.)
		/// and its explicit budget allocation share.
		pub fn buffer_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// The DAP staging account.
		///
		/// Incoming funds land here and are periodically drained and deactivated into the
		/// DAP buffer account by `on_idle`.
		pub fn staging_account() -> T::AccountId {
			sp_dap::DAP_PALLET_ID.into_sub_account_truncating(sp_dap::DAP_STAGING_ACCOUNT_ID)
		}

		/// Deactivate funds on buffer inflow.
		pub(crate) fn deactivate_buffer_funds(amount: BalanceOf<T>) {
			<T::Currency as Unbalanced<T::AccountId>>::deactivate(amount);
		}

		/// Reactivate funds on buffer outflow, mirroring [`Self::deactivate_buffer_funds`].
		pub(crate) fn reactivate_buffer_funds(amount: BalanceOf<T>) {
			<T::Currency as Unbalanced<T::AccountId>>::reactivate(amount);
		}

		/// What `draw` has spent in the drip period identified by `period`; zero once a drip has
		/// moved the period on.
		pub(crate) fn spent_in(draw: &DrawBudget<BalanceOf<T>>, period: u64) -> BalanceOf<T> {
			if draw.period == period {
				draw.spent
			} else {
				Zero::zero()
			}
		}

		/// Pay `amount` from the buffer to `beneficiary`, charged against the draw registered
		/// under `key`, and reactivate the paid amount.
		///
		/// The only way funds leave the buffer. All-or-nothing: on `Err` nothing changed.
		pub fn pay_from_buffer(
			key: &BudgetKey,
			beneficiary: &T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			if amount.is_zero() {
				return Ok(());
			}

			let period = LastIssuanceTimestamp::<T>::get();
			let mut draw = BufferDraws::<T>::get().get(key).cloned().ok_or_else(|| {
				log::warn!(target: LOG_TARGET, "Buffer draw refused: {key:?} is not registered");
				Error::<T>::UnregisteredDraw
			})?;

			let spent = Self::spent_in(&draw, period);
			let remaining = draw.limit.saturating_sub(spent);
			if amount > remaining {
				log::warn!(
					target: LOG_TARGET,
					"Buffer draw refused: {key:?} wants {amount:?} but only {remaining:?} is \
					 left of its {:?} budget this period",
					draw.limit
				);
				return Err(Error::<T>::DrawBudgetExceeded.into());
			}

			// Written back only after the transfer, so a failure does not consume budget.
			T::Currency::transfer(
				&Self::buffer_account(),
				beneficiary,
				amount,
				Preservation::Preserve,
			)?;
			Self::reactivate_buffer_funds(amount);

			draw.spent = spent.saturating_add(amount);
			draw.period = period;
			BufferDraws::<T>::mutate(|draws| {
				let _ = draws
					.try_insert(key.clone(), draw)
					.defensive_proof("key is already present in the map; qed");
			});

			Self::deposit_event(Event::BufferDrawn {
				key: key.clone(),
				beneficiary: beneficiary.clone(),
				amount,
			});

			log::debug!(
				target: LOG_TARGET,
				"Paid {amount:?} from the DAP buffer to {beneficiary:?} against draw {key:?}"
			);

			Ok(())
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

			// Always advance the clock so elapsed time doesn't accumulate across skipped drips.
			LastIssuanceTimestamp::<T>::put(now);

			let _ = Self::mint_and_distribute(elapsed);
			T::WeightInfo::drip_issuance()
		}

		/// Mints `IssuanceCurve::issue(total_issuance, elapsed)` and distributes the
		/// result according to `BudgetAllocation`.
		///
		/// Does not read or write `LastIssuanceTimestamp`, does not enforce
		/// `IssuanceCadence`, and does not apply the `MaxElapsedPerDrip` safety
		/// ceiling.
		///
		/// Returns the total amount successfully minted. Individual recipient mint
		/// failures emit `MintFailed` and are skipped; the function does not roll
		/// back successful mints for earlier recipients.
		pub(crate) fn mint_and_distribute(elapsed: u64) -> BalanceOf<T> {
			let total_issuance = T::Currency::total_issuance();
			let issuance = T::IssuanceCurve::issue(total_issuance, elapsed);

			if issuance.is_zero() {
				return BalanceOf::<T>::zero();
			}

			let budget = BudgetAllocation::<T>::get();
			if budget.is_empty() {
				// TODO: Add defensive! panic once budget is always configured.
				log::warn!(
					target: LOG_TARGET,
					"BudgetAllocation is empty — no issuance will be distributed"
				);
				return BalanceOf::<T>::zero();
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
				"Issuance drip: issued={issuance:?}, minted={total_minted:?}, elapsed={elapsed}ms"
			);

			debug_assert!(
				!total_minted.is_zero(),
				"mint_and_distribute: issuance was non-zero but nothing was minted"
			);

			total_minted
		}
	}

	#[cfg(any(test, feature = "try-runtime"))]
	impl<T: Config> Pallet<T> {
		#[allow(dead_code)]
		pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			Self::check_budget_allocation()?;
			Self::check_buffer_draws()
		}

		/// Checks that no [`BufferDraws`] entry is accounted against a future drip period, which
		/// would let it out-spend its budget once that period arrives.
		fn check_buffer_draws() -> Result<(), sp_runtime::TryRuntimeError> {
			let current_period = LastIssuanceTimestamp::<T>::get();

			for (_key, draw) in BufferDraws::<T>::get() {
				ensure!(
					draw.period <= current_period,
					"BufferDraws entry is accounted against a future drip period"
				);
			}

			Ok(())
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

/// Implementation of `OnUnbalanced` for the `fungible::Balanced` trait.
/// Example: use as `type Slash = Dap` in staking-async config.
///
/// For pallets still using the legacy `Currency` trait (e.g. `pallet_referenda`),
/// use [`DapLegacyAdapter`] instead.
impl<T: Config> OnUnbalanced<CreditOf<T>> for Pallet<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let staging = Self::staging_account();
		let numeric_amount = amount.peek();

		// Funds land in the staging account; `on_idle` will drain them into the buffer and
		// deactivate them there.  Deactivation is intentionally deferred so that active issuance
		// does not flicker down-then-up within the same block.
		let _ = T::Currency::resolve(&staging, amount).inspect_err(|_| {
			defensive!(
				"🚨 Failed to deposit slash to DAP staging account - funds burned, it should never happen!"
			);
		});
		log::debug!(
			target: LOG_TARGET,
			"💸 Deposited {numeric_amount:?} to DAP staging account"
		);
	}
}

/// Type alias for legacy `NegativeImbalance` from the `Currency` trait.
type LegacyNegativeImbalance<A, C> = <C as Currency<A>>::NegativeImbalance;

/// Adapter that redirects `NegativeImbalance` from the legacy `Currency` trait to the DAP buffer.
///
/// Cannot be implemented directly on `Pallet<T>` because the compiler cannot prove that
/// `<C as Currency>::NegativeImbalance` and `fungible::Credit` are always distinct types,
/// so two `OnUnbalanced` impls on the same struct are rejected.
///
/// Will be removed once all consumer pallets migrate to fungible traits.
///
/// # Example
/// ```ignore
/// type Slash = pallet_dap::DapLegacyAdapter<Runtime, Balances>;
/// ```
pub struct DapLegacyAdapter<T, C>(core::marker::PhantomData<(T, C)>);

impl<T: Config, C> OnUnbalanced<LegacyNegativeImbalance<T::AccountId, C>> for DapLegacyAdapter<T, C>
where
	C: Currency<T::AccountId, Balance = BalanceOf<T>>,
{
	fn on_nonzero_unbalanced(amount: LegacyNegativeImbalance<T::AccountId, C>) {
		let staging = Pallet::<T>::staging_account();
		let numeric_amount = amount.peek();
		// NOTE: resolve_creating is infallible.
		C::resolve_creating(&staging, amount);
		log::debug!(
			target: LOG_TARGET,
			"💸 Deposited (legacy) {numeric_amount:?} to DAP staging account"
		);
	}
}

/// A registered draw on the DAP buffer, bound to the [`BudgetKey`] in `K`.
///
/// Hand this to any pallet that needs paying out of the buffer, instead of it holding the buffer
/// account. Payments are capped by the draw's budget in [`BufferDraws`].
///
/// # Example
/// ```ignore
/// parameter_types! {
///     pub SignedRewards: BudgetKey = BudgetKey::truncate_from(b"epmb_signed".to_vec());
/// }
///
/// type RewardSource = pallet_dap::BufferDraw<Runtime, SignedRewards>;
/// ```
pub struct BufferDraw<T, K>(core::marker::PhantomData<(T, K)>);

impl<T: Config, K: Get<BudgetKey>> PaymentSource<T::AccountId, BalanceOf<T>> for BufferDraw<T, K> {
	fn pay(beneficiary: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		Pallet::<T>::pay_from_buffer(&K::get(), beneficiary, amount)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_can_pay(amount: BalanceOf<T>) {
		// The payout preserves the buffer, so it needs `amount` on top of the ED.
		let buffer = Pallet::<T>::buffer_account();
		let _ =
			T::Currency::mint_into(&buffer, amount.saturating_add(T::Currency::minimum_balance()));
		Pallet::<T>::deactivate_buffer_funds(amount);

		let key = K::get();
		let period = LastIssuanceTimestamp::<T>::get();
		BufferDraws::<T>::mutate(|draws| {
			let spent = draws
				.get(&key)
				.map(|draw| Pallet::<T>::spent_in(draw, period))
				.unwrap_or_else(Zero::zero);
			let limit = spent.saturating_add(amount);
			let _ = draws.try_insert(key, DrawBudget { limit, spent, period });
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
