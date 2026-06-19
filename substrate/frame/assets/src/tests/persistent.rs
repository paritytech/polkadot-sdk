// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tests for the persistent (existential-deposit independent) accounts feature.

use super::*;
use crate::ExistenceReason;
use frame_support::{assert_noop, assert_ok};

/// Returns the [`ExistenceReason`] of the asset-account, panicking if it does not exist.
fn reason_of(asset: u32, who: u64) -> ExistenceReason<u64, u64> {
	Account::<Test>::get(asset, who).expect("account should exist").reason
}

#[test]
fn force_set_persistent_creates_account() {
	new_test_ext().execute_with(|| {
		// Non-sufficient asset with a non-trivial minimum balance.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		assert!(!Account::<Test>::contains_key(0, 5));

		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));

		// The account now exists with a zero balance and no `system` reference.
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 0);
		assert_eq!(account.reason, ExistenceReason::Persistent);
		assert_eq!(System::consumers(&5), 0);
		assert_eq!(System::sufficients(&5), 0);
		// The asset's account counter is bumped.
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 1);

		System::assert_last_event(RuntimeEvent::Assets(crate::Event::AccountPersistenceSet {
			asset_id: 0,
			who: 5,
		}));
	});
}

#[test]
fn force_set_persistent_requires_force_origin() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		assert_noop!(
			Assets::force_set_persistent(RuntimeOrigin::signed(1), 0, 5),
			sp_runtime::DispatchError::BadOrigin,
		);
	});
}

#[test]
fn force_set_persistent_unknown_asset_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5),
			Error::<Test>::Unknown,
		);
	});
}

#[test]
fn force_set_persistent_twice_fails() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_noop!(
			Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5),
			Error::<Test>::AlreadyPersistent,
		);
	});
}

#[test]
fn force_set_persistent_rejects_deposit_backed_account() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		// `touch` reserves a deposit and creates a `DepositHeld` account.
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::touch(RuntimeOrigin::signed(5), 0));
		assert!(matches!(reason_of(0, 5), ExistenceReason::DepositHeld(_)));

		assert_noop!(
			Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5),
			Error::<Test>::AccountHasDeposit,
		);
	});
}

#[test]
fn force_set_persistent_converts_existing_account_and_releases_refs() {
	new_test_ext().execute_with(|| {
		// Non-sufficient asset: a regular account holds a `Consumer` reference.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));
		assert_eq!(reason_of(0, 5), ExistenceReason::Consumer);
		assert_eq!(System::consumers(&5), 1);

		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));

		// Balance is preserved, reason flipped to persistent and the consumer ref released.
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 50);
		assert_eq!(account.reason, ExistenceReason::Persistent);
		assert_eq!(System::consumers(&5), 0);
		// The account counter is unchanged (account already existed).
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 1);
	});
}

#[test]
fn persistent_account_survives_being_drained() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 10));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));

		// Burn the whole balance: a regular account would be reaped here.
		assert_ok!(Assets::burn(RuntimeOrigin::signed(1), 0, 5, 10));

		// The persistent account is kept alive with a zero balance.
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 0);
		assert_eq!(account.reason, ExistenceReason::Persistent);
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 1);
	});
}

#[test]
fn regular_account_baseline_is_reaped_when_drained() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 10));
		// Sanity check that the same flow reaps a non-persistent account.
		assert_ok!(Assets::burn(RuntimeOrigin::signed(1), 0, 5, 10));
		assert!(!Account::<Test>::contains_key(0, 5));
	});
}

#[test]
fn persistent_account_survives_partial_burn_below_min() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 12));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));

		// Partial burn leaving a non-zero balance strictly below `min_balance`. A regular account
		// would either be kept above ED or fully reaped; the persistent account must just survive
		// with the sub-minimal balance (and must not trip the `is_zero` debug-assert).
		assert_ok!(Assets::burn(RuntimeOrigin::signed(1), 0, 5, 4));
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 8);
		assert_eq!(account.reason, ExistenceReason::Persistent);
	});
}

#[test]
fn persistent_account_survives_transfer_out() {
	new_test_ext().execute_with(|| {
		// Sufficient asset so the destination/source need no provider.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, true, 10));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 30));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));

		// Transfer down to a non-zero sub-minimal balance: must survive.
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(5), 0, 6, 25));
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 5);
		assert_eq!(account.reason, ExistenceReason::Persistent);

		// Transfer the rest out: still survives at zero balance.
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(5), 0, 6, 5));
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 0);
		assert_eq!(account.reason, ExistenceReason::Persistent);
	});
}

