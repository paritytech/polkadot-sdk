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
	test_utils::{ALICE, BOB, CHARLIE, DJANGO},
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
use crate::tests::RuntimeOrigin;

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
		// Ensure BOB's account exists so native currency holds can be created on it.
		frame_system::Pallet::<Test>::inc_providers(&BOB);
		// Give the contract a PGAS asset account for balanced operations.
		assert_ok!(<<Test as Config>::Deposit as Deposit<Test>>::ensure_pgas_account(&BOB));

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
// Playground: does `burn_held` work when the contract's PGAS hold is below the PGAS asset's
// ED? `charge_and_hold`'s PGAS branch writes directly via `increase_balance_on_hold`
// (bypassing ED), so this is reachable in practice. The refund path's `settle_pgas_refund`
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
			frame_system::Pallet::<Test>::inc_providers(&BOB);

			// PGAS branch: writes directly to the hold, bypassing PGAS ED.
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
			frame_system::Pallet::<Test>::inc_providers(&BOB);

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

// ===========================================================================
// Edge-case and security tests demonstrating known shortcomings and tradeoffs
// of the PGAS deposit backend.
// ===========================================================================

fn charge_ed(from: &AccountId32, to: &AccountId32) -> Result<u128, sp_runtime::DispatchError> {
	<<Test as Config>::Deposit as Deposit<Test>>::charge_ed(Funds::Balance(from), to)
}

/// When `charge_ed` uses the PGAS path, the contract account is created via pallet-assets'
/// `inc_sufficients` (not `inc_providers`). `frame_system::inc_consumers` requires
/// `providers > 0`, so it returns `NoProviders` for a sufficients-only account.
///
/// This demonstrates that a PGAS-only ED creates an account that cannot accept consumer
/// references — the same consumer that pallet-revive adds at instantiation (exec.rs:1330).
#[test]
fn pgas_charge_ed_creates_sufficients_only_account() {
	ExtBuilder::default()
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			let contract = CHARLIE;
			assert!(!frame_system::Pallet::<Test>::account_exists(&contract));

			// charge_ed with PGAS creates the account via sufficient asset transfer.
			assert_ok!(charge_ed(&ALICE, &contract));
			assert!(frame_system::Pallet::<Test>::account_exists(&contract));

			// Account has sufficients > 0 but providers == 0.
			let info = frame_system::Pallet::<Test>::account(&contract);
			assert!(info.sufficients > 0, "should have a sufficient reference from PGAS");
			assert_eq!(info.providers, 0, "no provider — only PGAS was deposited");

			// inc_consumers fails: this is what exec.rs:1330 calls right after charge_ed.
			assert_eq!(
				frame_system::Pallet::<Test>::inc_consumers(&contract),
				Err(sp_runtime::DispatchError::NoProviders),
				"inc_consumers needs providers > 0, but PGAS only gives sufficients"
			);
		});
}

/// When `charge_ed` falls back to native currency (user has no PGAS), the contract gets a
/// provider reference and `inc_consumers` works.
#[test]
fn native_charge_ed_creates_provider_account() {
	ExtBuilder::default().build().execute_with(|| {
		let contract = CHARLIE;
		Balances::set_balance(&ALICE, 1_000);
		assert!(!frame_system::Pallet::<Test>::account_exists(&contract));

		assert_ok!(charge_ed(&ALICE, &contract));

		let info = frame_system::Pallet::<Test>::account(&contract);
		assert!(info.providers > 0, "native ED gives a provider");
		assert_eq!(info.sufficients, 0);

		// inc_consumers succeeds — the normal instantiation path works.
		assert_ok!(frame_system::Pallet::<Test>::inc_consumers(&contract));
	});
}

