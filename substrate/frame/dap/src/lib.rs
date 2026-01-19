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
//! This pallet manages issuance and distribution of staking rewards through era pot accounts.
//!
//! ## Key Responsibilities:
//!
//! - **Slash Collection**: Implements `OnUnbalanced` to collect slashed funds into a buffer account
//!   instead of burning them.
//! - **Era Reward Management**: Implements `StakingRewardProvider` to mint and manage era reward
//!   pot accounts that the staking implementation can pull for staker payouts.
//!
//! For existing chains adding DAP, include `dap::migrations::v1::InitBufferAccount` in your
//! migrations tuple.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use codec::DecodeWithMemTracking;
use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate},
		Imbalance, OnUnbalanced,
	},
	PalletId,
};
use sp_runtime::traits::{CheckedAdd, Saturating, Zero};
use sp_staking::{EraIndex, EraPayoutV2, StakingRewardProvider};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Budget allocation configuration for era emission.
///
/// Defines how the total era inflation is split across different reward categories.
/// All allocations must sum to exactly 100% (`Perbill::one()`).
///
/// The buffer accumulates funds for multiple purposes:
/// - Treasury budget
/// - Strategic reserve
/// - Funds for acquiring stablecoins for validator/collator operational costs
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Clone,
	PartialEq,
	Eq,
	Debug,
	Default,
	Copy,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct BudgetConfig {
	/// Allocation to stakers (nominators + validator stake rewards).
	///
	/// This is the traditional staking reward that rewards staker based on their stake.
	pub staker_rewards: sp_runtime::Perbill,

	/// Allocation to validator self-stake incentive (vested).
	///
	/// Extra rewards for validators based on their self-stake, vested over time to encourage
	/// long-term commitment.
	pub validator_self_stake_incentive: sp_runtime::Perbill,

	/// Allocation to buffer.
	///
	/// The buffer accumulates funds for treasury transfers, stablecoin acquisition, and strategic
	/// reserve. All allocations must explicitly sum to 100%.
	pub buffer: sp_runtime::Perbill,
}

impl BudgetConfig {
	/// Validates that all budget allocations sum to exactly 100%.
	///
	/// Returns true if the configuration is valid, false otherwise.
	pub fn is_valid(&self) -> bool {
		let Some(partial) = self.staker_rewards.checked_add(&self.validator_self_stake_incentive)
		else {
			return false;
		};

		let Some(total) = partial.checked_add(&self.buffer) else {
			return false;
		};

		total == sp_runtime::Perbill::one()
	}