#[test]
fn asset_with_persistent_account_can_be_destroyed() {
	new_test_ext().execute_with(|| {
		// Regression: a persistent account must not block asset destruction.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 1);

		assert_ok!(Assets::start_destroy(RuntimeOrigin::signed(1), 0));
		assert_ok!(Assets::destroy_accounts(RuntimeOrigin::signed(1), 0));
		// The persistent account was removed and the counter decremented to zero.
		assert!(!Account::<Test>::contains_key(0, 5));
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 0);
		// Destruction can now finish.
		assert_ok!(Assets::destroy_approvals(RuntimeOrigin::signed(1), 0));
		assert_ok!(Assets::finish_destroy(RuntimeOrigin::signed(1), 0));
		assert!(Asset::<Test>::get(0).is_none());
	});
}

#[test]
fn force_unset_persistent_demotes_to_consumer_on_non_sufficient_asset() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		// `who` needs a provider so that a consumer reference can be (re)acquired.
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));
		assert_eq!(reason_of(0, 5), ExistenceReason::Persistent);
		assert_eq!(System::consumers(&5), 0);

		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5));

		assert_eq!(reason_of(0, 5), ExistenceReason::Consumer);
		assert_eq!(System::consumers(&5), 1);
	});
}

#[test]
fn force_unset_persistent_fails_when_consumer_unavailable() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		// `5` has no provider, so a consumer reference cannot be acquired on demote.
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));

		assert_noop!(
			Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5),
			Error::<Test>::UnavailableConsumer,
		);
		// The account is left untouched (still persistent, still funded).
		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 50);
		assert_eq!(account.reason, ExistenceReason::Persistent);
	});
}

#[test]
fn force_unset_persistent_reaps_empty_account() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 1);

		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5));

		// Zero-balance account is reaped.
		assert!(!Account::<Test>::contains_key(0, 5));
		assert_eq!(Asset::<Test>::get(0).unwrap().accounts, 0);
		System::assert_last_event(RuntimeEvent::Assets(crate::Event::AccountPersistenceRemoved {
			asset_id: 0,
			who: 5,
		}));
	});
}

#[test]
fn force_unset_persistent_demotes_funded_account() {
	new_test_ext().execute_with(|| {
		// Sufficient asset so the demoted account becomes `Sufficient`.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, true, 10));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_eq!(reason_of(0, 5), ExistenceReason::Persistent);

		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5));

		let account = Account::<Test>::get(0, 5).unwrap();
		assert_eq!(account.balance, 50);
		assert_eq!(account.reason, ExistenceReason::Sufficient);
		assert_eq!(System::sufficients(&5), 1);
	});
}

#[test]
fn force_unset_persistent_rejects_subminimal_balance() {
	new_test_ext().execute_with(|| {
		// Sufficient asset so we can fund below `min_balance` without a consumer ref.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, true, 10));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		// Bring the balance to a non-zero value strictly below the minimum balance.
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 5));
		assert_eq!(Account::<Test>::get(0, 5).unwrap().balance, 5);

		// Demoting would force a burn of the sub-minimal balance, which is rejected.
		assert_noop!(
			Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5),
			Error::<Test>::WouldBurn,
		);
	});
}

#[test]
fn force_unset_persistent_on_non_persistent_fails() {
	new_test_ext().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));

		assert_noop!(
			Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5),
			Error::<Test>::NotPersistent,
		);
	});
}

