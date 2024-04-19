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

//! Tests for the ledger recovery.

use super::*;
use frame_support::traits::InspectLockableCurrency;

#[test]
fn inspect_recovery_ledger_simple_works() {
	ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
		setup_double_bonded_ledgers();

		// non corrupted ledger.
		assert_eq!(Staking::inspect_bond_state(&11).unwrap(), LedgerIntegrityState::Ok);

		// non bonded stash.
		assert!(Bonded::<Test>::get(&1111).is_none());
		assert!(Staking::inspect_bond_state(&1111).is_err());

		// double bonded but not corrupted.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
	})
}

#[test]
fn inspect_recovery_ledger_corupted_killed_works() {
	ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
		setup_double_bonded_ledgers();

		let lock_333_before = Balances::balance_locked(crate::STAKING_ID, &333);

		// get into corrupted and killed ledger state by killing a corrupted ledger:
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		// kill(333)
		// (444, 444) -> corrupted and None.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);

		// now try-state fails.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// 333 is corrupted since it's controller is linking 444 ledger.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
		// 444 however is OK.
		assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::Ok);

		// kill the corrupted ledger that is associated with stash 333.
		assert_ok!(StakingLedger::<Test>::kill(&333));

		// 333 bond is no more but it returns `BadState` because the lock on this stash is
		// still set (see checks below).
		assert_eq!(Staking::inspect_bond_state(&333), Err(Error::<Test>::BadState));
		// now the *other* ledger associated with 444 has been corrupted and killed (None).
		assert_eq!(Staking::inspect_bond_state(&444), Ok(LedgerIntegrityState::CorruptedKilled));

		// side effects on 333 - ledger, bonded, payee, lock should be completely empty.
		// however, 333 lock remains.
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &333), lock_333_before); // NOK
		assert!(Bonded::<Test>::get(&333).is_none()); // OK
		assert!(Payee::<Test>::get(&333).is_none()); // OK
		assert!(Ledger::<Test>::get(&444).is_none()); // OK

		// side effects on 444 - ledger, bonded, payee, lock should remain be intact.
		// however, 444 lock was removed.
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &444), 0); // NOK
		assert!(Bonded::<Test>::get(&444).is_some()); // OK
		assert!(Payee::<Test>::get(&444).is_some()); // OK
		assert!(Ledger::<Test>::get(&555).is_none()); // NOK

		assert!(Staking::do_try_state(System::block_number()).is_err());
	})
}

#[test]
fn inspect_recovery_ledger_corupted_killed_other_works() {
	ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
		setup_double_bonded_ledgers();

		let lock_333_before = Balances::balance_locked(crate::STAKING_ID, &333);

		// get into corrupted and killed ledger state by killing a corrupted ledger:
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		// kill(444)
		// (333, 444) -> corrupted and None
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);

		// now try-state fails.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// 333 is corrupted since it's controller is linking 444 ledger.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
		// 444 however is OK.
		assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::Ok);

		// kill the *other* ledger that is double bonded but not corrupted.
		assert_ok!(StakingLedger::<Test>::kill(&444));

		// now 333 is corrupted and None through the *other* ledger being killed.
		assert_eq!(
			Staking::inspect_bond_state(&333).unwrap(),
			LedgerIntegrityState::CorruptedKilled,
		);
		// 444 is cleaned and not a stash anymore; no lock left behind.
		assert_eq!(Ledger::<Test>::get(&444), None);
		assert_eq!(Staking::inspect_bond_state(&444), Err(Error::<Test>::NotStash));

		// side effects on 333 - ledger, bonded, payee, lock should be intact.
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &333), lock_333_before); // OK
		assert_eq!(Bonded::<Test>::get(&333), Some(444)); // OK
		assert!(Payee::<Test>::get(&333).is_some()); // OK
											 // however, ledger associated with its controller was killed.
		assert!(Ledger::<Test>::get(&444).is_none()); // NOK

		// side effects on 444 - ledger, bonded, payee, lock should be completely removed.
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &444), 0); // OK
		assert!(Bonded::<Test>::get(&444).is_none()); // OK
		assert!(Payee::<Test>::get(&444).is_none()); // OK
		assert!(Ledger::<Test>::get(&555).is_none()); // OK

		assert!(Staking::do_try_state(System::block_number()).is_err());
	})
}

#[test]
fn inspect_recovery_ledger_lock_corrupted_works() {
	ExtBuilder::default().has_stakers(true).try_state(false).build_and_execute(|| {
		setup_double_bonded_ledgers();

		// get into lock corrupted ledger state by bond_extra on a ledger that is double bonded
		// with a corrupted ledger.
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		//  bond_extra(333, 10) -> lock corrupted on 444
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);
		bond_extra_no_checks(&333, 10);

		// now try-state fails.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// 333 is corrupted since it's controller is linking 444 ledger.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
		// 444 ledger is not corrupted but locks got out of sync.
		assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::LockCorrupted);
	})
}

// Corrupted ledger restore.
//
// * Double bonded and corrupted ledger.
#[test]
fn restore_ledger_corrupted_works() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		setup_double_bonded_ledgers();

		// get into corrupted and killed ledger state.
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);

		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);

		// now try-state fails.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// recover the ledger bonded by 333 stash.
		assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));

		// try-state checks are ok now.
		assert_ok!(Staking::do_try_state(System::block_number()));
	})
}

