// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for the purchase pallet.

#[cfg(test)]
use super::*;

use sp_core::crypto::AccountId32;
// The testing primitives are very useful for avoiding having to work with signatures
// or public keys. `u64` is used as the `AccountId` and no `Signature`s are required.
use frame_support::{assert_noop, assert_ok, traits::Currency};
use sp_runtime::{traits::Dispatchable, ArithmeticError, DispatchError::BadOrigin};

use crate::purchase::mock::*;

#[test]
fn set_statement_works_and_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		let statement = b"Test Set Statement".to_vec();
		// Invalid origin
		assert_noop!(
			Purchase::set_statement(RuntimeOrigin::signed(alice()), statement.clone()),
			BadOrigin,
		);
		// Too Long
		let long_statement = [0u8; 10_000].to_vec();
		assert_noop!(
			Purchase::set_statement(RuntimeOrigin::signed(configuration_origin()), long_statement),
			Error::<Test>::InvalidStatement,
		);
		// Just right...
		assert_ok!(Purchase::set_statement(
			RuntimeOrigin::signed(configuration_origin()),
			statement.clone()
		));
		assert_eq!(Statement::<Test>::get(), statement);
	});
}

#[test]
fn set_unlock_block_works_and_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		let unlock_block = 69;
		// Invalid origin
		assert_noop!(
			Purchase::set_unlock_block(RuntimeOrigin::signed(alice()), unlock_block),
			BadOrigin,
		);
		// Block Number in Past
		let bad_unlock_block = 50;
		System::set_block_number(bad_unlock_block);
		assert_noop!(
			Purchase::set_unlock_block(
				RuntimeOrigin::signed(configuration_origin()),
				bad_unlock_block
			),
			Error::<Test>::InvalidUnlockBlock,
		);
		// Just right...
		assert_ok!(Purchase::set_unlock_block(
			RuntimeOrigin::signed(configuration_origin()),
			unlock_block
		));
		assert_eq!(UnlockBlock::<Test>::get(), unlock_block);
	});
}

#[test]
fn set_payment_account_works_and_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		let payment_account: AccountId32 = [69u8; 32].into();
		// Invalid Origin
		assert_noop!(
			Purchase::set_payment_account(RuntimeOrigin::signed(alice()), payment_account.clone()),
			BadOrigin,
		);
		// Just right...
		assert_ok!(Purchase::set_payment_account(
			RuntimeOrigin::signed(configuration_origin()),
			payment_account.clone()
		));
		assert_eq!(PaymentAccount::<Test>::get(), Some(payment_account));
	});
}

#[test]
fn signature_verification_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Purchase::verify_signature(&alice(), &alice_signature()));
		assert_ok!(Purchase::verify_signature(&alice_ed25519(), &alice_signature_ed25519()));
		assert_ok!(Purchase::verify_signature(&bob(), &bob_signature()));

		// Mixing and matching fails
		assert_noop!(
			Purchase::verify_signature(&alice(), &bob_signature()),
			Error::<Test>::InvalidSignature
		);
		assert_noop!(
			Purchase::verify_signature(&bob(), &alice_signature()),
			Error::<Test>::InvalidSignature
		);
	});
}

#[test]
fn account_creation_works() {
	new_test_ext().execute_with(|| {
		assert!(!Accounts::<Test>::contains_key(alice()));
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			alice_signature().to_vec(),
		));
		assert_eq!(
			Accounts::<Test>::get(alice()),
			AccountStatus {
				validity: AccountValidity::Initiated,
				free_balance: Zero::zero(),
				locked_balance: Zero::zero(),
				signature: alice_signature().to_vec(),
				vat: Permill::zero(),
			}
		);
	});
}

#[test]
fn account_creation_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Wrong Origin
		assert_noop!(
			Purchase::create_account(
				RuntimeOrigin::signed(alice()),
				alice(),
				alice_signature().to_vec()
			),
			BadOrigin,
		);

		// Wrong Account/Signature
		assert_noop!(
			Purchase::create_account(
				RuntimeOrigin::signed(validity_origin()),
				alice(),
				bob_signature().to_vec()
			),
			Error::<Test>::InvalidSignature,
		);

		// Account with vesting
		Balances::make_free_balance_be(&alice(), 100);
		assert_ok!(<Test as Config>::VestingSchedule::add_vesting_schedule(&alice(), 100, 1, 50));
		assert_noop!(
			Purchase::create_account(
				RuntimeOrigin::signed(validity_origin()),
				alice(),
				alice_signature().to_vec()
			),
			Error::<Test>::VestingScheduleExists,
		);

		// Duplicate Purchasing Account
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			bob(),
			bob_signature().to_vec()
		));
		assert_noop!(
			Purchase::create_account(
				RuntimeOrigin::signed(validity_origin()),
				bob(),
				bob_signature().to_vec()
			),
			Error::<Test>::ExistingAccount,
		);
	});
}

