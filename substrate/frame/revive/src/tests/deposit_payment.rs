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
	payer_native: u128,
	/// Payer's free PGAS balance.
	payer_pgas: u128,
	/// Native currency currently held on the contract.
	contract_native_held: u128,
	/// PGAS currently held on the contract.
	contract_pgas_held: u128,
	/// `NativeDepositOf[contract][payer]`: the payer's outstanding native-currency
	native_entitlement: u128,
}

fn snapshot(payer: &AccountId32, contract: &AccountId32) -> State {
	let hold = HoldReason::StorageDepositReserve.into();
	State {
		payer_native: Balances::free_balance(payer),
		payer_pgas: Assets::balance(PGAS_ASSET_ID, payer),
		contract_native_held: Balances::balance_on_hold(&hold, contract),
		contract_pgas_held: AssetsHolder::balance_on_hold(PGAS_ASSET_ID, &hold, contract),
		native_entitlement: NativeDepositOf::<Test>::get(contract, payer),
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
	initial_native: u128,
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
		Balances::set_balance(&ALICE, case.initial_native);
		// Mint the native and PGAS ED onto BOB, mirroring what `init_account` does at
		// contract creation time.
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::init_account(&BOB));

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
fn pay_native_refund_native() {
	run(TestCase {
		initial_native: 1_000,
		initial_pgas: 0,
		charges: vec![Charge {
			amount: 100,
			expected: State {
				payer_native: 900,
				contract_native_held: 100,
				native_entitlement: 100,
				..State::default()
			},
		}],
		refund: 100,
		expected_after_refund: State { payer_native: 1_000, ..State::default() },
	});
}

/// PGAS-only: ALICE's PGAS covers the hold, so native currency is untouched and no
/// entitlement is recorded; the refund returns PGAS (at `PGasRefundPercent`).
#[test]
fn pay_pgas_refund_pgas() {
	run(TestCase {
		initial_native: 1_000,
		initial_pgas: 1_000,
		charges: vec![Charge {
			amount: 100,
			expected: State {
				payer_native: 1_000,
				payer_pgas: 900,
				contract_pgas_held: 100,
				..State::default()
			},
		}],
		refund: 100,
		expected_after_refund: State { payer_native: 1_000, payer_pgas: 910, ..State::default() },
	});
}

/// Mixed: first charge (40) fits into ALICE's 100 PGAS; second charge (80)
/// exceeds remaining PGAS (60) so falls back to native currency in full. Refund pays native
/// first (capped by the entitlement), then PGAS for the remainder.
#[test]
fn pay_mixed_refund_mixed() {
	run(TestCase {
		initial_native: 1_000,
		initial_pgas: 100,
		charges: vec![
			Charge {
				amount: 40,
				expected: State {
					payer_native: 1_000,
					payer_pgas: 60,
					contract_pgas_held: 40,
					..State::default()
				},
			},
			Charge {
				amount: 80,
				expected: State {
					payer_native: 920,
					payer_pgas: 60,
					contract_native_held: 80,
					contract_pgas_held: 40,
					native_entitlement: 80,
				},
			},
		],
		refund: 120,
		expected_after_refund: State { payer_native: 1_000, payer_pgas: 64, ..State::default() },
	});
}

// ---------------------------------------------------------------------------
// Sub-ED hold: `init_account` mints the PGAS ED into the contract's free balance, so a
// charge of `amount < ED` puts a sub-ED hold on top. The refund path's `settle_pgas_refund`
// calls `burn_held` on the hold, and we want to confirm that's safe.
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
			assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::init_account(&BOB));

			// PGAS branch: 50 transferred on top of the ED minted by `init_account`.
			assert_ok!(charge_and_hold(&ALICE, &BOB, 50));
			assert_eq!(
				snapshot(&ALICE, &BOB),
				State {
					payer_native: 1_000,
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
				State { payer_native: 1_000, payer_pgas: 955, ..State::default() },
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
			assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::init_account(&BOB));

			assert_ok!(charge_and_hold(&ALICE, &BOB, 50));
			assert_ok!(refund_on_hold(&BOB, &ALICE, 20));
			assert_eq!(
				snapshot(&ALICE, &BOB),
				State {
					payer_native: 1_000,
					payer_pgas: 952,
					contract_pgas_held: 30,
					..State::default()
				},
				"after partial refund (2 refunded, 18 burned, 30 still held)",
			);
		});
}
