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

//! Distribution pallet.
//!
//! The Distribution Pallet handles the distribution of whitelisted projects rewards.
//! For now only one reward distribution pattern has been implemented,
//! but the pallet could be extended to offer to the user claiming rewards for a project,
//! a choice between more than one distribution pattern.
//!
//! ## Overview
//!
//! The Distribution Pallet receives a list of Whitelisted/Nominated Projects with their respective
//! calculated rewards. For each project, it will create a corresponding spend that will be stored
//! until the project reward can be claimed. At the moment, the reward claim period start
//! corresponds to: [beginning of an Epoch_Block + BufferPeriod] (The BufferPeriod can be configured
//! in the runtime).
//!
//! ### Terminology
//!
//! - **PotId:** Pot containing the funds used to pay the rewards.
//! - **BufferPeriod:** Minimum required buffer time period between project nomination and reward
//!   claim.
//!
//! ## Interface
//!
//! ### Permissionless Functions
//!
//! * `pot_account`: Output the pot account_id.
//! * `pot_check`: Series of checks on the Pot, to ensure that we have enough funds before executing
//!   a Spend.
//! * `spend`: Funds transfer from the Pot to a project account.
//!
//! ### Privileged Functions
//!
//! * `claim_reward_for`: Claim a reward for a nominated/whitelisted project.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
mod functions;
mod types;
pub use types::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;

		/// Treasury account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		type RuntimeHoldReason: From<HoldReason>;

		/// This the minimum required buffer time period between project nomination
		/// and payment/reward_claim from the treasury.
		#[pallet::constant]
		type BufferPeriod: Get<BlockNumberFor<Self>>;

		/// Maximum number projects that can be accepted by this pallet
		#[pallet::constant]
		type MaxProjects: Get<u32>;

		/// Epoch duration in blocks
		#[pallet::constant]
		type EpochDurationBlocks: Get<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for a given buffer time before payment
		#[codec(index = 0)]
		FundsReserved,
	}

	/// Spends that still have to be claimed.
	#[pallet::storage]
	pub(super) type Spends<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, SpendInfo<T>, OptionQuery>;

	/// List of whitelisted projects to be rewarded
	#[pallet::storage]
	pub type Projects<T: Config> =
		StorageValue<_, BoundedVec<ProjectInfo<T>, T::MaxProjects>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardClaimed { when: BlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// A Spend was created
		SpendCreated { when: BlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough Funds in the Pot
		InsufficientPotReserves,
		/// The funds transfer operation failed
		TransferFailed,
		/// Spend or Spend index does not exists
		InexistentSpend,
		/// No valid Account_id found
		NoValidAccount,
		/// No project available for funding
		NoProjectAvailable,
		/// The Funds transfer failed
		FailedSpendOperation,
		/// Still not in claiming period
		NotClaimingPeriod,
		/// Funds locking failed
		FundsReserveFailed,
		/// An invalid result  was returned
		InvalidResult,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			Self::begin_block(n)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// OPF Reward Claim logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// From this extrinsic any user can claim a reward for a nominated/whitelisted project.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::InexistentSpend`]:Spend or Spend index does not exists
		/// - [`Error::<T>::NoValidAccount`]:  No valid Account_id found
		/// - [`Error::<T>::NotClaimingPeriod`]: Still not in claiming period
		///  
		/// ## Events
		/// Emits [`Event::<T>::RewardClaimed`] if successful for a positive approval.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim_reward_for(T::MaxProjects::get()))]
		pub fn claim_reward_for(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let _caller = ensure_signed(origin)?;
			let pot = Self::pot_account();
			let info = Spends::<T>::get(&project_id).ok_or(Error::<T>::InexistentSpend)?;
			let now = T::BlockNumberProvider::current_block_number();

			// Check that we're within the claiming period
			ensure!(now > info.valid_from, Error::<T>::NotClaimingPeriod);
			// Unlock the funds
			T::NativeBalance::release(
				&HoldReason::FundsReserved.into(),
				&pot,
				info.amount,
				Precision::Exact,
			)?;
			// transfer the funds
			Self::spend(info.amount, project_id.clone())?;

			let infos = Spends::<T>::take(&project_id).ok_or(Error::<T>::InexistentSpend)?;

			Self::deposit_event(Event::RewardClaimed {
				when: now,
				amount: infos.amount,
				project_id,
			});

			Ok(())
		}
	}
}