#[test]
fn adversarial_counter_drift_probe() {
	new_test_ext().execute_with(|| {
		// Helper: assert details.accounts == # account entries, details.sufficients == #
		// Sufficient-reason entries == System::sufficients summed.
		fn assert_invariants(asset: u32) {
			let details = Asset::<Test>::get(asset).unwrap();
			let entries: Vec<_> = Account::<Test>::iter_prefix(asset).collect();
			assert_eq!(
				details.accounts as usize,
				entries.len(),
				"accounts counter drift: details={} entries={}",
				details.accounts,
				entries.len()
			);
			let suff = entries
				.iter()
				.filter(|(_, a)| matches!(a.reason, ExistenceReason::Sufficient))
				.count();
			assert_eq!(
				details.sufficients as usize, suff,
				"sufficients counter drift: details={} actual={}",
				details.sufficients, suff
			);
		}

		// ---- Non-sufficient asset (id 0), min_balance 10. ----
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		Balances::make_free_balance_be(&6, 100);

		// set persistent (create), mint, unset (demote to consumer), set again, burn-all,
		// unset (reap), set again, destroy.
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_invariants(0);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));
		assert_invariants(0);
		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5));
		assert_eq!(reason_of(0, 5), ExistenceReason::Consumer);
		assert_invariants(0);
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_eq!(System::consumers(&5), 0);
		assert_invariants(0);
		assert_ok!(Assets::burn(RuntimeOrigin::signed(1), 0, 5, 50));
		assert_invariants(0);
		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5));
		assert!(!Account::<Test>::contains_key(0, 5));
		assert_invariants(0);

		// transfer from a regular account to a persistent destination then drain persistent out.
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 6, 100));
		assert_invariants(0);
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(6), 0, 5, 100));
		assert_invariants(0);
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(5), 0, 6, 100));
		assert_eq!(Account::<Test>::get(0, 5).unwrap().balance, 0);
		assert_eq!(reason_of(0, 5), ExistenceReason::Persistent);
		assert_invariants(0);

		// ---- Sufficient asset (id 1), min_balance 10. ----
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 1, 1, true, 10));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 1, 7, 50));
		assert_eq!(reason_of(1, 7), ExistenceReason::Sufficient);
		assert_invariants(1);
		// Sufficient -> Persistent (must drop a sufficient).
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 1, 7));
		assert_eq!(System::sufficients(&7), 0);
		assert_invariants(1);
		// Persistent -> Sufficient (demote funded, gains a sufficient).
		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 1, 7));
		assert_eq!(reason_of(1, 7), ExistenceReason::Sufficient);
		assert_eq!(System::sufficients(&7), 1);
		assert_invariants(1);
		// set again, transfer all out (persistent survives at zero), unset reaps.
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 1, 7));
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(7), 1, 8, 50));
		assert_invariants(1);
		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 1, 7));
		assert!(!Account::<Test>::contains_key(1, 7));
		assert_invariants(1);

		// destroy sufficient asset with a remaining persistent account.
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 1, 9));
		assert_invariants(1);
		assert_ok!(Assets::start_destroy(RuntimeOrigin::signed(1), 1));
		assert_ok!(Assets::destroy_accounts(RuntimeOrigin::signed(1), 1));
		assert_eq!(Asset::<Test>::get(1).unwrap().accounts, 0);
		assert_eq!(Asset::<Test>::get(1).unwrap().sufficients, 0);
	});
}

#[test]
fn adversarial_counter_drift_probe_edge() {
	new_test_ext().execute_with(|| {
		fn assert_invariants(asset: u32) {
			let details = Asset::<Test>::get(asset).unwrap();
			let entries: Vec<_> = Account::<Test>::iter_prefix(asset).collect();
			assert_eq!(details.accounts as usize, entries.len(), "accounts drift");
			let suff = entries
				.iter()
				.filter(|(_, a)| matches!(a.reason, ExistenceReason::Sufficient))
				.count();
			assert_eq!(details.sufficients as usize, suff, "sufficients drift");
		}

		// DepositRefunded -> Persistent path: touch (DepositHeld) -> refund (DepositRefunded)
		// requires balance zero; refund removes the account though. Instead build DepositRefunded
		// via touch then mint then... refund needs zero balance. So: touch creates DepositHeld with
		// zero balance; refund removes it. We can't easily reach a *surviving* DepositRefunded with
		// a balance here, so just test set-persistent on a DepositHeld is rejected and
		// self-transfer.

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, 1, false, 10));
		Balances::make_free_balance_be(&5, 100);
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 0, 5));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 0, 5, 50));
		assert_invariants(0);

		// Self-transfer on persistent account: must not drift.
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(5), 0, 5, 50));
		assert_eq!(reason_of(0, 5), ExistenceReason::Persistent);
		assert_invariants(0);
		// Self-transfer of the full balance.
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(5), 0, 5, 50));
		assert_invariants(0);

		// Unset at exactly min_balance (boundary): demote to consumer, no drift.
		assert_ok!(Assets::burn(RuntimeOrigin::signed(1), 0, 5, 40));
		assert_eq!(Account::<Test>::get(0, 5).unwrap().balance, 10);
		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 0, 5));
		assert_eq!(reason_of(0, 5), ExistenceReason::Consumer);
		assert_invariants(0);

		// Sufficient asset: mint to create brand-new persistent then mint more (existing-account
		// path), confirm no sufficients change while persistent.
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 1, 1, true, 10));
		assert_ok!(Assets::force_set_persistent(RuntimeOrigin::root(), 1, 7));
		assert_eq!(Asset::<Test>::get(1).unwrap().sufficients, 0);
		assert_invariants(1);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 1, 7, 5));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(1), 1, 7, 5));
		assert_eq!(Asset::<Test>::get(1).unwrap().sufficients, 0);
		assert_invariants(1);
		// now exactly at min -> demote to sufficient.
		assert_ok!(Assets::force_unset_persistent(RuntimeOrigin::root(), 1, 7));
		assert_eq!(reason_of(1, 7), ExistenceReason::Sufficient);
		assert_eq!(Asset::<Test>::get(1).unwrap().sufficients, 1);
		assert_invariants(1);
	});
}
