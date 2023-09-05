// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Code that allows `NamedReservableCurrency` to be used as a `StakeAndSlash`
//! mechanism of the relayers pallet.

use bp_relayers::{PayRewardFromAccount, RewardsAccountParams, StakeAndSlash};
use codec::Codec;
use frame_support::traits::{tokens::BalanceStatus, NamedReservableCurrency};
use sp_runtime::{traits::Get, DispatchError, DispatchResult};
use sp_std::{fmt::Debug, marker::PhantomData};

/// `StakeAndSlash` that works with `NamedReservableCurrency` and uses named
/// reservations.
///
/// **WARNING**: this implementation assumes that the relayers pallet is configured to
/// use the [`bp_relayers::PayRewardFromAccount`] as its relayers payment scheme.
pub struct StakeAndSlashNamed<AccountId, BlockNumber, Currency, ReserveId, Stake, Lease>(
	PhantomData<(AccountId, BlockNumber, Currency, ReserveId, Stake, Lease)>,
);

impl<AccountId, BlockNumber, Currency, ReserveId, Stake, Lease>
	StakeAndSlash<AccountId, BlockNumber, Currency::Balance>
	for StakeAndSlashNamed<AccountId, BlockNumber, Currency, ReserveId, Stake, Lease>
where
	AccountId: Codec + Debug,
	Currency: NamedReservableCurrency<AccountId>,
	ReserveId: Get<Currency::ReserveIdentifier>,
	Stake: Get<Currency::Balance>,
	Lease: Get<BlockNumber>,
{
	type RequiredStake = Stake;
	type RequiredRegistrationLease = Lease;

	fn reserve(relayer: &AccountId, amount: Currency::Balance) -> DispatchResult {
		Currency::reserve_named(&ReserveId::get(), relayer, amount)
	}

	fn unreserve(relayer: &AccountId, amount: Currency::Balance) -> Currency::Balance {
		Currency::unreserve_named(&ReserveId::get(), relayer, amount)
	}

	fn repatriate_reserved(
		relayer: &AccountId,
		beneficiary: RewardsAccountParams,
		amount: Currency::Balance,
	) -> Result<Currency::Balance, DispatchError> {
		let beneficiary_account =
			PayRewardFromAccount::<(), AccountId>::rewards_account(beneficiary);
		Currency::repatriate_reserved_named(
			&ReserveId::get(),
			relayer,
			&beneficiary_account,
			amount,
			BalanceStatus::Free,
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	use frame_support::traits::fungible::Mutate;

	fn test_stake() -> Balance {
		Stake::get()
	}

	#[test]
	fn reserve_works() {
		run_test(|| {
			assert!(TestStakeAndSlash::reserve(&1, test_stake()).is_err());
			assert_eq!(Balances::free_balance(1), 0);
			assert_eq!(Balances::reserved_balance(1), 0);

			Balances::mint_into(&2, test_stake() - 1).unwrap();
			assert!(TestStakeAndSlash::reserve(&2, test_stake()).is_err());
			assert_eq!(Balances::free_balance(2), test_stake() - 1);
			assert_eq!(Balances::reserved_balance(2), 0);

			Balances::mint_into(&3, test_stake() * 2).unwrap();
			assert_eq!(TestStakeAndSlash::reserve(&3, test_stake()), Ok(()));
			assert_eq!(Balances::free_balance(3), test_stake());
			assert_eq!(Balances::reserved_balance(3), test_stake());
		})
	}

	#[test]
	fn unreserve_works() {
		run_test(|| {
			assert_eq!(TestStakeAndSlash::unreserve(&1, test_stake()), test_stake());
			assert_eq!(Balances::free_balance(1), 0);
			assert_eq!(Balances::reserved_balance(1), 0);

			Balances::mint_into(&2, test_stake() * 2).unwrap();
			TestStakeAndSlash::reserve(&2, test_stake() / 3).unwrap();
			assert_eq!(
				TestStakeAndSlash::unreserve(&2, test_stake()),
				test_stake() - test_stake() / 3
			);
			assert_eq!(Balances::free_balance(2), test_stake() * 2);
			assert_eq!(Balances::reserved_balance(2), 0);

			Balances::mint_into(&3, test_stake() * 2).unwrap();
			TestStakeAndSlash::reserve(&3, test_stake()).unwrap();
			assert_eq!(TestStakeAndSlash::unreserve(&3, test_stake()), 0);
			assert_eq!(Balances::free_balance(3), test_stake() * 2);
			assert_eq!(Balances::reserved_balance(3), 0);
		})
	}

	#[test]
	fn repatriate_reserved_works() {
		run_test(|| {
			let beneficiary = TEST_REWARDS_ACCOUNT_PARAMS;
			let beneficiary_account = TestPaymentProcedure::rewards_account(beneficiary);

			let mut expected_balance = ExistentialDeposit::get();
			Balances::mint_into(&beneficiary_account, expected_balance).unwrap();

			assert_eq!(
				TestStakeAndSlash::repatriate_reserved(&1, beneficiary, test_stake()),
				Ok(test_stake())
			);
			assert_eq!(Balances::free_balance(1), 0);
			assert_eq!(Balances::reserved_balance(1), 0);
			assert_eq!(Balances::free_balance(beneficiary_account), expected_balance);
			assert_eq!(Balances::reserved_balance(beneficiary_account), 0);

			expected_balance += test_stake() / 3;
			Balances::mint_into(&2, test_stake() * 2).unwrap();
			TestStakeAndSlash::reserve(&2, test_stake() / 3).unwrap();
			assert_eq!(
				TestStakeAndSlash::repatriate_reserved(&2, beneficiary, test_stake()),
				Ok(test_stake() - test_stake() / 3)
			);
			assert_eq!(Balances::free_balance(2), test_stake() * 2 - test_stake() / 3);
			assert_eq!(Balances::reserved_balance(2), 0);
			assert_eq!(Balances::free_balance(beneficiary_account), expected_balance);
			assert_eq!(Balances::reserved_balance(beneficiary_account), 0);

			expected_balance += test_stake();
			Balances::mint_into(&3, test_stake() * 2).unwrap();
			TestStakeAndSlash::reserve(&3, test_stake()).unwrap();
			assert_eq!(
				TestStakeAndSlash::repatriate_reserved(&3, beneficiary, test_stake()),
				Ok(0)
			);
			assert_eq!(Balances::free_balance(3), test_stake());
			assert_eq!(Balances::reserved_balance(3), 0);
			assert_eq!(Balances::free_balance(beneficiary_account), expected_balance);
			assert_eq!(Balances::reserved_balance(beneficiary_account), 0);
		})
	}

	#[test]
	fn repatriate_reserved_doesnt_work_when_beneficiary_account_is_missing() {
		run_test(|| {
			let beneficiary = TEST_REWARDS_ACCOUNT_PARAMS;
			let beneficiary_account = TestPaymentProcedure::rewards_account(beneficiary);

			Balances::mint_into(&3, test_stake() * 2).unwrap();
			TestStakeAndSlash::reserve(&3, test_stake()).unwrap();
			assert!(TestStakeAndSlash::repatriate_reserved(&3, beneficiary, test_stake()).is_err());
			assert_eq!(Balances::free_balance(3), test_stake());
			assert_eq!(Balances::reserved_balance(3), test_stake());
			assert_eq!(Balances::free_balance(beneficiary_account), 0);
			assert_eq!(Balances::reserved_balance(beneficiary_account), 0);
		});
	}
}
