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

use crate::{migrations::MigrationIdentifier, *};
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	storage_alias,
	traits::{LockableCurrency, ReservableCurrency},
	weights::WeightMeter,
	BoundedVec, DefaultNoBound,
};
use frame_system::pallet_prelude::BlockNumberFor;

/// The log target.
const LOG_TARGET: &'static str = "runtime::democracy::migration::v2";

/// Type alias for `frame_system`'s account id.
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Type alias for `democracy`'s fungible type.
pub type FungibleOf<T> = <T as pallet::Config>::Fungible;

/// Type alias for `democracy`'s MaxDeposits.
pub type MaxDepositsOf<T> = <T as Config>::MaxDeposits;

/// Type alias for `democracy`'s MaxVotes.
pub type MaxVotesOf<T> = <T as Config>::MaxDeposits;

pub mod old {
	use super::*;

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

#[derive(Encode, Decode, MaxEncodedLen, DefaultNoBound)]
pub enum Cursor<T: Config> {
	/// The index of the last deposit that has been migrated.
	#[default]
	Deposit(Option<PropIndex>),
	/// The last vote's account that has been migrated.
	Vote(Option<AccountIdOf<T>>),
}

pub struct Migration<T: Config, OldCurrency>(PhantomData<(T, OldCurrency)>);

impl<T, OldCurrency> SteppedMigration for Migration<T, OldCurrency>
where
	T: Config,
	OldCurrency: 'static
		+ ReservableCurrency<AccountIdOf<T>>
		+ LockableCurrency<AccountIdOf<T>, Moment = BlockNumberFor<T>>,
	OldCurrency::Balance: IsType<BalanceOf<T>>,
{
	type Cursor = Cursor<T>;
	type Identifier = MigrationIdentifier;

	fn id() -> Self::Identifier {
		MigrationIdentifier::new(2)
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		match cursor.unwrap_or_default() {
			Cursor::Deposit(index) => Ok(Some(Self::deposit_step(index, meter)?)),
			Cursor::Vote(account) => Self::vote_step(account, meter),
		}
	}
}

impl<T, OldCurrency> Migration<T, OldCurrency>
where
	T: Config,
	OldCurrency: 'static
		+ ReservableCurrency<AccountIdOf<T>>
		+ LockableCurrency<AccountIdOf<T>, Moment = BlockNumberFor<T>>,
	OldCurrency::Balance: IsType<BalanceOf<T>>,
{
	/// Execute one deposit reserve to hold migration step.
	pub fn deposit_step(
		index: Option<PropIndex>,
		meter: &mut WeightMeter,
	) -> Result<Cursor<T>, SteppedMigrationError> {
		// Check there are enough weight to proceed.
		let required = T::WeightInfo::v2_migrate_deposit(MaxDepositsOf::<T>::get());
		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required })
		}

		// Iterate to next deposit.
		let next_deposit = (if let Some(index) = index {
			old::DepositOf::<T>::iter_from(old::DepositOf::<T>::hashed_key_for(index))
		} else {
			old::DepositOf::<T>::iter()
		})
		.next();

		// Translate reserved deposit to held deposit.
		if let Some((index, (accounts, amount))) = next_deposit {
			meter.consume(T::WeightInfo::v2_migrate_deposit(accounts.len() as _));
			accounts
				.into_iter()
				.for_each(|depositor| Self::translate_reserve_to_hold(&depositor, amount.into()));
			Ok(Cursor::Deposit(Some(index)))
		} else {
			meter.consume(T::WeightInfo::v2_migrate_deposit(0));
			Ok(Cursor::Vote(None))
		}
	}

	/// Execute one vote lock to freeze migration step.
	pub fn vote_step(
		index: Option<AccountIdOf<T>>,
		meter: &mut WeightMeter,
	) -> Result<Option<Cursor<T>>, SteppedMigrationError> {
		// Check there are enough weight to proceed.
		let required = T::WeightInfo::v2_migrate_vote(1);
		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required })
		}

		// Iterate to next vote.
		let next_vote = (if let Some(index) = index {
			old::VotingOf::<T>::iter_from(old::VotingOf::<T>::hashed_key_for(index))
		} else {
			old::VotingOf::<T>::iter()
		})
		.next();

		// Translate locked deposit to frozen deposit.
		if let Some((account_id, voting)) = next_vote {
			meter.consume(T::WeightInfo::v2_migrate_vote(1));
			let balance = voting.locked_balance().into();
			Self::translate_lock_to_freeze(&account_id, balance);
			Ok(Some(Cursor::Vote(Some(account_id))))
		} else {
			meter.consume(T::WeightInfo::v2_migrate_vote(0));
			Ok(None)
		}
	}

	/// Store proposal deposits for benchmarking purposes.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub fn bench_store_deposit(prop_index: PropIndex, depositors: Vec<AccountIdOf<T>>) {
		let amount = T::MinimumDeposit::get();
		for depositor in &depositors {
			OldCurrency::reserve(&depositor, amount.into()).expect("Failed to reserve deposit");
		}

		let depositors = BoundedVec::<_, T::MaxDeposits>::truncate_from(depositors);
		old::DepositOf::<T>::insert(prop_index, (depositors, amount));
	}

	/// Translate reserved deposit to held deposit.
	pub fn translate_reserve_to_hold(depositor: &AccountIdOf<T>, amount: OldCurrency::Balance) {
		let remaining = OldCurrency::unreserve(&depositor, amount);
		if remaining > Zero::zero() {
			log::warn!(
			target: LOG_TARGET,
			"account {depositor:?} has some non-unreservable deposit {remaining:?} from a total of {amount:?}
			that will remain in reserved.",
			);
		}

		let amount = amount.saturating_sub(remaining);

		log::debug!(
			target: LOG_TARGET,
			"Holding {amount:?} on account {depositor:?}.",
		);

		T::Fungible::hold(&HoldReason::Proposal.into(), &depositor, amount.into()).unwrap_or_else(
			|err| {
				log::error!(
					target: LOG_TARGET,
					"Failed to hold {amount:?} from account {depositor:?}, reason: {err:?}.",
				);
			},
		);
	}

	/// Store votes for benchmarking purposes.
	#[cfg(any(feature = "runtime-benchmarks", test))]
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
	pub fn translate_lock_to_freeze(account_id: &AccountIdOf<T>, amount: OldCurrency::Balance) {
		OldCurrency::remove_lock(DEMOCRACY_ID, account_id);
		T::Fungible::set_freeze(&FreezeReason::Vote.into(), account_id, amount.into())
			.unwrap_or_else(|err| {
				log::error!(
					target: LOG_TARGET,
					"Failed to freeze {amount:?} from account {account_id:?}, reason: {err:?}.",
				);
			});
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::tests::{Test as T, *};
	use frame_support::traits::fungible::InspectHold;

	type MigrationOf<T> = Migration<T, pallet_balances::Pallet<T>>;

	#[test]
	fn migration_works() {
		new_test_ext().execute_with(|| {
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 0);
			let alice = 1;

			// Store a proposal deposit and vote for alice.
			MigrationOf::<T>::bench_store_deposit(0u32, vec![alice]);
			MigrationOf::<T>::bench_store_vote(alice.into());

			// Check that alice's deposit is reserved and vote balance is locked.
			assert_eq!(pallet_balances::Pallet::<T>::reserved_balance(&alice), 1);
			assert_eq!(pallet_balances::Pallet::<T>::locks(&alice)[0].amount, 1_000_000);

			// Run migration.
			let mut cursor = None;
			let mut meter = WeightMeter::new();
			while let Ok(Some(next_cursor)) = MigrationOf::<T>::step(cursor, &mut meter) {
				cursor = Some(next_cursor);
			}

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
