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

//! Tests for the [`PGasDeposit`] storage-deposit backend.

use crate::{
	Config, HoldReason, NativeDepositOf,
	deposit_payment::{Deposit, Funds},
	test_utils::{ALICE, BOB},
	tests::{Assets, AssetsHolder, Balances, ExtBuilder, PGAS_ASSET_ID, Test},
};
use frame_support::{
	assert_ok,
	traits::{
		fungible::{InspectHold, Mutate as _},
		tokens::fungibles::InspectHold as _,
	},
};
use pretty_assertions::assert_eq;
use sp_runtime::{AccountId32, DispatchResult};

/// Full observable state snapshot for a (payer, contract) pair.
#[derive(Debug, Default, PartialEq, Eq)]
struct State {
	/// Payer's free native currency balance.
	payer_dot: u128,
	/// Payer's free PGAS balance.
	payer_pgas: u128,
	/// Native currency currently held on the contract.
	contract_dot_held: u128,
	/// PGAS currently held on the contract.
	contract_pgas_held: u128,
	/// `NativeDepositOf[contract][payer]`: the payer's outstanding native-currency
	dot_entitlement: u128,
}

fn snapshot(payer: &AccountId32, contract: &AccountId32) -> State {
	let hold = HoldReason::StorageDepositReserve.into();
	State {
		payer_dot: Balances::free_balance(payer),
		payer_pgas: Assets::balance(PGAS_ASSET_ID, payer),
		contract_dot_held: Balances::balance_on_hold(&hold, contract),
		contract_pgas_held: AssetsHolder::balance_on_hold(PGAS_ASSET_ID, &hold, contract),
		dot_entitlement: NativeDepositOf::<Test>::get(contract, payer),
	}
}

/// One charge with the state expected immediately afterwards.
struct Charge {
	/// Amount to charge.
	amount: u128,
	/// Expected `State` right after the charge lands.
	expected: State,
}

/// A full scenario: initial balances, a sequence of charges, then one refund.
struct TestCase {
	/// ALICE's starting free native currency balance.
	initial_dot: u128,
	/// ALICE's starting PGAS balance.
	initial_pgas: u128,
	/// Sequential charges applied to BOB.
	charges: Vec<Charge>,
	/// Amount released back to ALICE.
	refund: u128,
	/// Expected `State` snapshot after the refund.
	expected_after_refund: State,
}

fn charge_and_hold(from: &AccountId32, to: &AccountId32, amount: u128) -> DispatchResult {
	<<Test as Config>::Deposit as Deposit<Test>>::charge_and_hold(
		HoldReason::StorageDepositReserve,
		Funds::Balance(from),
		to,
		amount,
	)
}

fn refund_on_hold(from: &AccountId32, to: &AccountId32, amount: u128) -> DispatchResult {
	<<Test as Config>::Deposit as Deposit<Test>>::refund_on_hold(
		HoldReason::StorageDepositReserve,
		from,
		Funds::Balance(to),
		amount,
	)
}

fn run(case: TestCase) {
	let mut builder = ExtBuilder::default();
	if case.initial_pgas > 0 {
		builder = builder.with_pgas_balances(vec![(ALICE, case.initial_pgas)]);
	}
	builder.build().execute_with(|| {
		Balances::set_balance(&ALICE, case.initial_dot);
		// Pretend BOB is a contract: mint both EDs (and thus a PGAS asset account) so
		// balanced operations and holds can land on it. `mint_into` brings the underlying
		// system account into existence as a side effect.
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));

		for (i, charge) in case.charges.iter().enumerate() {
			assert_ok!(charge_and_hold(&ALICE, &BOB, charge.amount));
			assert_eq!(snapshot(&ALICE, &BOB), charge.expected, "after charge {i}");
		}

		assert_ok!(refund_on_hold(&BOB, &ALICE, case.refund));
		assert_eq!(snapshot(&ALICE, &BOB), case.expected_after_refund, "after refund");
	});
}

/// Native-only: ALICE has no PGAS, so the 100-unit hold is fully backed by native currency
/// and [`NativeDepositOf`] tracks it; the refund returns the native currency.
#[test]
fn pay_dot_refund_dot() {
	run(TestCase {
		initial_dot: 1_000,
		initial_pgas: 0,
		charges: vec![Charge {
			amount: 100,
			expected: State {
				payer_dot: 900,
				contract_dot_held: 100,
				dot_entitlement: 100,
				..State::default()
			},
		}],
		refund: 100,
		expected_after_refund: State { payer_dot: 1_000, ..State::default() },
	});
}

/// PGAS-only: ALICE's PGAS covers the hold, so native currency is untouched and no
/// entitlement is recorded; the refund returns PGAS (at `PGasRefundPercent`).
#[test]
fn pay_pgas_refund_pgas() {
	run(TestCase {
		initial_dot: 1_000,
		initial_pgas: 1_000,
		charges: vec![Charge {
			amount: 100,
			expected: State {
				payer_dot: 1_000,
				payer_pgas: 900,
				contract_pgas_held: 100,
				..State::default()
			},
		}],
		refund: 100,
		expected_after_refund: State { payer_dot: 1_000, payer_pgas: 910, ..State::default() },
	});
}

