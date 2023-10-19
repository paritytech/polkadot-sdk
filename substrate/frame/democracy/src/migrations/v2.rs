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

//! Storage migrations for the preimage pallet.

use crate::*;
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade, BoundedVec};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::hexdisplay::HexDisplay;

/// The log target.
const LOG_TARGET: &'static str = "runtime::democracy::migration::v2";

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// The original data layout of the democracy pallet without a specific version number.
mod old {
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

use frame_support::traits::{LockableCurrency, ReservableCurrency};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// Migration for translating bare `Hash`es into `Bounded<Call>`s.
pub struct Migration<T, OldCurrency>(sp_std::marker::PhantomData<(T, OldCurrency)>);
impl<T, OldCurrency> Migration<T, OldCurrency>
where
	T: Config,
	OldCurrency: 'static
		+ ReservableCurrency<AccountIdOf<T>>
		+ LockableCurrency<AccountIdOf<T>, Moment = BlockNumberFor<T>>,
	OldCurrency::Balance: IsType<BalanceOf<T>>,
{
	pub fn get_deposits(weight: &mut Weight) -> BTreeMap<AccountIdOf<T>, OldCurrency::Balance> {
		old::DepositOf::<T>::iter()
			.flat_map(|(_prop_index, (accounts, balance))| {
				weight.saturating_accrue(T::WeightInfo::v2_migration_get_deposits(
					accounts.len() as u32
				));
				accounts.into_iter().map(|account| (account, balance)).collect::<Vec<_>>()
			})
			.fold(
				BTreeMap::new(),
				|mut acc: BTreeMap<AccountIdOf<T>, OldCurrency::Balance>, (account, balance)| {
					// TODO: mutate weight
					acc.entry(account.clone())
						.or_insert(Zero::zero())
						.saturating_accrue(balance.into());
					acc
				},
			)
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn bench_store_deposit(depositors: Vec<AccountIdOf<T>>)
	{
		let amount = T::MinimumDeposit::get();
		for depositor in &depositors {
			OldCurrency::reserve(&depositor, amount.into()).expect("Failed to reserve deposit");
		}

		let depositors = BoundedVec::<_, T::MaxDeposits>::truncate_from(depositors);
		old::DepositOf::<T>::insert(0u32, (depositors, amount));
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn bench_store_vote(voter: AccountIdOf<T>, len: u32) {
		use frame_support::traits::WithdrawReasons;
		let balance = 1_000_000u32;
		OldCurrency::set_lock(DEMOCRACY_ID, &voter, balance.into(), WithdrawReasons::except(WithdrawReasons::RESERVE));
		let votes = (0..len).map(|i| {
			(
				i,
				AccountVote::Standard {
					vote: Vote { aye: true, conviction: Conviction::Locked1x },
					balance: balance.into(),
				},
				)
		});
		let votes = BoundedVec::<_, T::MaxVotes>::truncate_from(votes.collect());
		let vote = Voting::Direct { votes, delegations: Default::default(), prior: Default::default() };
		VotingOf::<T>::insert(voter, vote);
	}

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

	pub fn translate_lock_to_freeze(
		account_id: AccountIdOf<T>,
		amount: OldCurrency::Balance,
	) -> Weight {
		OldCurrency::remove_lock(DEMOCRACY_ID, &account_id);
		T::Fungible::extend_freeze(&FreezeReason::Vote.into(), &account_id, amount.into())
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
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// TODO get reserve and lock funds
		todo!()
	}

	#[allow(deprecated)]
	fn on_runtime_upgrade() -> Weight {
		let mut weight: Weight = Weight::zero();

		// convert reserved deposit to held deposit
		Self::get_deposits(&mut weight).into_iter().for_each(|(depositor, amount)| {
			weight.saturating_accrue(Self::translate_reserve_to_hold(&depositor, amount));
		});

		// convert locked deposit to frozen deposit
		old::VotingOf::<T>::iter()
			.map(|(account_id, voting)| (account_id, voting.locked_balance()))
			.for_each(|(account_id, amount)| {
				weight.saturating_accrue(Self::translate_lock_to_freeze(account_id, amount.into()));
			});

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		todo!()
	}
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
	#[test]
	fn migration_works() {
		todo!()
	}
}