/// PGAS holds placed via `increase_balance_on_hold` bypass pallet-assets' Account storage.
/// The contract has held PGAS in pallet-assets-holder but NO `pallet_assets::Account` entry.
/// This means pallet-assets is unaware of the held balance.
#[test]
fn pgas_hold_exists_without_pallet_assets_account() {
	ExtBuilder::default()
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			let contract = BOB;
			frame_system::Pallet::<Test>::inc_providers(&contract);

			// Charge PGAS deposit: uses decrease_balance on ALICE + increase_balance_on_hold
			// on BOB, bypassing normal pallet-assets account creation.
			assert_ok!(charge_and_hold(&ALICE, &contract, 100));

			// BOB has PGAS on hold in assets-holder...
			let hold = HoldReason::StorageDepositReserve.into();
			let held = AssetsHolder::balance_on_hold(PGAS_ASSET_ID, &hold, &contract);
			assert_eq!(held, 100);

			// ...but has NO free PGAS balance in pallet-assets.
			let free = Assets::balance(PGAS_ASSET_ID, &contract);
			assert_eq!(free, 0, "contract has no pallet-assets Account entry");

			// The contract's frame_system sufficients was NOT incremented by the hold.
			let info = frame_system::Pallet::<Test>::account(&contract);
			assert_eq!(
				info.sufficients, 0,
				"increase_balance_on_hold does not touch frame_system refs"
			);
		});
}

/// NativeDepositOf tracks per (contract, user). A user who deposits into two different
/// contracts cannot drain more DOT from one contract than they put in.
#[test]
fn native_deposit_tracked_per_contract() {
	ExtBuilder::default().build().execute_with(|| {
		Balances::set_balance(&ALICE, 1_000);
		let contract_a = BOB;
		let contract_b = CHARLIE;
		frame_system::Pallet::<Test>::inc_providers(&contract_a);
		frame_system::Pallet::<Test>::inc_providers(&contract_b);

		// ALICE deposits 100 DOT into each contract (no PGAS → native fallback).
		assert_ok!(charge_and_hold(&ALICE, &contract_a, 100));
		assert_ok!(charge_and_hold(&ALICE, &contract_b, 200));

		assert_eq!(NativeDepositOf::<Test>::get(&contract_a, &ALICE), 100);
		assert_eq!(NativeDepositOf::<Test>::get(&contract_b, &ALICE), 200);

		// Refunding from contract_a is capped at 100 (what ALICE put there).
		assert_ok!(refund_on_hold(&contract_a, &ALICE, 100));
		assert_eq!(NativeDepositOf::<Test>::get(&contract_a, &ALICE), 0);
		// contract_b entitlement unchanged.
		assert_eq!(NativeDepositOf::<Test>::get(&contract_b, &ALICE), 200);
	});
}

/// Two users depositing into the same contract get independent entitlements.
/// User A cannot claim user B's native deposit.
#[test]
fn native_deposit_independent_per_user() {
	use crate::test_utils::CHARLIE;
	ExtBuilder::default().build().execute_with(|| {
		Balances::set_balance(&ALICE, 1_000);
		Balances::set_balance(&CHARLIE, 1_000);
		let contract = BOB;
		frame_system::Pallet::<Test>::inc_providers(&contract);

		assert_ok!(charge_and_hold(&ALICE, &contract, 50));
		assert_ok!(charge_and_hold(&CHARLIE, &contract, 80));

		assert_eq!(NativeDepositOf::<Test>::get(&contract, &ALICE), 50);
		assert_eq!(NativeDepositOf::<Test>::get(&contract, &CHARLIE), 80);

		// Refund to ALICE: capped at her 50, even though total DOT held on contract is 130.
		assert_ok!(refund_on_hold(&contract, &ALICE, 50));
		assert_eq!(Balances::free_balance(&ALICE), 1_000, "ALICE got her 50 back");
		// CHARLIE's entitlement untouched.
		assert_eq!(NativeDepositOf::<Test>::get(&contract, &CHARLIE), 80);
	});
}