/// Mixed: first charge (40) fits into ALICE's 100 PGAS; second charge (80)
/// exceeds remaining PGAS (60) so falls back to native currency in full. Refund pays native
/// first (capped by the entitlement), then PGAS for the remainder.
#[test]
fn pay_mixed_refund_mixed() {
	run(TestCase {
		initial_dot: 1_000,
		initial_pgas: 100,
		charges: vec![
			Charge {
				amount: 40,
				expected: State {
					payer_dot: 1_000,
					payer_pgas: 60,
					contract_pgas_held: 40,
					..State::default()
				},
			},
			Charge {
				amount: 80,
				expected: State {
					payer_dot: 920,
					payer_pgas: 60,
					contract_dot_held: 80,
					contract_pgas_held: 40,
					dot_entitlement: 80,
				},
			},
		],
		refund: 120,
		expected_after_refund: State { payer_dot: 1_000, payer_pgas: 64, ..State::default() },
	});
}

// ---------------------------------------------------------------------------
// Sub-ED hold: with PGAS ED set high (100) and a small charge (50), the resulting hold sits
// below the PGAS asset's ED. The refund path calls `burn_held` on that hold, and we want to
// confirm `burn_held` is happy with sub-ED held balances.
// ---------------------------------------------------------------------------

/// Sub-ED hold: charge 50 PGAS with PGAS ED = 100, then refund — the 10% refund (5) credits
/// ALICE and the rest (45) is burned. `burn_held` should not reject the sub-ED hold.
#[test]
fn burn_held_on_sub_ed_hold_works() {
	ExtBuilder::default()
		.with_pgas_min_balance(100)
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			Balances::set_balance(&ALICE, 1_000);
			// Pretend BOB is a contract — mint both EDs so balanced transfer/hold land cleanly.
			assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));

			assert_ok!(charge_and_hold(&ALICE, &BOB, 50));
			assert_eq!(
				snapshot(&ALICE, &BOB),
				State {
					payer_dot: 1_000,
					payer_pgas: 950,
					contract_pgas_held: 50,
					..State::default()
				},
				"after sub-ED charge",
			);

			// Refund: 10% of 50 = 5 goes to ALICE, 45 is burned via burn_held.
			assert_ok!(refund_on_hold(&BOB, &ALICE, 50));
			assert_eq!(
				snapshot(&ALICE, &BOB),
				State { payer_dot: 1_000, payer_pgas: 955, ..State::default() },
				"after refund (5 refunded, 45 burned)",
			);
		});
}

/// Partial sub-ED refund: charge 50 PGAS (below PGAS ED = 100), refund 20. Expect 2 refunded
/// to ALICE, 18 burned, 30 still held.
#[test]
fn burn_held_on_sub_ed_hold_partial_refund() {
	ExtBuilder::default()
		.with_pgas_min_balance(100)
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			Balances::set_balance(&ALICE, 1_000);
			assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));

			assert_ok!(charge_and_hold(&ALICE, &BOB, 50));
			assert_ok!(refund_on_hold(&BOB, &ALICE, 20));
			assert_eq!(
				snapshot(&ALICE, &BOB),
				State {
					payer_dot: 1_000,
					payer_pgas: 952,
					contract_pgas_held: 30,
					..State::default()
				},
				"after partial refund (2 refunded, 18 burned, 30 still held)",
			);
		});
}

// ---------------------------------------------------------------------------
// Direct tests for `mint_contract_eds` / `burn_contract_eds` against the PGAS backend
// (the test runtime uses `PGasDeposit`).
// ---------------------------------------------------------------------------

/// `mint_contract_eds` mints the native ED (deactivated) and the PGAS ED into the contract.
/// `burn_contract_eds` is its exact inverse: total_issuance, inactive_issuance, and active
/// issuance all return to their starting values.
#[test]
fn mint_and_burn_contract_eds_round_trip() {
	use frame_support::traits::{
		fungible::Inspect as _,
		tokens::fungibles::Inspect as FungiblesInspect,
	};
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		let dot_total_before = Balances::total_issuance();
		let dot_inactive_before = Balances::inactive_issuance();
		let dot_active_before = Balances::active_issuance();
		let pgas_total_before = Assets::total_issuance(PGAS_ASSET_ID);

		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));

		// BOB has the native ED in free balance, deactivated.
		assert_eq!(Balances::balance(&BOB), 50, "BOB should have native ED minted");
		// Total issuance bumped, but active issuance unchanged because the new ED is deactivated.
		assert_eq!(Balances::total_issuance(), dot_total_before + 50);
		assert_eq!(Balances::inactive_issuance(), dot_inactive_before + 50);
		assert_eq!(
			Balances::active_issuance(),
			dot_active_before,
			"deactivate keeps active issuance pinned"
		);

		// And the PGAS asset account is alive with PGAS ED.
		let pgas_ed = Assets::minimum_balance(PGAS_ASSET_ID);
		assert_eq!(Assets::balance(PGAS_ASSET_ID, &BOB), pgas_ed);
		assert_eq!(Assets::total_issuance(PGAS_ASSET_ID), pgas_total_before + pgas_ed);

		// The contract has no system consumers from the trait alone; the consumer pin is
		// added by `exec.rs`. To exercise `burn_contract_eds`, drop the provider count
		// holds that mint_into added — i.e. just call burn directly. With consumers == 0,
		// the burn can fully reduce the balance.
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::burn_contract_eds(&BOB));

		// Round-trip: every issuance counter is back where we started.
		assert_eq!(Balances::balance(&BOB), 0, "DOT ED has been burned out of BOB");
		assert_eq!(Assets::balance(PGAS_ASSET_ID, &BOB), 0);
		assert_eq!(Balances::total_issuance(), dot_total_before);
		assert_eq!(Balances::inactive_issuance(), dot_inactive_before);
		assert_eq!(Balances::active_issuance(), dot_active_before);
		assert_eq!(Assets::total_issuance(PGAS_ASSET_ID), pgas_total_before);
	});
}

