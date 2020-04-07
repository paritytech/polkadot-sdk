// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Balances pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_system::RawOrigin;
use frame_benchmarking::{benchmarks, account};
use sp_runtime::traits::Bounded;

use crate::Module as Balances;

const SEED: u32 = 0;
const MAX_EXISTENTIAL_DEPOSIT: u32 = 1000;
const MAX_USER_INDEX: u32 = 1000;

benchmarks! {
	_ {
		let e in 2 .. MAX_EXISTENTIAL_DEPOSIT => ();
		let u in 1 .. MAX_USER_INDEX => ();
	}

	// Benchmark `transfer` extrinsic with the worst possible conditions:
	// * Transfer will kill the sender account.
	// * Transfer will create the recipient account.
	transfer {
		let u in ...;
		let e in ...;

		let existential_deposit = T::ExistentialDeposit::get();
		let caller = account("caller", u, SEED);

		// Give some multiple of the existential deposit + creation fee + transfer fee
		let balance = existential_deposit.saturating_mul(e.into());
		let _ = <Balances<T> as Currency<_>>::make_free_balance_be(&caller, balance);

		// Transfer `e - 1` existential deposits + 1 unit, which guarantees to create one account, and reap this user.
		let recipient = account("recipient", u, SEED);
		let recipient_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(recipient);
		let transfer_amount = existential_deposit.saturating_mul((e - 1).into()) + 1.into();
	}: _(RawOrigin::Signed(caller), recipient_lookup, transfer_amount)

	// Benchmark `transfer` with the best possible condition:
	// * Both accounts exist and will continue to exist.
	transfer_best_case {
		let u in ...;
		let e in ...;

		let caller = account("caller", u, SEED);
		let recipient: T::AccountId = account("recipient", u, SEED);
		let recipient_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(recipient.clone());

		// Give the sender account max funds for transfer (their account will never reasonably be killed).
		let _ = <Balances<T> as Currency<_>>::make_free_balance_be(&caller, T::Balance::max_value());

		// Give the recipient account existential deposit (thus their account already exists).
		let existential_deposit = T::ExistentialDeposit::get();
		let _ = <Balances<T> as Currency<_>>::make_free_balance_be(&recipient, existential_deposit);
		let transfer_amount = existential_deposit.saturating_mul(e.into());
	}: transfer(RawOrigin::Signed(caller), recipient_lookup, transfer_amount)

	// Benchmark `transfer_keep_alive` with the worst possible condition:
	// * The recipient account is created.
	transfer_keep_alive {
		let u in ...;
		let e in ...;

		let caller = account("caller", u, SEED);
		let recipient = account("recipient", u, SEED);
		let recipient_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(recipient);

		// Give the sender account max funds, thus a transfer will not kill account.
		let _ = <Balances<T> as Currency<_>>::make_free_balance_be(&caller, T::Balance::max_value());
		let existential_deposit = T::ExistentialDeposit::get();
		let transfer_amount = existential_deposit.saturating_mul(e.into());
	}: _(RawOrigin::Signed(caller), recipient_lookup, transfer_amount)

	// Benchmark `set_balance` coming from ROOT account. This always creates an account.
	set_balance {
		let u in ...;
		let e in ...;

		let user: T::AccountId = account("user", u, SEED);
		let user_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(user.clone());

		// Give the user some initial balance.
		let existential_deposit = T::ExistentialDeposit::get();
		let balance_amount = existential_deposit.saturating_mul(e.into());
		let _ = <Balances<T> as Currency<_>>::make_free_balance_be(&user, balance_amount);
	}: _(RawOrigin::Root, user_lookup, balance_amount, balance_amount)

	// Benchmark `set_balance` coming from ROOT account. This always kills an account.
	set_balance_killing {
		let u in ...;
		let e in ...;

		let user: T::AccountId = account("user", u, SEED);
		let user_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(user.clone());

		// Give the user some initial balance.
		let existential_deposit = T::ExistentialDeposit::get();
		let balance_amount = existential_deposit.saturating_mul(e.into());
		let _ = <Balances<T> as Currency<_>>::make_free_balance_be(&user, balance_amount);
	}: set_balance(RawOrigin::Root, user_lookup, 0.into(), 0.into())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests_composite::{ExtBuilder, Test};
	use frame_support::assert_ok;

	#[test]
	fn transfer() {
		ExtBuilder::default().build().execute_with(|| {
			assert_ok!(test_benchmark_transfer::<Test>());
		});
	}

	#[test]
	fn transfer_best_case() {
		ExtBuilder::default().build().execute_with(|| {
			assert_ok!(test_benchmark_transfer_best_case::<Test>());
		});
	}

	#[test]
	fn transfer_keep_alive() {
		ExtBuilder::default().build().execute_with(|| {
			assert_ok!(test_benchmark_transfer_keep_alive::<Test>());
		});
	}

	#[test]
	fn transfer_set_balance() {
		ExtBuilder::default().build().execute_with(|| {
			assert_ok!(test_benchmark_set_balance::<Test>());
		});
	}

	#[test]
	fn transfer_set_balance_killing() {
		ExtBuilder::default().build().execute_with(|| {
			assert_ok!(test_benchmark_set_balance_killing::<Test>());
		});
	}
}