/// When PGAS refund amount is below the PGAS asset's ED and the recipient has no PGAS
/// account, the refund portion is folded into the burn instead of failing.
#[test]
fn sub_ed_pgas_refund_folded_into_burn_when_recipient_has_no_account() {
	ExtBuilder::default()
		.with_pgas_min_balance(100)
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			let contract = BOB;
			frame_system::Pallet::<Test>::inc_providers(&contract);

			// Charge 30 PGAS (sub-ED).
			assert_ok!(charge_and_hold(&ALICE, &contract, 30));

			// Burn ALICE's remaining PGAS so she has no PGAS account for the refund.
			assert_ok!(Assets::transfer(RuntimeOrigin::signed(ALICE), PGAS_ASSET_ID, DJANGO, 970));

			// ALICE now has zero PGAS and no PGAS account.
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &ALICE), 0);

			// Refund: 10% of 30 = 3 PGAS. But 3 < PGAS ED (100), so can't credit ALICE.
			// Instead of failing, the 3 is folded into burn. Total burned = 30.
			assert_ok!(refund_on_hold(&contract, &ALICE, 30));
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGAS_ASSET_ID,
					&HoldReason::StorageDepositReserve.into(),
					&contract
				),
				0,
				"all PGAS hold released"
			);
			assert_eq!(
				Assets::balance(PGAS_ASSET_ID, &ALICE),
				0,
				"ALICE gets nothing — sub-ED refund folded into burn"
			);
		});
}

/// `charge_ed` with PGAS creates the destination account via a sufficient asset transfer.
/// Once the account exists (sufficients > 0), DOT can be transferred into it normally —
/// no native ED from the origin is needed.
///
/// This is the mechanism that allows PGAS-only origins to trigger transfers to new accounts:
/// PGAS ED creates the account, then DOT flows from the contract to the new account.
#[test]
fn pgas_ed_creates_account_then_dot_transfer_works() {
	ExtBuilder::default()
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			let contract = BOB;
			let new_account = CHARLIE;
			frame_system::Pallet::<Test>::inc_providers(&contract);
			Balances::set_balance(&contract, 1_000);

			// ALICE has zero DOT but plenty of PGAS.
			Balances::set_balance(&ALICE, 0);
			assert!(!frame_system::Pallet::<Test>::account_exists(&new_account));

			// charge_ed uses PGAS: creates new_account via sufficient asset transfer.
			assert_ok!(charge_ed(&ALICE, &new_account));

			// new_account now exists with sufficients > 0, providers == 0.
			assert!(frame_system::Pallet::<Test>::account_exists(&new_account));
			let info = frame_system::Pallet::<Test>::account(&new_account);
			assert!(info.sufficients > 0);
			assert_eq!(info.providers, 0);

			// DOT transfer from the contract to new_account succeeds — the account exists.
			assert_ok!(<Balances as frame_support::traits::fungible::Mutate<_>>::transfer(
				&contract,
				&new_account,
				100,
				frame_support::traits::tokens::Preservation::Preserve,
			));
			assert_eq!(Balances::free_balance(&new_account), 100);

			// After receiving DOT >= native ED, the account also has a provider.
			let info = frame_system::Pallet::<Test>::account(&new_account);
			assert!(info.providers > 0, "DOT deposit adds a provider");
		});
}

/// Origin with zero DOT and zero PGAS cannot create accounts at all.
#[test]
fn no_dot_no_pgas_origin_cannot_fund_ed() {
	ExtBuilder::default().build().execute_with(|| {
		let new_account = CHARLIE;
		Balances::set_balance(&ALICE, 0);

		let result = charge_ed(&ALICE, &new_account);
		assert!(result.is_err(), "no DOT and no PGAS → charge_ed fails");
		assert!(!frame_system::Pallet::<Test>::account_exists(&new_account));
	});
}