/// Idempotency: calling `mint_contract_eds` twice does not double-mint either asset.
/// The check is needed for paths where the contract may already exist (e.g. forced funds,
/// migration helpers) before instantiation runs.
#[test]
fn mint_contract_eds_is_idempotent() {
	use frame_support::traits::fungible::Inspect as _;
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));
		let dot_after_first = Balances::balance(&BOB);
		let inactive_after_first = Balances::inactive_issuance();
		let pgas_after_first = Assets::balance(PGAS_ASSET_ID, &BOB);

		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));
		assert_eq!(Balances::balance(&BOB), dot_after_first, "no double native mint");
		assert_eq!(
			Balances::inactive_issuance(),
			inactive_after_first,
			"no double deactivate"
		);
		assert_eq!(
			Assets::balance(PGAS_ASSET_ID, &BOB),
			pgas_after_first,
			"no double PGAS mint"
		);
	});
}

/// After `mint_contract_eds`, the contract has a PGAS asset account with at least the
/// PGAS ED — so a sub-ED PGAS transfer into it succeeds (it would normally fail because
/// transfers below the asset's ED to a fresh account get rejected).
#[test]
fn minted_contract_can_receive_sub_ed_pgas() {
	use frame_support::traits::tokens::fungibles::Mutate as FungiblesMutate;
	ExtBuilder::default()
		.with_pgas_min_balance(100)
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			Balances::set_balance(&ALICE, 1_000);
			assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));

			// BOB's PGAS account is alive with the PGAS ED (=100).
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &BOB), 100);

			// A 30-PGAS transfer (well below the asset's ED of 100) lands cleanly on BOB
			// because the account already exists.
			assert_ok!(<Assets as FungiblesMutate<_>>::transfer(
				PGAS_ASSET_ID,
				&ALICE,
				&BOB,
				30,
				frame_support::traits::tokens::Preservation::Preserve,
			));
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &BOB), 130);
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &ALICE), 970);
		});
}

/// After `mint_contract_eds`, the contract has a native (DOT) account too — so a sub-ED
/// native transfer into it also succeeds.
#[test]
fn minted_contract_can_receive_sub_ed_native() {
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		Balances::set_balance(&ALICE, 1_000);
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));

		// BOB's native account is alive with native ED (=50).
		use frame_support::traits::fungible::Inspect as _;
		assert_eq!(Balances::balance(&BOB), 50);

		// A 10-DOT transfer (below ED) lands on BOB because the account exists.
		use frame_support::traits::fungible::Mutate as _;
		assert_ok!(Balances::transfer(
			&ALICE,
			&BOB,
			10,
			frame_support::traits::tokens::Preservation::Preserve,
		));
		assert_eq!(Balances::balance(&BOB), 60);
		assert_eq!(Balances::balance(&ALICE), 990);
	});
}

/// The native ED minted by `mint_contract_eds` is NOT extractable while the contract has
/// a system consumer. Burning the ED directly is rejected by the underlying balance pallet
/// because `can_dec_provider` is false (consumer pinned).
#[test]
fn minted_contract_native_ed_not_extractable_with_consumer() {
	use frame_support::traits::{
		fungible::Inspect as _,
		tokens::{Fortitude, Precision, Preservation, fungible::Mutate as _},
	};
	ExtBuilder::default().existential_deposit(50).build().execute_with(|| {
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::mint_contract_eds(&BOB));
		// Real instantiate path also adds a consumer pin — emulate that here so the
		// account looks like a live contract.
		assert_ok!(frame_system::Pallet::<Test>::inc_consumers(&BOB));

		// `Preservation::Expendable` would normally let the account be drained; with a
		// consumer outstanding, even Expendable+Force can't pull the ED out.
		let result = Balances::burn_from(
			&BOB,
			50,
			Preservation::Expendable,
			Precision::Exact,
			Fortitude::Force,
		);
		assert!(
			result.is_err(),
			"the consumer pin must keep the native ED non-extractable; got {result:?}"
		);
		assert_eq!(Balances::balance(&BOB), 50, "BOB still has the full ED");
	});
}