#[test]
fn update_validity_status_works() {
	new_test_ext().execute_with(|| {
		// Alice account is created.
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			alice_signature().to_vec(),
		));
		// She submits KYC, and we update the status to `Pending`.
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::Pending,
		));
		// KYC comes back negative, so we mark the account invalid.
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::Invalid,
		));
		assert_eq!(
			Accounts::<Test>::get(alice()),
			AccountStatus {
				validity: AccountValidity::Invalid,
				free_balance: Zero::zero(),
				locked_balance: Zero::zero(),
				signature: alice_signature().to_vec(),
				vat: Permill::zero(),
			}
		);
		// She fixes it, we mark her account valid.
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::ValidLow,
		));
		assert_eq!(
			Accounts::<Test>::get(alice()),
			AccountStatus {
				validity: AccountValidity::ValidLow,
				free_balance: Zero::zero(),
				locked_balance: Zero::zero(),
				signature: alice_signature().to_vec(),
				vat: Permill::zero(),
			}
		);
	});
}

#[test]
fn update_validity_status_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Wrong Origin
		assert_noop!(
			Purchase::update_validity_status(
				RuntimeOrigin::signed(alice()),
				alice(),
				AccountValidity::Pending,
			),
			BadOrigin
		);
		// Inactive Account
		assert_noop!(
			Purchase::update_validity_status(
				RuntimeOrigin::signed(validity_origin()),
				alice(),
				AccountValidity::Pending,
			),
			Error::<Test>::InvalidAccount
		);
		// Already Completed
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			alice_signature().to_vec(),
		));
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::Completed,
		));
		assert_noop!(
			Purchase::update_validity_status(
				RuntimeOrigin::signed(validity_origin()),
				alice(),
				AccountValidity::Pending,
			),
			Error::<Test>::AlreadyCompleted
		);
	});
}

#[test]
fn update_balance_works() {
	new_test_ext().execute_with(|| {
		// Alice account is created
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			alice_signature().to_vec()
		));
		// And approved for basic contribution
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::ValidLow,
		));
		// We set a balance on the user based on the payment they made. 50 locked, 50 free.
		assert_ok!(Purchase::update_balance(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			50,
			50,
			Permill::from_rational(77u32, 1000u32),
		));
		assert_eq!(
			Accounts::<Test>::get(alice()),
			AccountStatus {
				validity: AccountValidity::ValidLow,
				free_balance: 50,
				locked_balance: 50,
				signature: alice_signature().to_vec(),
				vat: Permill::from_parts(77000),
			}
		);
		// We can update the balance based on new information.
		assert_ok!(Purchase::update_balance(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			25,
			50,
			Permill::zero(),
		));
		assert_eq!(
			Accounts::<Test>::get(alice()),
			AccountStatus {
				validity: AccountValidity::ValidLow,
				free_balance: 25,
				locked_balance: 50,
				signature: alice_signature().to_vec(),
				vat: Permill::zero(),
			}
		);
	});
}

#[test]
fn update_balance_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Wrong Origin
		assert_noop!(
			Purchase::update_balance(
				RuntimeOrigin::signed(alice()),
				alice(),
				50,
				50,
				Permill::zero(),
			),
			BadOrigin
		);
		// Inactive Account
		assert_noop!(
			Purchase::update_balance(
				RuntimeOrigin::signed(validity_origin()),
				alice(),
				50,
				50,
				Permill::zero(),
			),
			Error::<Test>::InvalidAccount
		);
		// Overflow
		assert_noop!(
			Purchase::update_balance(
				RuntimeOrigin::signed(validity_origin()),
				alice(),
				u64::MAX,
				u64::MAX,
				Permill::zero(),
			),
			Error::<Test>::InvalidAccount
		);
	});
}

