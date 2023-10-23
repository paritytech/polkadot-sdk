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

//! Migrate the democracy pallet to use the Fungible trait.
//! See <https://github.com/paritytech/polkadot-sdk/pull/1861>

use crate::*;
use frame_support::{
	pallet_prelude::*,
	storage_alias,
	traits::{LockableCurrency, OnRuntimeUpgrade, ReservableCurrency},
	BoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::hexdisplay::HexDisplay;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// The log target.
const LOG_TARGET: &'static str = "runtime::democracy::migration::v2";

/// Type alias for `frame_system`'s account id.
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Type alias for `democracy`'s fungible type.
pub type FungibleOf<T> = <T as pallet::Config>::Fungible;

pub mod old {
	use super::*;
	pub type MaxVotesOf<T> = <T as Config>::MaxDeposits;
	pub type MaxDepositsOf<T> = <T as Config>::MaxDeposits;

	#[storage_alias]
	pub type DepositOf<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		PropIndex,
		(BoundedVec<AccountIdOf<T>, MaxDepositsOf<T>>, BalanceOf<T>),
	>;

	#[storage_alias]
	pub type VotingOf<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		AccountIdOf<T>,
		Voting<BalanceOf<T>, AccountIdOf<T>, BlockNumberFor<T>, MaxVotesOf<T>>,
		ValueQuery,
	>;
}

pub struct Migration<T, OldCurrency>(sp_std::marker::PhantomData<(T, OldCurrency)>);
impl<T, OldCurrency> Migration<T, OldCurrency>
where
	T: Config,
	OldCurrency: 'static
		+ ReservableCurrency<AccountIdOf<T>>
		+ LockableCurrency<AccountIdOf<T>, Moment = BlockNumberFor<T>>,
	OldCurrency::Balance: IsType<BalanceOf<T>>,
{
	/// Return a tuple with a map of proposal deposits by account id and the number of proposals.
	pub fn get_deposits_and_proposal_count() -> (BTreeMap<AccountIdOf<T>, OldCurrency::Balance>, u32)
	{
		let mut proposal_count = 0u32;
		let map = old::DepositOf::<T>::iter()
			.flat_map(|(_prop_index, (accounts, balance))| {
				proposal_count += 1;
				accounts.into_iter().map(|account| (account, balance)).collect::<Vec<_>>()
			})
			.fold(
				BTreeMap::new(),
				|mut acc: BTreeMap<AccountIdOf<T>, OldCurrency::Balance>, (account, balance)| {
					acc.entry(account.clone())
						.or_insert(Zero::zero())
						.saturating_accrue(balance.into());
					acc
				},
			);

		(map, proposal_count)
	}

	/// Store proposal deposits for benchmarking purposes.
	#[cfg(any(feature = "runtime-benchmarks", feature = "try-runtime"))]
	pub fn bench_store_deposit(prop_index: PropIndex, depositors: Vec<AccountIdOf<T>>) {
		let amount = T::MinimumDeposit::get();
		for depositor in &depositors {
			OldCurrency::reserve(&depositor, amount.into()).expect("Failed to reserve deposit");
		}

		let depositors = BoundedVec::<_, T::MaxDeposits>::truncate_from(depositors);
		old::DepositOf::<T>::insert(prop_index, (depositors, amount));
	}

	/// Translate reserved deposit to held deposit.
	pub fn translate_reserve_to_hold(
		depositor: &AccountIdOf<T>,
		amount: OldCurrency::Balance,
	) -> Weight {
		let remaining = OldCurrency::unreserve(&depositor, amount);
		if remaining > Zero::zero() {
			log::warn!(
			target: LOG_TARGET,
			"account 0x{:?} has some non-unreservable deposit {:?} from a total of {:?}
			that will remain in reserved.",
			HexDisplay::from(&depositor.encode()),
			remaining,
			amount
			);
		}

		let amount = amount.saturating_sub(remaining);

		log::debug!(
			target: LOG_TARGET,
			"Holding {:?} on account 0x{:?}.",
			amount,
			HexDisplay::from(&depositor.encode()),
		);

		T::Fungible::hold(&HoldReason::Proposal.into(), &depositor, amount.into()).unwrap_or_else(
			|err| {
				log::error!(
					target: LOG_TARGET,
					"Failed to hold {:?} from account 0x{:?}, reason: {:?}.",
					amount,
					HexDisplay::from(&depositor.encode()),
					err
				);
			},
		);

		T::WeightInfo::v2_migration_translate_reserve_to_hold()
	}

	/// Store votes for benchmarking purposes.
	#[cfg(any(feature = "runtime-benchmarks", feature = "try-runtime"))]
	pub fn bench_store_vote(voter: AccountIdOf<T>) {
		use frame_support::traits::WithdrawReasons;
		let balance = 1_000_000u32;
		OldCurrency::set_lock(
			DEMOCRACY_ID,
			&voter,
			balance.into(),
			WithdrawReasons::except(WithdrawReasons::RESERVE),
		);
		let votes = vec![(
			0u32,
			AccountVote::Standard {
				vote: Vote { aye: true, conviction: Conviction::Locked1x },
				balance: balance.into(),
			},
		)];
		let votes = BoundedVec::<_, T::MaxVotes>::truncate_from(votes);
		let vote =
			Voting::Direct { votes, delegations: Default::default(), prior: Default::default() };
		VotingOf::<T>::insert(voter, vote);
	}

	/// Translate votes locked deposit to frozen deposit.
	pub fn translate_lock_to_freeze(
		account_id: AccountIdOf<T>,
		amount: OldCurrency::Balance,
	) -> Weight {
		OldCurrency::remove_lock(DEMOCRACY_ID, &account_id);
		T::Fungible::set_freeze(&FreezeReason::Vote.into(), &account_id, amount.into())
			.unwrap_or_else(|err| {
				log::error!(
					target: LOG_TARGET,
					"Failed to freeze {:?} from account 0x{:?}, reason: {:?}.",
					amount,
					HexDisplay::from(&account_id.encode()),
					err
				);
			});

		T::WeightInfo::v2_migration_translate_lock_to_freeze()
	}
}

impl<T, OldCurrency> OnRuntimeUpgrade for Migration<T, OldCurrency>
where
	T: Config,
	OldCurrency: 'static
		+ ReservableCurrency<AccountIdOf<T>>
		+ LockableCurrency<AccountIdOf<T>, Moment = BlockNumberFor<T>>,
	OldCurrency::Balance: IsType<BalanceOf<T>>,
{
	fn on_runtime_upgrade() -> Weight {
		let mut weight = T::WeightInfo::v2_migration_base();

		if StorageVersion::get::<Pallet<T>>() != 1 {
			log::warn!(
				target: LOG_TARGET,
				"skipping on_runtime_upgrade: executed on wrong storage version.\
			Expected version 1"
			);
			return weight
		}

		// Convert reserved deposit to held deposit.
		let (deposits, proposal_count) = Self::get_deposits_and_proposal_count();
		weight.saturating_accrue(T::WeightInfo::v2_migration_proposals_count(proposal_count));

		deposits.into_iter().for_each(|(depositor, amount)| {
			weight.saturating_accrue(Self::translate_reserve_to_hold(&depositor, amount));
		});

		// Convert locked deposit to frozen deposit.
		old::VotingOf::<T>::iter().for_each(|(account_id, voting)| {
			let balance = voting.locked_balance().into();
			weight.saturating_accrue(T::WeightInfo::v2_migration_read_next_vote());
			weight.saturating_accrue(Self::translate_lock_to_freeze(account_id, balance));
		});

		StorageVersion::new(2).put::<Pallet<T>>();
		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::traits::fungible::InspectHold;

		ensure!(StorageVersion::get::<Pallet<T>>() == 2, "must upgrade");
		let (deposits, _) = Self::get_deposits_and_proposal_count();

		for (depositor, amount) in deposits {
			assert_eq!(
				FungibleOf::<T>::balance_on_hold(&HoldReason::Proposal.into(), &depositor),
				amount.into()
			);
		}

		for (voter, voting) in old::VotingOf::<T>::iter() {
			assert_eq!(
				FungibleOf::<T>::balance_frozen(&FreezeReason::Vote.into(), &voter),
				voting.locked_balance()
			);
		}

		Ok(())
	}
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
	use super::*;
	use crate::tests::{Test as T, *};
	use frame_support::traits::fungible::InspectHold;

	type MigrationOf<T> = Migration<T, pallet_balances::Pallet<T>>;

	#[test]
	fn migration_works() {
		new_test_ext().execute_with(|| {
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 0);
			StorageVersion::new(1).put::<Pallet<T>>();
			let alice = 1;

			// Store a proposal deposit and vote for alice.
			MigrationOf::<T>::bench_store_deposit(0u32, vec![alice]);
			MigrationOf::<T>::bench_store_vote(alice.into());

			// Check that alice's deposit is reserved and vote balance is locked.
			assert_eq!(pallet_balances::Pallet::<T>::reserved_balance(&alice), 1);
			assert_eq!(pallet_balances::Pallet::<T>::locks(&alice)[0].amount, 1_000_000);

			// Run migration.
			let state = MigrationOf::<T>::pre_upgrade().unwrap();
			MigrationOf::<T>::on_runtime_upgrade();
			MigrationOf::<T>::post_upgrade(state).unwrap();

			// Check that alice's deposit is now held instead of reserved.
			assert_eq!(FungibleOf::<T>::balance_on_hold(&HoldReason::Proposal.into(), &alice), 1);
			assert_eq!(
				FungibleOf::<T>::balance_frozen(&FreezeReason::Vote.into(), &alice),
				1_000_000
			);
			assert_eq!(pallet_balances::Pallet::<T>::locks(&alice).len(), 0);
		})
	}
}
