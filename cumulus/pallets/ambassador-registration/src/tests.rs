// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::{mock::*, Error, RegistrationStatus, AMBASSADOR_LOCK_ID, MIN_LOCK_AMOUNT};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::TokenError;

#[test]
fn locking_dot_works() {
	new_test_ext().execute_with(|| {
		// Account 1 locks DOT
		assert_ok!(AmbassadorRegistration::lock_dot(RuntimeOrigin::signed(1)));

		// Check status is updated correctly
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::LockedOnly)
		);

		// Check lock was applied
		assert!(Balances::locks(&1).iter().any(|lock| lock.id == AMBASSADOR_LOCK_ID));

		// Trying to lock again should fail
		assert_noop!(
			AmbassadorRegistration::lock_dot(RuntimeOrigin::signed(1)),
			Error::<Test>::AlreadyLocked
		);
	});
}

#[test]
fn verifying_introduction_works() {
	new_test_ext().execute_with(|| {
		// Verifier (account 100) verifies account 1's introduction
		assert_ok!(AmbassadorRegistration::verify_introduction(
			RuntimeOrigin::signed(100),
			1
		));

		// Check status is updated correctly
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::IntroducedOnly)
		);

		// Trying to verify again should fail
		assert_noop!(
			AmbassadorRegistration::verify_introduction(RuntimeOrigin::signed(100), 1),
			Error::<Test>::AlreadyIntroduced
		);

		// Non-verifier cannot verify introductions
		assert_noop!(
			AmbassadorRegistration::verify_introduction(RuntimeOrigin::signed(2), 1),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn complete_registration_works() {
	new_test_ext().execute_with(|| {
		// Lock DOT
		assert_ok!(AmbassadorRegistration::lock_dot(RuntimeOrigin::signed(1)));
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::LockedOnly)
		);

		// Verify introduction
		assert_ok!(AmbassadorRegistration::verify_introduction(
			RuntimeOrigin::signed(100),
			1
		));

		// Check status is now complete
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::Complete)
		);

		// Check is_registered returns true
		assert!(AmbassadorRegistration::is_registered(&1));
	});
}

#[test]
fn complete_registration_works_reverse_order() {
	new_test_ext().execute_with(|| {
		// Verify introduction
		assert_ok!(AmbassadorRegistration::verify_introduction(
			RuntimeOrigin::signed(100),
			1
		));
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::IntroducedOnly)
		);

		// Lock DOT
		assert_ok!(AmbassadorRegistration::lock_dot(RuntimeOrigin::signed(1)));

		// Check status is now complete
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::Complete)
		);

		// Check is_registered returns true
		assert!(AmbassadorRegistration::is_registered(&1));
	});
}

#[test]
fn removing_registration_works() {
	new_test_ext().execute_with(|| {
		// Complete registration
		assert_ok!(AmbassadorRegistration::verify_introduction(
			RuntimeOrigin::signed(100),
			1
		));
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::IntroducedOnly)
		);

		assert_ok!(AmbassadorRegistration::lock_dot(RuntimeOrigin::signed(1)));

		// Check status is now complete
		assert_eq!(
			AmbassadorRegistration::ambassador_registration_statuses(1),
			Some(RegistrationStatus::Complete)
		);

		// Admin (account 200) removes registration
		assert_ok!(AmbassadorRegistration::remove_registration(
			RuntimeOrigin::signed(200),
			1
		));

		// Check status is removed
		assert_eq!(AmbassadorRegistration::ambassador_registration_statuses(1), None);

		// Check lock is removed
		assert!(!Balances::locks(&1).iter().any(|lock| lock.id == AMBASSADOR_LOCK_ID));

		// Check is_registered returns false
		assert!(!AmbassadorRegistration::is_registered(&1));

		// Trying to remove again should fail
		assert_noop!(
			AmbassadorRegistration::remove_registration(RuntimeOrigin::signed(200), 1),
			Error::<Test>::NotRegistered
		);

		// Non-admin cannot remove registrations
		assert_noop!(
			AmbassadorRegistration::remove_registration(RuntimeOrigin::signed(2), 1),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn lock_prevents_transfers() {
	new_test_ext().execute_with(|| {
		// Account 1 locks DOT
		assert_ok!(AmbassadorRegistration::lock_dot(RuntimeOrigin::signed(1)));

		// Check the lock was applied
		assert!(Balances::locks(&1).iter().any(|lock| lock.id == AMBASSADOR_LOCK_ID));

		// Try transfer more than the unlocked balance
		// Account 1 has 100 units, 1 DOT is locked, so transferring 100 should fail
		assert_noop!(
			Balances::transfer_allow_death(RuntimeOrigin::signed(1), 2, 100),
			TokenError::Frozen
		);

		// Can still transfer less than the total balance minus the locked amount
		let lock_amount: u64 = MIN_LOCK_AMOUNT.into();
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(1), 2, 99 - lock_amount));
	});
}