/// The PGAS `charge_and_hold` path uses `Preservation::Preserve`, so the payer must retain
/// at least PGAS ED after the charge. If the charge would drain the payer below ED, it falls
/// through to native currency instead.
///
/// When the PGAS path IS taken, the contract gets held PGAS via `increase_balance_on_hold`
/// without a pallet-assets Account being created.
#[test]
fn pgas_charge_preserves_payer_ed_or_falls_through_to_native() {
	ExtBuilder::default()
		.with_pgas_balances(vec![(ALICE, 200)])
		.build()
		.execute_with(|| {
			let contract = BOB;
			Balances::set_balance(&ALICE, 1_000);
			frame_system::Pallet::<Test>::inc_providers(&contract);

			// ALICE has 200 PGAS, min_balance = 1. Reducible = 199.
			// Charge 100 PGAS: fits in reducible balance, takes PGAS path.
			assert_ok!(charge_and_hold(&ALICE, &contract, 100));
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &ALICE), 100, "100 PGAS remains");

			// Contract has 100 PGAS on hold, 0 free — no pallet-assets Account.
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &contract), 0);
			assert_eq!(
				AssetsHolder::balance_on_hold(
					PGAS_ASSET_ID,
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				100
			);

			// Now charge 100 more. ALICE has 100 PGAS, reducible = 99 < 100.
			// Falls through to native.
			assert_ok!(charge_and_hold(&ALICE, &contract, 100));
			assert_eq!(Assets::balance(PGAS_ASSET_ID, &ALICE), 100, "PGAS untouched");
			assert_eq!(Balances::free_balance(&ALICE), 900, "DOT used instead");
		});
}

/// Refund of native deposit uses `Precision::BestEffort`. If the actual native hold is less
/// than `NativeDepositOf` records (e.g., due to slashing), the refund takes what it can and
/// settles the remainder from PGAS. The entitlement is only decremented by the actual amount
/// released, not the full requested amount.
#[test]
fn native_refund_best_effort_when_hold_reduced() {
	ExtBuilder::default()
		.with_pgas_balances(vec![(ALICE, 1_000)])
		.build()
		.execute_with(|| {
			let contract = BOB;
			Balances::set_balance(&ALICE, 1_000);
			frame_system::Pallet::<Test>::inc_providers(&contract);

			// Remove ALICE's PGAS first so charge falls through to DOT.
			assert_ok!(Assets::transfer(
				RuntimeOrigin::signed(ALICE), PGAS_ASSET_ID, DJANGO, 1_000
			));

			// Charge 100 DOT (native path because ALICE has no PGAS).
			assert_ok!(charge_and_hold(&ALICE, &contract, 100));
			assert_eq!(NativeDepositOf::<Test>::get(&contract, &ALICE), 100);

			// Simulate slashing: forcibly reduce the native hold from 100 to 60.
			use frame_support::traits::fungible::MutateHold as _;
			Balances::release(
				&HoldReason::StorageDepositReserve.into(),
				&contract,
				40,
				frame_support::traits::tokens::Precision::BestEffort,
			)
			.unwrap();

			// NativeDepositOf still says 100, but actual hold is only 60.
			assert_eq!(
				Balances::balance_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					&contract,
				),
				60
			);

			// Give ALICE PGAS and add a PGAS hold on the contract to cover the shortfall.
			use frame_support::traits::tokens::fungibles::Mutate as _;
			assert_ok!(Assets::mint_into(PGAS_ASSET_ID, &ALICE, 500));
			assert_ok!(charge_and_hold(&ALICE, &contract, 40));

			// Refund 100 total. DOT portion: requested = min(100, entitlement=100) = 100,
			// but BestEffort only releases 60 (actual hold). Remaining 40 settles from PGAS.
			let alice_dot_before = Balances::free_balance(&ALICE);
			assert_ok!(refund_on_hold(&contract, &ALICE, 100));
			let alice_dot_after = Balances::free_balance(&ALICE);

			// ALICE got 60 DOT back (what was actually held).
			assert_eq!(alice_dot_after - alice_dot_before, 60);

			// Entitlement decremented by 60 (actual release), not 100 (requested).
			assert_eq!(NativeDepositOf::<Test>::get(&contract, &ALICE), 40);
		});
}