#[test]
fn payout_works() {
	new_test_ext().execute_with(|| {
		// Alice and Bob accounts are created
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			alice_signature().to_vec()
		));
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			bob(),
			bob_signature().to_vec()
		));
		// Alice is approved for basic contribution
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::ValidLow,
		));
		// Bob is approved for high contribution
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			bob(),
			AccountValidity::ValidHigh,
		));
		// We set a balance on the users based on the payment they made. 50 locked, 50 free.
		assert_ok!(Purchase::update_balance(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			50,
			50,
			Permill::zero(),
		));
		assert_ok!(Purchase::update_balance(
			RuntimeOrigin::signed(validity_origin()),
			bob(),
			100,
			150,
			Permill::zero(),
		));
		// Now we call payout for Alice and Bob.
		assert_ok!(Purchase::payout(RuntimeOrigin::signed(payment_account()), alice(),));
		assert_ok!(Purchase::payout(RuntimeOrigin::signed(payment_account()), bob(),));
		// Payment is made.
		assert_eq!(<Test as Config>::Currency::free_balance(&payment_account()), 99_650);
		assert_eq!(<Test as Config>::Currency::free_balance(&alice()), 100);
		// 10% of the 50 units is unlocked automatically for Alice
		assert_eq!(<Test as Config>::VestingSchedule::vesting_balance(&alice()), Some(45));
		assert_eq!(<Test as Config>::Currency::free_balance(&bob()), 250);
		// A max of 10 units is unlocked automatically for Bob
		assert_eq!(<Test as Config>::VestingSchedule::vesting_balance(&bob()), Some(140));
		// Status is completed.
		assert_eq!(
			Accounts::<Test>::get(alice()),
			AccountStatus {
				validity: AccountValidity::Completed,
				free_balance: 50,
				locked_balance: 50,
				signature: alice_signature().to_vec(),
				vat: Permill::zero(),
			}
		);
		assert_eq!(
			Accounts::<Test>::get(bob()),
			AccountStatus {
				validity: AccountValidity::Completed,
				free_balance: 100,
				locked_balance: 150,
				signature: bob_signature().to_vec(),
				vat: Permill::zero(),
			}
		);
		// Vesting lock is removed in whole on block 101 (100 blocks after block 1)
		System::set_block_number(100);
		let vest_call = RuntimeCall::Vesting(pallet_vesting::Call::<Test>::vest {});
		assert_ok!(vest_call.clone().dispatch(RuntimeOrigin::signed(alice())));
		assert_ok!(vest_call.clone().dispatch(RuntimeOrigin::signed(bob())));
		assert_eq!(<Test as Config>::VestingSchedule::vesting_balance(&alice()), Some(45));
		assert_eq!(<Test as Config>::VestingSchedule::vesting_balance(&bob()), Some(140));
		System::set_block_number(101);
		assert_ok!(vest_call.clone().dispatch(RuntimeOrigin::signed(alice())));
		assert_ok!(vest_call.clone().dispatch(RuntimeOrigin::signed(bob())));
		assert_eq!(<Test as Config>::VestingSchedule::vesting_balance(&alice()), None);
		assert_eq!(<Test as Config>::VestingSchedule::vesting_balance(&bob()), None);
	});
}

#[test]
fn payout_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Wrong Origin
		assert_noop!(Purchase::payout(RuntimeOrigin::signed(alice()), alice(),), BadOrigin);
		// Account with Existing Vesting Schedule
		Balances::make_free_balance_be(&bob(), 100);
		assert_ok!(<Test as Config>::VestingSchedule::add_vesting_schedule(&bob(), 100, 1, 50,));
		assert_noop!(
			Purchase::payout(RuntimeOrigin::signed(payment_account()), bob(),),
			Error::<Test>::VestingScheduleExists
		);
		// Invalid Account (never created)
		assert_noop!(
			Purchase::payout(RuntimeOrigin::signed(payment_account()), alice(),),
			Error::<Test>::InvalidAccount
		);
		// Invalid Account (created, but not valid)
		assert_ok!(Purchase::create_account(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			alice_signature().to_vec()
		));
		assert_noop!(
			Purchase::payout(RuntimeOrigin::signed(payment_account()), alice(),),
			Error::<Test>::InvalidAccount
		);
		// Not enough funds in payment account
		assert_ok!(Purchase::update_validity_status(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			AccountValidity::ValidHigh,
		));
		assert_ok!(Purchase::update_balance(
			RuntimeOrigin::signed(validity_origin()),
			alice(),
			100_000,
			100_000,
			Permill::zero(),
		));
		assert_noop!(
			Purchase::payout(RuntimeOrigin::signed(payment_account()), alice()),
			ArithmeticError::Underflow
		);
	});
}

#[test]
fn remove_pallet_works() {
	new_test_ext().execute_with(|| {
		let account_status = AccountStatus {
			validity: AccountValidity::Completed,
			free_balance: 1234,
			locked_balance: 4321,
			signature: b"my signature".to_vec(),
			vat: Permill::from_percent(50),
		};

		// Add some storage.
		Accounts::<Test>::insert(alice(), account_status.clone());
		Accounts::<Test>::insert(bob(), account_status);
		PaymentAccount::<Test>::put(alice());
		Statement::<Test>::put(b"hello, world!".to_vec());
		UnlockBlock::<Test>::put(4);

		// Verify storage exists.
		assert_eq!(Accounts::<Test>::iter().count(), 2);
		assert!(PaymentAccount::<Test>::exists());
		assert!(Statement::<Test>::exists());
		assert!(UnlockBlock::<Test>::exists());

		// Remove storage.
		remove_pallet::<Test>();

		// Verify storage is gone.
		assert_eq!(Accounts::<Test>::iter().count(), 0);
		assert!(!PaymentAccount::<Test>::exists());
		assert!(!Statement::<Test>::exists());
		assert!(!UnlockBlock::<Test>::exists());
	});
}