	/// Returns the default budget configuration.
	///
	/// Maintains backward compatibility with the previous 85%/15% split:
	/// - 85% to stakers
	/// - 0% to validator self-stake incentive
	/// - 15% to buffer for strategic reserve, treasury, operational costs
	pub fn default_config() -> Self {
		Self {
			staker_rewards: sp_runtime::Perbill::from_percent(85),
			validator_self_stake_incentive: sp_runtime::Perbill::from_percent(0),
			buffer: sp_runtime::Perbill::from_percent(15),
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{sp_runtime::traits::AccountIdConversion, traits::StorageVersion};
	use frame_system::pallet_prelude::OriginFor;

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
			+ Balanced<Self::AccountId>;

		/// The pallet ID used to derive the buffer account.
		///
		/// Each runtime should configure a unique ID to avoid collisions if multiple
		/// DAP instances are used.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Era payout implementation.
		///
		/// This is typically implemented in the runtime to provide the inflation curve logic.
		type EraPayout: EraPayoutV2<BalanceOf<Self>>;

		/// Origin that can update budget allocation parameters.
		type BudgetOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Era rewards allocated and minted.
		///
		/// Emitted when a new era's rewards are computed and minted into the respective accounts.
		EraRewardsAllocated {
			/// Era index for which rewards were allocated.
			era: EraIndex,
			/// Amount minted for staker rewards (nominators + validator stake rewards).
			staker_rewards: BalanceOf<T>,
			/// Amount minted for validator incentive.
			validator_incentive: BalanceOf<T>,
			/// Amount minted for buffer (treasury, strategic reserve, operational costs).
			buffer_rewards: BalanceOf<T>,
		},
		/// Budget allocation configuration was updated.
		BudgetAllocationUpdated {
			/// The new budget configuration.
			config: BudgetConfig,
		},
		/// An unexpected/defensive event was triggered.
		Unexpected(UnexpectedKind),
	}

	/// Defensive/unexpected errors/events.
	///
	/// In case of observation in explorers, report it as an issue in polkadot-sdk.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, DebugNoBound)]
	pub enum UnexpectedKind {
		/// Failed to mint era inflation.
		EraMintFailed { era: EraIndex },
	}

	#[pallet::type_value]
	pub fn DefaultBudgetConfig() -> BudgetConfig {
		BudgetConfig::default_config()
	}

	/// Budget allocation configuration storage.
	///
	/// Stores the current distribution of era rewards across different categories.
	/// Defaults to 85% stakers, 0% validator self-stake incentive, 15% treasury.
	#[pallet::storage]
	#[pallet::getter(fn budget_allocation)]
	pub type BudgetAllocation<T> = StorageValue<_, BudgetConfig, ValueQuery, DefaultBudgetConfig>;

	#[pallet::error]
	pub enum Error<T> {
		/// Budget allocation configuration is invalid (percentages do not add upto 100%).
		InvalidBudgetConfig,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the budget allocation configuration.
		///
		/// Updates how era emission is distributed across different categories.
		/// The configuration must be valid (all percentages sum to == 100%).
		///
		/// # Errors
		/// - `InvalidBudgetConfig` if percentages sum to != 100%
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().reads_writes(0, 1))]
		pub fn set_budget_allocation(
			origin: OriginFor<T>,
			new_config: BudgetConfig,
		) -> DispatchResult {
			T::BudgetOrigin::ensure_origin(origin)?;

			ensure!(new_config.is_valid(), Error::<T>::InvalidBudgetConfig);

			BudgetAllocation::<T>::put(new_config);

			Self::deposit_event(Event::BudgetAllocationUpdated { config: new_config });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the DAP buffer account.
		///
		/// The buffer account collects:
		/// - Slashed funds and other burns.
		/// - Treasury portion of era rewards
		/// - Unclaimed staker rewards
		/// - Part of era emission based on [`BudgetConfig`].
		pub fn buffer_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Create the buffer account with a provider reference and fund it with ED.
		///
		/// Called once at genesis (for new chains and test/benchmark setup) or via migration
		/// (for existing chains). Safe to call multiple times - will early exit if account
		/// already exists with sufficient balance.
		pub fn create_buffer_account() {
			let buffer = Self::buffer_account();
			let ed = T::Currency::minimum_balance();

			if frame_system::Pallet::<T>::providers(&buffer) > 0 &&
				T::Currency::balance(&buffer) >= ed
			{
				log::debug!(
					target: LOG_TARGET,
					"DAP buffer account already initialized: {buffer:?}"
				);
				return;
			}

			// Ensure the account exists by incrementing its provider count.
			frame_system::Pallet::<T>::inc_providers(&buffer);
			log::info!(
				target: LOG_TARGET,
				"Attempting to mint ED ({ed:?}) into DAP buffer: {buffer:?}"
			);

			match T::Currency::mint_into(&buffer, ed) {
				Ok(_) => {
					log::info!(
						target: LOG_TARGET,
						"🏦 Created DAP buffer account: {buffer:?}"
					);
				},
				Err(e) => {
					log::error!(
						target: LOG_TARGET,
						"🚨 Failed to mint ED into DAP buffer: {e:?}"
					);
				},
			}
		}
	}

	/// Genesis config for the DAP pallet.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		_phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Create and fund the buffer account at genesis.
			Pallet::<T>::create_buffer_account();
		}
	}
}

/// Migrations for the DAP pallet.
pub mod migrations {
	use super::*;

	/// Version 1 migration.
	pub mod v1 {
		use super::*;

		mod inner {
			use super::*;
			use frame_support::traits::UncheckedOnRuntimeUpgrade;

			/// Inner migration that creates the buffer account.
			pub struct InitBufferAccountInner<T>(core::marker::PhantomData<T>);

			impl<T: Config> UncheckedOnRuntimeUpgrade for InitBufferAccountInner<T> {
				fn on_runtime_upgrade() -> Weight {
					Pallet::<T>::create_buffer_account();
					// Weight: inc_providers (1 read, 1 write) + mint_into (2 reads, 2 writes)
					T::DbWeight::get().reads_writes(3, 3)
				}
			}
		}

