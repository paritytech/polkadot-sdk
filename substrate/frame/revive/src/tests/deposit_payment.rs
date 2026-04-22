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
	deposit_payment::Deposit,
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
	/// Payer's free DOT balance.
	payer_dot: u128,
	/// Payer's free PGAS balance.
	payer_pgas: u128,
	/// DOT currently held on the contract.
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
	/// ALICE's starting free DOT balance.
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

fn transfer_and_hold(from: &AccountId32, to: &AccountId32, amount: u128) -> DispatchResult {
	<<Test as Config>::Deposit as Deposit<Test>>::transfer_and_hold(
		HoldReason::StorageDepositReserve,
		from,
		to,
		amount,
	)
}

fn refund_on_hold(from: &AccountId32, to: &AccountId32, amount: u128) -> DispatchResult {
	<<Test as Config>::Deposit as Deposit<Test>>::refund_on_hold(
		HoldReason::StorageDepositReserve,
		from,
		to,
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
		// Ensure BOB's account exists so DOT holds can be created on it.
		frame_system::Pallet::<Test>::inc_providers(&BOB);

		for (i, charge) in case.charges.iter().enumerate() {
			assert_ok!(transfer_and_hold(&ALICE, &BOB, charge.amount));
			assert_eq!(snapshot(&ALICE, &BOB), charge.expected, "after charge {i}");
		}

		assert_ok!(refund_on_hold(&BOB, &ALICE, case.refund));
		assert_eq!(snapshot(&ALICE, &BOB), case.expected_after_refund, "after refund");
	});
}

/// DOT-only: ALICE has no PGAS, so the 100-unit hold is fully backed by DOT and
/// [`NativeDepositOf`] tracks it; the refund returns the DOT.
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

/// PGAS-only: ALICE's PGAS covers the hold, so DOT is untouched and no
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
/// exceeds remaining PGAS (60) so falls back to DOT in full. Refund pays DOT
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
