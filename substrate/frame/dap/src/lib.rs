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

use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate},
		Imbalance, OnUnbalanced,
	},
	PalletId,
};
use sp_staking::{EraIndex, EraPayout, StakingRewardProvider};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{sp_runtime::traits::AccountIdConversion, traits::StorageVersion};

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
		/// Called by DAP to determine how much to mint for each era.
		type EraPayout: EraPayout<BalanceOf<Self>>;
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
			/// Amount minted for staker rewards (deposited into era pot).
			staker_rewards: BalanceOf<T>,
			/// Amount minted for treasury (deposited into buffer).
			treasury_rewards: BalanceOf<T>,
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

	impl<T: Config> Pallet<T> {
		/// Get the DAP buffer account.
		///
		/// The buffer account collects:
		/// - Slashed funds and other burns.
		/// - Treasury portion of era rewards
		/// - Unclaimed staker rewards
		/// - Future strategic reserves
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
		_phantom: core::marker::PhantomData<T>,
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

/// Implementation of StakingRewardProvider for pallet-dap.
impl<T: Config> StakingRewardProvider<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn allocate_era_rewards(
		era: EraIndex,
		total_staked: BalanceOf<T>,
		era_duration_millis: u64,
		staking_era_pot: &T::AccountId,
	) -> BalanceOf<T> {
		// Look up total issuance
		let total_issuance = T::Currency::total_issuance();

		// Compute era payout
		let (to_stakers, to_treasury) = T::EraPayout::era_payout(
			total_staked,
			total_issuance,
			// note: era_duration_millis already is defensively capped by staking implementation
			era_duration_millis,
		);

		log::info!(
			target: LOG_TARGET,
			"💰 Era {era} allocation: to_stakers={to_stakers:?}, to_treasury={to_treasury:?}"
		);

		// Mint `to_stakers` into the provided era system account
		// Skip if zero amount
		if !to_stakers.is_zero() {
			if let Err(_e) = T::Currency::mint_into(staking_era_pot, to_stakers) {
				// fail in tests, log in prod
				defensive!("Era Mint should never fail");
				// trigger unexpected event for observability
				Self::deposit_event(Event::Unexpected(UnexpectedKind::EraMintFailed { era }));
				// can't do much here, just return!
				return Default::default();
			}
		}

		// Mint treasury portion into the buffer account
		let buffer = Self::buffer_account();
		if !to_treasury.is_zero() {
			if let Err(_e) = T::Currency::mint_into(&buffer, to_treasury) {
				// fail in tests, log in prod.
				defensive!("Era Mint should never fail");
				// trigger unexpected event for observability.
				Self::deposit_event(Event::Unexpected(UnexpectedKind::EraMintFailed { era }));
				// move on as we were able to mint staker reward
			}
		}

		Self::deposit_event(Event::EraRewardsAllocated {
			era,
			staker_rewards: to_stakers,
			treasury_rewards: to_treasury,
		});

		// Return the amount allocated to stakers
		to_stakers
	}
}

impl<T: Config> sp_staking::UnclaimedRewardSink<T::AccountId> for Pallet<T> {
	fn unclaimed_reward_sink() -> T::AccountId {
		Self::buffer_account()
	}
}