		/// Migration to create the DAP buffer account (version 0 → 1).
		pub type InitBufferAccount<T> = frame_support::migrations::VersionedMigration<
			0,
			1,
			inner::InitBufferAccountInner<T>,
			Pallet<T>,
			<T as frame_system::Config>::DbWeight,
		>;
	}
}

/// Type alias for credit (negative imbalance - funds that were slashed/removed).
/// This is for the `fungible::Balanced` trait as used by staking-async.
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Implementation of OnUnbalanced for the fungible::Balanced trait.
/// Example: use as `type Slash = Dap` in staking-async config.
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
				defensive!("🚨 Failed to deposit slash to DAP buffer - funds burned, it should never happen!");
			})
			.inspect(|_| {
				log::debug!(
					target: LOG_TARGET,
					"💸 Deposited slash of {numeric_amount:?} to DAP buffer"
				);
			});
	}
}

impl<T: Config> StakingRewardProvider<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn allocate_era_rewards(
		era: EraIndex,
		total_staked: BalanceOf<T>,
		era_duration_millis: u64,
		staker_pot: &T::AccountId,
		validator_incentive_pot: &T::AccountId,
	) -> sp_staking::EraRewardAllocation<BalanceOf<T>> {
		// Look up total issuance
		let total_issuance = T::Currency::total_issuance();

		// Compute total era inflation using EraPayoutV2
		let total_inflation = T::EraPayout::era_payout(
			total_staked,
			total_issuance,
			// note: era_duration_millis already is defensively capped by staking implementation
			era_duration_millis,
		);

		// Get current budget allocation configuration
		let budget = BudgetAllocation::<T>::get();

		// Split total inflation according to budget configuration
		let to_stakers = budget.staker_rewards.mul_floor(total_inflation);
		let to_validator_incentive =
			budget.validator_self_stake_incentive.mul_floor(total_inflation);

		// Buffer gets the remainder to ensure all inflation is minted (no rounding dust lost)
		let to_buffer = total_inflation
			.saturating_sub(to_stakers)
			.saturating_sub(to_validator_incentive);

		log::info!(
			target: LOG_TARGET,
			"💰 Era {era} allocation: total={total_inflation:?}, stakers={to_stakers:?}, \
			validator_incentive={to_validator_incentive:?}, buffer={to_buffer:?}"
		);

		// Mint staker rewards into the staker pot
		if !to_stakers.is_zero() {
			if let Err(_e) = T::Currency::mint_into(staker_pot, to_stakers) {
				defensive!("Era Mint should never fail");
				Self::deposit_event(Event::Unexpected(UnexpectedKind::EraMintFailed { era }));
				// Return zero allocation on failure
				return sp_staking::EraRewardAllocation {
					staker_rewards: Default::default(),
					validator_incentive: Default::default(),
				};
			}
		}

		// Mint validator incentive into the validator incentive pot
		if !to_validator_incentive.is_zero() {
			if let Err(_e) = T::Currency::mint_into(validator_incentive_pot, to_validator_incentive)
			{
				defensive!("Era Mint should never fail");
				Self::deposit_event(Event::Unexpected(UnexpectedKind::EraMintFailed { era }));
				// We already minted staker rewards, so continue
			}
		}

		// Mint buffer portion into the buffer account
		let buffer = Self::buffer_account();
		if !to_buffer.is_zero() {
			if let Err(_e) = T::Currency::mint_into(&buffer, to_buffer) {
				defensive!("Era Mint should never fail");
				Self::deposit_event(Event::Unexpected(UnexpectedKind::EraMintFailed { era }));
				// Continue even if buffer mint fails
			}
		}

		Self::deposit_event(Event::EraRewardsAllocated {
			era,
			staker_rewards: to_stakers,
			validator_incentive: to_validator_incentive,
			buffer_rewards: to_buffer,
		});

		// Return the allocation breakdown
		sp_staking::EraRewardAllocation {
			staker_rewards: to_stakers,
			validator_incentive: to_validator_incentive,
		}
	}
}

impl<T: Config> sp_staking::UnclaimedRewardSink<T::AccountId> for Pallet<T> {
	fn unclaimed_reward_sink() -> T::AccountId {
		Self::buffer_account()
	}
}