// Corrupted and killed ledger restore.
//
// * Double bonded and corrupted ledger.
// * Ledger killed by own controller.
#[test]
fn restore_ledger_corrupted_killed_works() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		setup_double_bonded_ledgers();

		// ledger.total == lock
		let total_444_before_corruption = Balances::balance_locked(crate::STAKING_ID, &444);

		// get into corrupted and killed ledger state by killing a corrupted ledger:
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		// kill(333)
		// (444, 444) -> corrupted and None.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);

		// kill the corrupted ledger that is associated with stash 333.
		assert_ok!(StakingLedger::<Test>::kill(&333));

		// 333 bond is no more but it returns `BadState` because the lock on this stash is
		// still set (see checks below).
		assert_eq!(Staking::inspect_bond_state(&333), Err(Error::<Test>::BadState));
		// now the *other* ledger associated with 444 has been corrupted and killed (None).
		assert!(Staking::ledger(StakingAccount::Stash(444)).is_err());

		// try-state should fail.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// recover the ledger bonded by 333 stash.
		assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));

		// for the try-state checks to pass, we also need to recover the stash 444 which is
		// corrupted too by proxy of kill(333). Currently, both the lock and the ledger of 444
		// have been cleared so we need to provide the new amount to restore the ledger.
		assert_noop!(
			Staking::restore_ledger(RuntimeOrigin::root(), 444, None, None, None),
			Error::<Test>::CannotRestoreLedger
		);

		assert_ok!(Staking::restore_ledger(
			RuntimeOrigin::root(),
			444,
			None,
			Some(total_444_before_corruption),
			None,
		));

		// try-state checks are ok now.
		assert_ok!(Staking::do_try_state(System::block_number()));
	})
}

// Corrupted and killed by *other* ledger restore.
//
// * Double bonded and corrupted ledger.
// * Ledger killed by own controller.
#[test]
fn restore_ledger_corrupted_killed_other_works() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		setup_double_bonded_ledgers();

		// get into corrupted and killed ledger state by killing a corrupted ledger:
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		// kill(444)
		// (333, 444) -> corrupted and None
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);

		// now try-state fails.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// 333 is corrupted since it's controller is linking 444 ledger.
		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Corrupted);
		// 444 however is OK.
		assert_eq!(Staking::inspect_bond_state(&444).unwrap(), LedgerIntegrityState::Ok);

		// kill the *other* ledger that is double bonded but not corrupted.
		assert_ok!(StakingLedger::<Test>::kill(&444));

		// recover the ledger bonded by 333 stash.
		assert_ok!(Staking::restore_ledger(RuntimeOrigin::root(), 333, None, None, None));

		// 444 does not need recover in this case since it's been killed successfully.
		assert_eq!(Staking::inspect_bond_state(&444), Err(Error::<Test>::NotStash));

		// try-state checks are ok now.
		assert_ok!(Staking::do_try_state(System::block_number()));
	})
}

// Corrupted with bond_extra.
//
// * Double bonded and corrupted ledger.
// * Corrupted ledger calls `bond_extra`
#[test]
fn restore_ledger_corrupted_bond_extra_works() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		setup_double_bonded_ledgers();

		let lock_333_before = Balances::balance_locked(crate::STAKING_ID, &333);
		let lock_444_before = Balances::balance_locked(crate::STAKING_ID, &444);

		// get into corrupted and killed ledger state by killing a corrupted ledger:
		// init state:
		//  (333, 444)
		//  (444, 555)
		// set_controller(444) to 444
		//  (333, 444) -> corrupted
		//  (444, 444)
		// bond_extra(444, 40) -> OK
		// bond_extra(333, 30) -> locks out of sync

		assert_eq!(Staking::inspect_bond_state(&333).unwrap(), LedgerIntegrityState::Ok);
		set_controller_no_checks(&444);

		// now try-state fails.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// if 444 bonds extra, the locks remain in sync.
		bond_extra_no_checks(&444, 40);
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &333), lock_333_before);
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &444), lock_444_before + 40);

		// however if 333 bonds extra, the wrong lock is updated.
		bond_extra_no_checks(&333, 30);
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &333), lock_444_before + 40 + 30); //not OK
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &444), lock_444_before + 40); // OK

		// recover the ledger bonded by 333 stash. Note that the total/lock needs to be
		// re-written since on-chain data lock has become out of sync.
		assert_ok!(Staking::restore_ledger(
			RuntimeOrigin::root(),
			333,
			None,
			Some(lock_333_before + 30),
			None
		));

		// now recover 444 that although it's not corrupted, its lock and ledger.total are out
		// of sync. in which case, we need to explicitly set the ledger's lock and amount,
		// otherwise the ledger recover will fail.
		assert_noop!(
			Staking::restore_ledger(RuntimeOrigin::root(), 444, None, None, None),
			Error::<Test>::CannotRestoreLedger
		);

		//and enforcing a new ledger lock/total on this non-corrupted ledger will work.
		assert_ok!(Staking::restore_ledger(
			RuntimeOrigin::root(),
			444,
			None,
			Some(lock_444_before + 40),
			None
		));

		// double-check that ledgers got to expected state and bond_extra done during the
		// corrupted state is part of the recovered ledgers.
		let ledger_333 = Bonded::<Test>::get(&333).and_then(Ledger::<Test>::get).unwrap();
		let ledger_444 = Bonded::<Test>::get(&444).and_then(Ledger::<Test>::get).unwrap();

		assert_eq!(ledger_333.total, lock_333_before + 30);
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &333), ledger_333.total);
		assert_eq!(ledger_444.total, lock_444_before + 40);
		assert_eq!(Balances::balance_locked(crate::STAKING_ID, &444), ledger_444.total);

		// try-state checks are ok now.
		assert_ok!(Staking::do_try_state(System::block_number()));
	})
}
