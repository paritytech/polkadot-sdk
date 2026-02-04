// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Unit tests for the auctions pallet.

use crate::{
	mock::*,
	pallet::{
		ActiveSurplusAuctionId, Auction, AuctionConfig, AuctionType, Auctions, CircuitBreakerLevel,
		Error, Event, NextAuctionId, OnIdleCursor, Stopped, SurplusHandlingMode, Tab,
	},
	price_calculators::PriceCurve,
	SurplusMode,
};
use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{fungible::Mutate as FungibleMutate, Hooks},
	weights::Weight,
};
use sp_pusd::{AuctionsHandler, CollateralManager, DebtComponents};
use sp_runtime::{
	traits::{Bounded, CheckedDiv, One, Saturating, Zero},
	DispatchError, FixedPointNumber, FixedU128, Permill,
};

// DOT unit (10 decimals)
const DOT: u128 = 10_000_000_000;

/// Helper to start an auction via the Auctions trait
/// For test simplicity, we treat the entire tab as principal (no interest/penalty split)
fn start_test_auction(vault_owner: u64, collateral: u128, tab: u128) -> Result<u32, DispatchError> {
	// First create a seized hold (simulating vaults pallet behavior)
	create_seized_hold(vault_owner, collateral);

	// Start the auction with tab as principal only (collateral is always native DOT)
	// In real usage, vaults pallet would pass principal, interest, and penalty separately
	crate::Pallet::<Test>::start_auction(
		vault_owner,
		collateral,
		DebtComponents::new(tab, 0, 0),
		KEEPER,
	)
}

#[test]
fn start_auction_works() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Verify auction was created
		assert_eq!(auction_id, 1);
		assert_eq!(NextAuctionId::<Test>::get(), 2);

		// Verify auction data
		let auction = Auctions::<Test>::get(1).unwrap();
		assert_eq!(auction.tab.total(), tab);
		assert_eq!(auction.auctionable_collateral, collateral);
		assert_eq!(auction.vault_owner, Some(VAULT_OWNER));
		assert_eq!(auction.starting_block, 1); // Block 1

		// Verify starting_price (oracle price * buffer = 0.000421 * 1.2)
		let config = AuctionConfig::<Test>::get(AuctionType::Liquidation);
		let expected_starting_price =
			MockCollateralManager::get_dot_price().unwrap().saturating_mul(config.buffer);
		assert_eq!(auction.starting_price, expected_starting_price);

		// Verify active auction count
		assert_eq!(Auctions::<Test>::count(), 1);

		// Verify event was emitted
		System::assert_has_event(
			Event::AuctionStarted {
				auction_type: AuctionType::Liquidation,
				id: 1,
				tab,
				lot: collateral,
				owner: Some(VAULT_OWNER),
				starting_block: 1, // Block 1
				starting_price: expected_starting_price,
				keeper: KEEPER,
			}
			.into(),
		);
	});
}

#[test]
fn start_auction_fails_when_stopped() {
	new_test_ext().execute_with(|| {
		// Set circuit breaker to NoNewAuctions (no new auctions)
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctions);

		let result = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT);
		assert_noop!(result, Error::<Test>::AuctionsStopped);
	});
}

#[test]
fn start_auction_fails_without_oracle_price() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::fungible::MutateHold;

		// Remove oracle price
		set_mock_price(None);

		// Need to manually hold since start_test_auction calls create_seized_hold
		// which would fail if there's no price check there
		Balances::hold(&MockHoldReason::Seized.into(), &VAULT_OWNER, 100 * DOT).unwrap();

		let result = crate::Pallet::<Test>::start_auction(
			VAULT_OWNER,
			100 * DOT,
			DebtComponents::new(1000 * PUSD_UNIT, 0, 0),
			KEEPER,
		);
		assert_noop!(result, Error::<Test>::PriceNotAvailable);
	});
}
/// Helper to create test `SlowedExponentialDecrease` curve
fn test_slowed_exp_curve() -> PriceCurve {
	PriceCurve::SlowedExponentialDecrease {
		center: 10,
		scale_factor: FixedU128::from(1000),
		linear_coeff: FixedU128::from_rational(65, 10000), // 0.0065
		center_ratio: FixedU128::from_rational(99, 100),   // 0.99
		minimum_price: FixedU128::from_rational(4, 10),    // 40% floor
	}
}

/// Helper: `starting_price` = 120, buffer = 1.2 (oracle = 100)
fn test_curve_params() -> (FixedU128, FixedU128) {
	let starting_price = FixedU128::from(120);
	let buffer = FixedU128::from_rational(12, 10); // 1.2, so oracle = 100
	(starting_price, buffer)
}

#[test]
fn slowed_exponential_decrease_price_at_center() {
	let curve = test_slowed_exp_curve();
	let (starting_price, buffer) = test_curve_params();
	// At center (t=10), price = oracle × center_ratio = 100 × 0.99 = 99
	let price = curve.calculate_price(starting_price, buffer, 10u64);
	assert_eq!(price, FixedU128::from(99));
}

#[test]
fn slowed_exponential_decrease_price_at_start() {
	let curve = test_slowed_exp_curve();
	let (starting_price, buffer) = test_curve_params();
	let price = curve.calculate_price(starting_price, buffer, 0u64);
	// With oracle_price scaling on linear_coeff:
	// c = oracle × center_ratio = 100 × 0.99 = 99
	// linear_term = oracle × linear_coeff × |x| = 100 × 0.0065 × 10 = 6.5
	// price ≈ 99 + 6.5 = 105.5 (below starting_price of 120)
	assert!(price > FixedU128::from(105));
	assert!(price < FixedU128::from(106));
}

#[test]
fn slowed_exponential_decrease_price_respects_floor() {
	let curve = test_slowed_exp_curve();
	let (starting_price, buffer) = test_curve_params();
	// After many blocks, should hit the 40% floor
	let price = curve.calculate_price(starting_price, buffer, 10000u64);
	assert_eq!(price, FixedU128::from(48)); // 120 * 0.4 = 48
}

#[test]
fn slowed_exponential_decrease_price_decays_over_time() {
	let curve = test_slowed_exp_curve();
	let (starting_price, buffer) = test_curve_params();

	let price_0 = curve.calculate_price(starting_price, buffer, 0u64);
	let price_10 = curve.calculate_price(starting_price, buffer, 10u64);
	let price_20 = curve.calculate_price(starting_price, buffer, 20u64);

	assert!(price_10 < price_0);
	assert!(price_20 < price_10);
}

#[test]
fn take_full_auction_works() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Get current price
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// BOB buys all collateral
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral, // Buy all collateral
			price,      // Accept current price
			BOB,        // Receive collateral
		));

		// Verify auction was removed
		assert!(Auctions::<Test>::get(auction_id).is_none());
		assert_eq!(Auctions::<Test>::count(), 0);

		// Verify callback was called
		assert!(get_debt_collected() > 0);
	});
}

#[test]
fn take_partial_auction_works() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Get current price
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);
		let initial_total_tab = auction.tab.total();

		// BOB buys half the collateral
		let buy_amount = 50 * DOT;
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			buy_amount,
			price,
			BOB,
		));

		// Verify auction still exists with reduced auctionable_collateral
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, collateral - buy_amount);
		assert!(auction.tab.total() < initial_total_tab); // Tab should be reduced
	});
}

#[test]
fn take_fails_when_stopped() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Set circuit breaker to AllDisabled (emergency stop)
		Stopped::<Test>::put(CircuitBreakerLevel::AllDisabled);

		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				50 * DOT,
				FixedU128::max_value(),
				BOB,
			),
			Error::<Test>::TakeStopped
		);
	});
}

#[test]
fn take_fails_with_price_too_high() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Try to buy with a very low max price
		let low_price = FixedU128::from_rational(1, 1000000);

		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				50 * DOT,
				low_price,
				BOB,
			),
			Error::<Test>::PriceTooHigh
		);
	});
}

#[test]
fn take_fails_with_zero_amount() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				0,
				FixedU128::max_value(),
				BOB,
			),
			Error::<Test>::PurchaseTooSmall
		);
	});
}

#[test]
fn take_fails_for_nonexistent_auction() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				999,
				50 * DOT,
				FixedU128::max_value(),
				BOB,
			),
			Error::<Test>::AuctionNotFound
		);
	});
}

// Note: InvalidAuctionType tests would require surplus auctions to be set up,
// which requires mock CollateralManager to support surplus operations with
// Insurance Fund balance. The error path is exercised by the implementation
// when take_liquidation is called on a Surplus auction or vice versa.

#[test]
fn restart_auction_works_after_tail_exceeded() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Advance past tail (21600 blocks)
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Verify auction needs restart
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert!(crate::Pallet::<Test>::needs_restart(&auction));

		// Restart the auction
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(KEEPER),
			auction_id,
			KEEPER,
		));

		// Verify auction was reset
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.starting_block, 21602); // Reset to current block
		assert!(!crate::Pallet::<Test>::needs_restart(&auction));
	});
}

#[test]
fn restart_auction_fails_when_not_needed() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Don't advance time - auction is fresh
		assert_noop!(
			crate::Pallet::<Test>::restart_auction(
				RuntimeOrigin::signed(KEEPER),
				auction_id,
				KEEPER
			),
			Error::<Test>::DoesNotNeedRestart
		);
	});
}

#[test]
fn restart_auction_fails_when_stopped() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Block on_idle restarts while advancing past tail.
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);

		// Set circuit breaker to NoNewAuctionsOrRestarts (no restart)
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);

		assert_noop!(
			crate::Pallet::<Test>::restart_auction(
				RuntimeOrigin::signed(KEEPER),
				auction_id,
				KEEPER
			),
			Error::<Test>::RestartStopped
		);
	});
}

#[test]
fn set_buffer_works() {
	new_test_ext().execute_with(|| {
		let new_buffer = FixedU128::from_rational(130, 100); // 30% buffer

		assert_ok!(crate::Pallet::<Test>::set_buffer(
			RuntimeOrigin::root(),
			AuctionType::Liquidation,
			new_buffer,
		));

		let config = AuctionConfig::<Test>::get(AuctionType::Liquidation);
		assert_eq!(config.buffer, new_buffer);

		// Verify surplus config is unchanged
		let surplus_config = AuctionConfig::<Test>::get(AuctionType::Surplus);
		assert_ne!(surplus_config.buffer, new_buffer);
	});
}
#[test]
fn set_stopped_works() {
	new_test_ext().execute_with(|| {
		// Set to NoNewAuctionsOrRestarts
		assert_ok!(crate::Pallet::<Test>::set_stopped(
			RuntimeOrigin::root(),
			CircuitBreakerLevel::NoNewAuctionsOrRestarts
		));
		assert_eq!(Stopped::<Test>::get(), CircuitBreakerLevel::NoNewAuctionsOrRestarts);

		// Set back to AllEnabled
		assert_ok!(crate::Pallet::<Test>::set_stopped(
			RuntimeOrigin::root(),
			CircuitBreakerLevel::AllEnabled
		));
		assert_eq!(Stopped::<Test>::get(), CircuitBreakerLevel::AllEnabled);
	});
}

#[test]
fn config_functions_fail_for_non_root() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			crate::Pallet::<Test>::set_buffer(
				RuntimeOrigin::signed(ALICE),
				AuctionType::Liquidation,
				FixedU128::one()
			),
			DispatchError::BadOrigin
		);
		assert_noop!(
			crate::Pallet::<Test>::set_maximum_duration(
				RuntimeOrigin::signed(ALICE),
				AuctionType::Liquidation,
				100
			),
			DispatchError::BadOrigin
		);
		assert_noop!(
			crate::Pallet::<Test>::set_stopped(
				RuntimeOrigin::signed(ALICE),
				CircuitBreakerLevel::NoNewAuctions
			),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn auction_price_decreases_over_time() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let initial_price = crate::Pallet::<Test>::current_price(&auction);

		// Advance 1000 blocks
		run_to_block(1001);

		let later_price = crate::Pallet::<Test>::current_price(&auction);

		// Price should be lower
		assert!(later_price < initial_price);
	});
}
#[test]
fn auction_completion_records_shortfall() {
	new_test_ext().execute_with(|| {
		let collateral = 10 * DOT; // Small amount
		let tab = 1000 * PUSD_UNIT; // Large debt

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Advance time so price drops, but stay above cusp (40%)
		// With tau=21600, at block 10000 (~46% through), price is ~54% of initial
		// which is above cusp, so no redo needed
		run_to_block(10000);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Buy all collateral at current price
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral,
			price,
			BOB,
		));

		// Auction should be completed with shortfall (tab > collateral_value)
		assert!(Auctions::<Test>::get(auction_id).is_none());

		// Shortfall should be recorded (the debt we couldn't raise from selling collateral)
		let shortfall = get_shortfall_recorded();
		assert!(shortfall > 0);
	});
}

/// This test verifies that `transfer_on_hold` with `Restriction::Free` releases
/// collateral to the buyer's FREE balance, not as a held balance.
///
/// PR review claimed this was a bug, but `Restriction::Free` should release to free balance.
#[test]
fn transfer_on_hold_releases_to_free_balance() {
	use frame_support::traits::fungible::{Inspect, InspectHold};

	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		// Record buyer's initial balances
		let buyer_initial_free = Balances::balance(&BOB);
		let buyer_initial_held: u128 =
			Balances::balance_on_hold(&MockHoldReason::Seized.into(), &BOB);
		assert_eq!(buyer_initial_held, 0, "Buyer should start with no held balance");

		// Start auction (creates seized hold on VAULT_OWNER)
		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Verify vault owner has held balance
		let vault_owner_held: u128 =
			Balances::balance_on_hold(&MockHoldReason::Seized.into(), &VAULT_OWNER);
		assert_eq!(vault_owner_held, collateral, "Vault owner should have collateral held");

		// Get current price
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// BOB buys all collateral
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral,
			price,
			BOB,
		));

		// CRITICAL CHECK: Verify buyer received collateral as FREE balance, NOT held
		let buyer_final_held: u128 =
			Balances::balance_on_hold(&MockHoldReason::Seized.into(), &BOB);
		let buyer_final_free = Balances::balance(&BOB);

		// Buyer should still have 0 held balance (collateral should be FREE)
		assert_eq!(
			buyer_final_held, 0,
			"BUG: Buyer should NOT have held balance after purchase! \
            transfer_on_hold with Restriction::Free should release to free balance."
		);

		// Buyer's free balance should have INCREASED by the collateral amount
		// (minus any fees they paid in pUSD, but the collateral is in native currency)
		assert!(
			buyer_final_free > buyer_initial_free - (tab / 10), /* rough check accounting for
			                                                     * payment */
			"Buyer should have received collateral as free balance. \
            Initial: {}, Final: {}, Expected increase of approximately: {}",
			buyer_initial_free,
			buyer_final_free,
			collateral
		);

		// Vault owner should have no more held balance
		let vault_owner_final_held: u128 =
			Balances::balance_on_hold(&MockHoldReason::Seized.into(), &VAULT_OWNER);
		assert_eq!(
			vault_owner_final_held, 0,
			"Vault owner should have no held balance after auction completes"
		);
	});
}

/// Test partial purchase also releases to free balance
/// Verify keeper receives correct incentive (tip + chip * tab) on restart
#[test]
fn restart_auction_pays_keeper_incentive() {
	new_test_ext().execute_with(|| {
		let principal = 1000 * PUSD_UNIT;
		let penalty = 1000 * PUSD_UNIT; // Provide penalty to fund keeper incentives.
		let base_tab = principal.saturating_add(penalty);

		create_seized_hold(VAULT_OWNER, 100 * DOT);
		let auction_id = crate::Pallet::<Test>::start_auction(
			VAULT_OWNER,
			100 * DOT,
			DebtComponents::new(principal, 0, penalty),
			KEEPER,
		)
		.unwrap();

		// Record keeper's initial pUSD balance
		let keeper_initial = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);

		// Advance past tail to make auction stale
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Get config for incentive calculation
		let config = AuctionConfig::<Test>::get(AuctionType::Liquidation);
		let expected_incentive_raw = config.tip + config.chip.mul_floor(base_tab);
		let expected_incentive = expected_incentive_raw.min(penalty);

		// Restart the auction
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(ALICE), // Anyone can call
			auction_id,
			KEEPER, // Keeper will receive incentive at completion
		));

		// Verify keeper has NOT received payment yet (payment happens at completion)
		let keeper_after_restart = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);
		assert_eq!(
			keeper_after_restart, keeper_initial,
			"Keeper should not receive payment on restart (paid at completion)"
		);

		// Verify event contains correct incentive
		System::assert_has_event(
			Event::AuctionRestarted {
				auction_type: AuctionType::Liquidation,
				id: auction_id,
				starting_price: Auctions::<Test>::get(auction_id).unwrap().starting_price,
				tab: base_tab,
				lot: 100 * DOT,
				owner: Some(VAULT_OWNER),
				keeper: KEEPER,
				incentive: expected_incentive,
			}
			.into(),
		);

		// Verify auction stores keeper incentive but tab unchanged
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.tab.total(), base_tab, "Tab should remain unchanged");
		assert_eq!(
			auction.keeper_incentive, expected_incentive,
			"Keeper incentive should be stored"
		);
		assert_eq!(auction.keeper, KEEPER, "Keeper should be updated");
	});
}

/// Verify partial take that would leave dust is rejected when auction is already below min
#[test]
fn take_rejects_dusty_remainder() {
	new_test_ext().execute_with(|| {
		// Neutralize keeper incentives so dust tests use base tab only.
		AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.tip = 0;
			config.chip = Permill::from_parts(0);
		});

		// Start auction with tab BELOW MinAuctionTab (100 PUSD)
		// This triggers the DustyAuction error path
		let tab = 80 * PUSD_UNIT; // Below 100 PUSD min
		let collateral = 100 * DOT;
		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Advance to center block where price is at oracle × center_ratio
		// This ensures price is low enough that partial take doesn't complete auction
		run_to_block(101);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Try partial take - should fail because auction.tab < min_tab
		// and a partial take would leave a dusty remainder
		let partial_slice = 10 * DOT; // Small partial amount

		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				partial_slice,
				price,
				BOB,
			),
			Error::<Test>::DustyAuction
		);

		// Full take should still work (clears the auction)
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral, // Take all
			price,
			BOB,
		));
	});
}

/// Verify take adjusts to leave at least `MinAuctionTab` when possible
#[test]
fn take_adjusts_to_avoid_dust() {
	new_test_ext().execute_with(|| {
		// Neutralize keeper incentives so dust tests use base tab only.
		AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.tip = 0;
			config.chip = Permill::from_parts(0);
		});

		// Start auction with tab well above MinAuctionTab.
		// At center price (~4.17 pUSD/DOT), 200 DOT = ~834 PUSD collateral value.
		// Tab of 500 PUSD is well covered.
		let tab = 500 * PUSD_UNIT;
		let collateral = 200 * DOT;
		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Advance to block 11 (past center) for stable price.
		// At block 11 (elapsed=10), price = oracle × center_ratio = 4.21 × 0.99 ≈ 4.17
		System::set_block_number(11);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Try to buy amount that would leave dust.
		// Requesting collateral worth 450 PUSD would leave 50 PUSD < 100 PUSD min
		let owe_target = 450 * PUSD_UNIT;
		let slice = FixedU128::saturating_from_integer(owe_target)
			.checked_div(&price)
			.unwrap()
			.saturating_mul_int(1u128);

		// Execute take - should fail; buyer must take all remaining collateral instead
		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				slice,
				FixedU128::max_value(), // High max price to pass price check
				BOB,
			),
			Error::<Test>::DustyAuction
		);

		// Taking all collateral clears the auction
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral,
			FixedU128::max_value(),
			BOB,
		));
		assert!(Auctions::<Test>::get(auction_id).is_none());
	});
}

/// Verify excess collateral returns to vault owner when tab is satisfied
#[test]
fn excess_collateral_returned_to_owner() {
	use frame_support::traits::fungible::{Inspect, InspectHold};

	new_test_ext().execute_with(|| {
		// Large collateral, moderate debt - will have excess
		let collateral = 100 * DOT;
		let tab = 200 * PUSD_UNIT; // Above min tab but still leaves excess

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Record vault owner's initial free balance
		let owner_initial_free = Balances::balance(&VAULT_OWNER);

		// Advance to center block for lower price
		run_to_block(101);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Buy enough to satisfy the full base tab.
		// At current price, slice_needed = total_owe / price
		let total_owe = auction.tab.total();
		let slice_needed = FixedU128::saturating_from_integer(total_owe)
			.checked_div(&price)
			.unwrap()
			.saturating_mul_int(1u128);

		// Request all collateral - system should only take what's needed for tab
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral, // Request all
			price,
			BOB,
		));

		// Auction should be completed (tab satisfied)
		assert!(Auctions::<Test>::get(auction_id).is_none());

		// Verify vault owner received excess collateral back (released from hold)
		let owner_held: u128 =
			Balances::balance_on_hold(&MockHoldReason::Seized.into(), &VAULT_OWNER);
		assert_eq!(owner_held, 0, "No collateral should remain held");

		let owner_final_free = Balances::balance(&VAULT_OWNER);
		let returned_collateral = owner_final_free - owner_initial_free;

		// Should have returned collateral - slice_needed
		assert!(returned_collateral > 0, "Vault owner should receive excess collateral back");

		// Verify completion event shows remaining collateral returned
		System::assert_has_event(
			Event::AuctionCompleted {
				auction_type: AuctionType::Liquidation,
				id: auction_id,
				remaining: collateral - slice_needed,
				shortfall: 0,
			}
			.into(),
		);
	});
}

/// Verify multiple sequential partial takes work correctly
#[test]
fn multiple_sequential_takes_work() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;
		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// First take: 30% of collateral
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let initial_total_tab = auction.tab.total();
		let price1 = crate::Pallet::<Test>::current_price(&auction);
		let take1 = 30 * DOT;

		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			take1,
			price1,
			BOB,
		));

		// Verify auction updated
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, collateral - take1);
		let tab_after_1 = auction.tab.total();
		assert!(tab_after_1 < initial_total_tab);

		// Advance some blocks
		run_to_block(100);

		// Reload auction to get fresh state for price calculation
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price2 = crate::Pallet::<Test>::current_price(&auction);
		assert!(price2 < price1, "Price should have decreased");
		let take2 = 30 * DOT;

		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(CHARLIE),
			auction_id,
			take2,
			price2,
			CHARLIE,
		));

		// Verify auction updated again
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, collateral - take1 - take2);
		assert!(auction.tab.total() < tab_after_1);

		// Third take: remaining collateral (use BOB who has pUSD)
		run_to_block(200);
		// Reload auction again
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price3 = crate::Pallet::<Test>::current_price(&auction);
		let remaining = auction.auctionable_collateral;

		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			remaining,
			price3,
			BOB,
		));

		// Auction should be completed
		assert!(Auctions::<Test>::get(auction_id).is_none());
	});
}

/// Verify take fails on stale auction that needs restart
#[test]
fn take_fails_on_stale_auction() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Advance past tail - auction becomes stale
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);

		// Verify auction needs restart
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert!(crate::Pallet::<Test>::needs_restart(&auction));

		// Take should fail
		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				50 * DOT,
				FixedU128::max_value(),
				BOB,
			),
			Error::<Test>::AuctionNeedsRestart
		);
	});
}

/// Verify pUSD is burned atomically with collateral transfer
#[test]
fn debt_burned_matches_collateral_value() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;
		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Record buyer's initial pUSD
		let buyer_initial_pusd = Assets::balance(STABLECOIN_ASSET_ID, BOB);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Buy specific amount
		let buy_amount = 50 * DOT;
		let expected_owe = price.saturating_mul_int(buy_amount);

		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			buy_amount,
			price,
			BOB,
		));

		// Verify exact pUSD amount was burned
		let buyer_final_pusd = Assets::balance(STABLECOIN_ASSET_ID, BOB);
		let actual_burned = buyer_initial_pusd - buyer_final_pusd;

		assert_eq!(actual_burned, expected_owe, "Burned pUSD should equal price * collateral");

		// Verify debt collected callback received same amount
		assert_eq!(get_debt_collected(), expected_owe);
	});
}

/// Verify `NoNewAuctions` allows take and restart but blocks new auctions
#[test]
fn circuit_breaker_no_new_auctions_allows_existing_operations() {
	new_test_ext().execute_with(|| {
		// Start auction while system is open
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Set circuit breaker to NoNewAuctions
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctions);

		// New auction should fail - use assert_err! since start_test_auction
		// mutates storage (creates seized hold) before the actual auction call
		let result = start_test_auction(ALICE, 50 * DOT, 500 * PUSD_UNIT);
		assert_err!(result, Error::<Test>::AuctionsStopped);

		// Take should work
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			10 * DOT,
			price,
			BOB,
		));

		// Advance past tail and verify restart works
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctions);
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(KEEPER),
			auction_id,
			KEEPER,
		));
	});
}

/// Verify `NoNewAuctionsOrRestarts` allows only take
#[test]
fn circuit_breaker_no_new_auctions_or_restarts_allows_only_take() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Set circuit breaker to NoNewAuctionsOrRestarts
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);

		// New auction should fail - use assert_err! since start_test_auction
		// mutates storage before the actual auction call
		let result = start_test_auction(ALICE, 50 * DOT, 500 * PUSD_UNIT);
		assert_err!(result, Error::<Test>::AuctionsStopped);

		// Take should work on non-stale auction
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			10 * DOT,
			price,
			BOB,
		));

		// Advance past tail - auction becomes stale
		run_to_block(21602);

		// Restart should fail due to circuit breaker
		assert_noop!(
			crate::Pallet::<Test>::restart_auction(
				RuntimeOrigin::signed(KEEPER),
				auction_id,
				KEEPER
			),
			Error::<Test>::RestartStopped
		);
	});
}

/// Expected incentives at different DOT prices:
/// | DOT Price | MinLiqDebt | chip (0.1%) | tip  | Total    |
/// |-----------|------------|-------------|------|----------|
/// | $4.00     | 222.22     | 0.22        | 1.00 | 1.22 pUSD|
/// | $1.80     | 100.00     | 0.10        | 1.00 | 1.10 pUSD|
/// | $1.00     | 55.56      | 0.06        | 1.00 | 1.06 pUSD|
#[test]
fn keeper_incentive_matches_example() {
	new_test_ext().execute_with(|| {
		// chip = 0.1% = Permill::from_parts(1000)
		// tip = 1 pUSD
		crate::pallet::AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.chip = Permill::from_parts(1000); // 0.1%
			config.tip = PUSD_UNIT; // 1 pUSD
		});

		// Test case 1: DOT = $4, MinLiqDebt = 222.22 pUSD
		// Expected: tip (1) + chip (0.1% * 222.22) = 1 + 0.22222 ≈ 1.22 pUSD
		{
			let tab = 222_220_000u128; // 222.22 pUSD (6 decimals)
			let config = AuctionConfig::<Test>::get(AuctionType::Liquidation);
			let chip_incentive = config.chip.mul_floor(tab);
			let total_incentive = config.tip + chip_incentive;

			// 0.1% of 222.22 = 0.22222 pUSD = 222220 in 6 decimals
			let expected_chip = 222_220u128; // 0.22222 pUSD
			let expected_total = PUSD_UNIT + expected_chip; // 1.22222 pUSD

			assert_eq!(
				chip_incentive, expected_chip,
				"Chip incentive for 222.22 pUSD tab should be ~0.22 pUSD"
			);
			assert_eq!(total_incentive, expected_total, "Total incentive should be ~1.22 pUSD");
		}

		// Test case 2: DOT = $1.80, MinLiqDebt = 100 pUSD
		// Expected: tip (1) + chip (0.1% * 100) = 1 + 0.10 = 1.10 pUSD
		{
			let tab = 100 * PUSD_UNIT; // 100 pUSD
			let config = AuctionConfig::<Test>::get(AuctionType::Liquidation);
			let chip_incentive = config.chip.mul_floor(tab);
			let total_incentive = config.tip + chip_incentive;

			let expected_chip = 100_000u128; // 0.1 pUSD
			let expected_total = PUSD_UNIT + expected_chip; // 1.10 pUSD

			assert_eq!(
				chip_incentive, expected_chip,
				"Chip incentive for 100 pUSD tab should be 0.10 pUSD"
			);
			assert_eq!(total_incentive, expected_total, "Total incentive should be 1.10 pUSD");
		}

		// Test case 3: DOT = $1.00, MinLiqDebt = 55.56 pUSD
		// Expected: tip (1) + chip (0.1% * 55.56) = 1 + 0.05556 ≈ 1.06 pUSD
		{
			let tab = 55_560_000u128; // 55.56 pUSD
			let config = AuctionConfig::<Test>::get(AuctionType::Liquidation);
			let chip_incentive = config.chip.mul_floor(tab);
			let total_incentive = config.tip + chip_incentive;

			let expected_chip = 55_560u128; // 0.05556 pUSD
			let expected_total = PUSD_UNIT + expected_chip; // 1.05556 pUSD

			assert_eq!(
				chip_incentive, expected_chip,
				"Chip incentive for 55.56 pUSD tab should be ~0.06 pUSD"
			);
			assert_eq!(total_incentive, expected_total, "Total incentive should be ~1.06 pUSD");
		}
	});
}

#[test]
fn restart_auction_pays_keeper_incentive_example_params() {
	new_test_ext().execute_with(|| {
		crate::pallet::AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.chip = Permill::from_parts(1000); // 0.1%
			config.tip = PUSD_UNIT; // 1 pUSD
		});

		let base_tab = 100 * PUSD_UNIT; // 100 pUSD base tab
		let penalty = 10 * PUSD_UNIT;
		let principal = base_tab - penalty;

		create_seized_hold(VAULT_OWNER, 100 * DOT);
		let auction_id = crate::Pallet::<Test>::start_auction(
			VAULT_OWNER,
			100 * DOT,
			DebtComponents::new(principal, 0, penalty),
			KEEPER,
		)
		.unwrap();

		// Record keeper's initial pUSD balance
		let keeper_initial = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);

		// Advance past tail to make auction stale
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Expected incentive: 1 pUSD + 0.1% * base_tab(100) = 1.10 pUSD
		let expected_incentive = PUSD_UNIT + 100_000; // 1.10 pUSD

		// Restart the auction
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(ALICE),
			auction_id,
			KEEPER,
		));

		// Verify keeper has NOT received payment yet (paid at completion)
		let keeper_after_restart = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);
		assert_eq!(
			keeper_after_restart, keeper_initial,
			"Keeper should not receive payment on restart"
		);

		// Verify keeper incentive is stored correctly
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(
			auction.keeper_incentive, expected_incentive,
			"Keeper incentive should be 1.10 pUSD (tip=1 + chip=0.10)"
		);
	});
}

/// Verify cubic price decay matches calculations.
///
/// With `SlowedExponentialDecrease` parameters: center=10, `minimum_price`=0.6
/// The price inflects around the center and eventually hits the floor.
#[test]
fn cubic_price_decay() {
	let curve = PriceCurve::SlowedExponentialDecrease {
		center: 10,
		scale_factor: FixedU128::from(1000),
		linear_coeff: FixedU128::from_rational(65, 10000), // 0.0065
		center_ratio: FixedU128::from_rational(99, 100),   // 0.99
		minimum_price: FixedU128::from_rational(60, 100),  // 60% floor
	};

	let buffer = FixedU128::from_rational(120, 100); // 1.2
	let starting_price = buffer; // 1.2 (oracle = 1.0)

	// Verify block 0: price should be above oracle price (1.0)
	let price_0 = curve.calculate_price(starting_price, buffer, 0u64);
	assert!(price_0 > FixedU128::one(), "Price at t=0 should be above oracle price");

	// Verify block 10 (center): price = oracle × center_ratio = 1.0 × 0.99 = 0.99
	let price_10 = curve.calculate_price(starting_price, buffer, 10u64);
	assert_eq!(price_10, FixedU128::from_rational(99, 100));

	// Verify monotonic decrease
	let price_5 = curve.calculate_price(starting_price, buffer, 5u64);
	let price_15 = curve.calculate_price(starting_price, buffer, 15u64);
	assert!(price_5 < price_0, "Price should decrease over time");
	assert!(price_10 < price_5, "Price should decrease over time");
	assert!(price_15 < price_10, "Price should decrease over time");

	// Verify cusp breach: price should hit floor eventually
	let cusp = FixedU128::from_rational(60, 100); // 0.6
	let cusp_threshold = starting_price.saturating_mul(cusp); // 0.72

	// After many blocks, should be at floor
	let price_100 = curve.calculate_price(starting_price, buffer, 100u64);
	assert_eq!(
		price_100, cusp_threshold,
		"Price should hit floor: actual={}, expected={}",
		price_100, cusp_threshold
	);
}

/// Verify cusp breach detection with parameters
#[test]
fn needs_restart_with_minimum_price() {
	new_test_ext().execute_with(|| {
		// Set config minimum_price to 60%
		crate::pallet::AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.minimum_price = FixedU128::from_rational(60, 100); // 0.6
															 // Also update the curve's minimum_price to be lower so we can actually breach
			config.curve = PriceCurve::SlowedExponentialDecrease {
				center: 10,
				scale_factor: FixedU128::from(1000),
				linear_coeff: FixedU128::from_rational(65, 10000),
				center_ratio: FixedU128::from_rational(99, 100),
				minimum_price: FixedU128::from_rational(30, 100), // 30% floor (below config's 60%)
			};
		});

		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Get initial starting_price and start block
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let initial_starting_price = auction.starting_price;

		// With ExponentialDecrease (decay_factor=0.999), price reaches 60% when:
		// 0.999^n = 0.6 => n = ln(0.6)/ln(0.999) ≈ 511 blocks
		// We need to go past that to breach minimum_price
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(600);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let current_price = crate::Pallet::<Test>::current_price(&auction);

		// Verify price is below minimum_price threshold
		let minimum_price = AuctionConfig::<Test>::get(AuctionType::Liquidation).minimum_price;
		let minimum_price_threshold = initial_starting_price.saturating_mul(minimum_price);

		assert!(
			current_price < minimum_price_threshold,
			"Price {} should be below minimum_price threshold {} at block 600",
			current_price,
			minimum_price_threshold
		);

		// Auction should need restart
		assert!(
			crate::Pallet::<Test>::needs_restart(&auction),
			"Auction should need restart when price < 60% of initial"
		);
	});
}

/// Verify the complete model: incentives + price decay + cusp breach
#[test]
fn full_model_integration() {
	new_test_ext().execute_with(|| {
		crate::pallet::AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.buffer = FixedU128::from_rational(120, 100); // 1.2 (20% above oracle)
			config.minimum_price = FixedU128::from_rational(60, 100); // 0.6
			config.chip = Permill::from_parts(1000); // 0.1%
			config.tip = PUSD_UNIT; // 1 pUSD
						   // Use a curve that allows price to drop below 60%
			config.curve = PriceCurve::SlowedExponentialDecrease {
				center: 10,
				scale_factor: FixedU128::from(1000),
				linear_coeff: FixedU128::from_rational(65, 10000),
				center_ratio: FixedU128::from_rational(99, 100),
				minimum_price: FixedU128::from_rational(30, 100), // 30% floor
			};
		});

		// Set DOT price to $1.80
		set_mock_price(Some(FixedU128::from_rational(180, 100)));

		// With MinCollatRatio=180% and MinVaultSize=100 DOT:
		// MinLiqDebt = 100 DOT * $1.80 / 1.80 = 100 pUSD
		let base_tab = 100 * PUSD_UNIT;
		let penalty = 10 * PUSD_UNIT;
		let principal = base_tab - penalty;
		let collateral = 100 * DOT;

		create_seized_hold(VAULT_OWNER, collateral);
		let auction_id = crate::Pallet::<Test>::start_auction(
			VAULT_OWNER,
			collateral,
			DebtComponents::new(principal, 0, penalty),
			KEEPER,
		)
		.unwrap();

		// Verify initial starting_price = oracle * buffer = 1.8 * 1.2 = 2.16 (normalized)
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let expected_normalized_starting_price = MockCollateralManager::get_dot_price()
			.unwrap()
			.saturating_mul(FixedU128::from_rational(120, 100));
		assert_eq!(auction.starting_price, expected_normalized_starting_price);

		// Record keeper balance and auction start block
		let keeper_initial = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);

		// With ExponentialDecrease (decay_factor=0.999), price reaches 60% when:
		// 0.999^n = 0.6 => n = ln(0.6)/ln(0.999) ≈ 511 blocks
		// We need to go past that to breach minimum_price (60%)
		let target_block = 600;
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(target_block);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Verify auction needs restart due to minimum_price breach
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert!(crate::Pallet::<Test>::needs_restart(&auction));

		// Restart and verify keeper incentive is stored (but not paid yet)
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(ALICE),
			auction_id,
			KEEPER,
		));

		// Keeper should NOT have received payment yet (paid at completion)
		let keeper_after_restart = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);
		assert_eq!(
			keeper_after_restart, keeper_initial,
			"Keeper should not receive payment on restart"
		);

		// Expected: 1 pUSD + 0.1% * 100 pUSD = 1.10 pUSD
		let expected_incentive = PUSD_UNIT + 100_000;

		// Verify auction was reset and keeper incentive stored
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.starting_block, target_block);
		assert!(!crate::Pallet::<Test>::needs_restart(&auction));
		assert_eq!(
			auction.keeper_incentive, expected_incentive,
			"Keeper incentive should be stored as 1.10 pUSD"
		);
		assert_eq!(auction.keeper, KEEPER, "Keeper should be updated");
	});
}

/// Verify that owe calculation rounds UP (ceiling) to protect protocol from bad debt.
///
/// When price * collateral has a fractional component, the buyer should pay
/// at least the true value (rounded up), not less (rounded down).
#[test]
fn owe_calculation_rounds_up_to_minimize_bad_debt() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Advance a few blocks so price has a fractional component
		// that would cause rounding issues
		run_to_block(100);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Record pUSD balance before purchase
		let bob_pusd_before = Assets::balance(STABLECOIN_ASSET_ID, BOB);

		// Buy a specific amount that likely causes fractional calculation
		let buy_amount = 33 * DOT + 333_333_333; // Odd amount to force precision issues

		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			buy_amount,
			price,
			BOB,
		));

		let bob_pusd_after = Assets::balance(STABLECOIN_ASSET_ID, BOB);
		let actual_paid = bob_pusd_before - bob_pusd_after;

		// Calculate what floor (rounding down) would have been
		let floor_owe = price.saturating_mul_int(buy_amount);

		// actual_paid should be >= floor_owe (we rounded UP, not down)
		assert!(
			actual_paid >= floor_owe,
			"Protocol received less than floor value! actual={}, floor={}. \
            This indicates rounding DOWN which accumulates bad debt.",
			actual_paid,
			floor_owe
		);

		// If there was any fractional component, actual_paid should be floor + 1
		let exact_value = price.saturating_mul(FixedU128::saturating_from_integer(buy_amount));
		let floor_as_fixed = FixedU128::saturating_from_integer(floor_owe);

		if exact_value > floor_as_fixed {
			assert_eq!(
				actual_paid,
				floor_owe + 1,
				"When there's precision loss, owe should round UP by exactly 1"
			);
		}
	});
}

/// Verify `MinPurchaseAmount` rejects purchases below threshold
#[test]
fn take_fails_below_min_purchase_amount() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// MinPurchaseAmount is 1 DOT (DOT_UNIT from mock)
		// Try to buy less than MinPurchaseAmount
		let small_amount = DOT / 2; // 0.5 DOT - below min

		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				small_amount,
				price,
				BOB,
			),
			Error::<Test>::PurchaseTooSmall
		);
	});
}

/// Verify `MinPurchaseAmount` allows purchases at exactly threshold
#[test]
fn take_succeeds_at_min_purchase_amount() {
	new_test_ext().execute_with(|| {
		let collateral = 100 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// MinPurchaseAmount is 1 DOT
		let min_amount = DOT; // Exactly minimum

		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			min_amount,
			price,
			BOB,
		));

		// Verify auction updated
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, collateral - min_amount);
	});
}

/// Verify `MinPurchaseAmount` allows buying entire lot even if below threshold
#[test]
fn take_full_lot_succeeds_below_min_purchase_amount() {
	new_test_ext().execute_with(|| {
		// Create auction with small lot below MinPurchaseAmount
		let small_lot = DOT / 2; // 0.5 DOT - below min
		let tab = 10 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, small_lot, tab).unwrap();

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Buying entire lot should succeed even below minimum
		// because it clears the auction
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			small_lot,
			price,
			BOB,
		));

		// Auction should be completed
		assert!(Auctions::<Test>::get(auction_id).is_none());
	});
}

/// Verify auction lifecycle: start -> partial takes -> shortfall completion
#[test]
fn full_auction_lifecycle_with_shortfall() {
	new_test_ext().execute_with(|| {
		// Small collateral, large debt - will result in shortfall
		let collateral = 10 * DOT;
		let tab = 1000 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();

		// Advance time so price drops
		run_to_block(5000);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Buy all remaining collateral
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral,
			price,
			BOB,
		));

		// Auction should be completed
		assert!(Auctions::<Test>::get(auction_id).is_none());

		// Shortfall should be recorded
		let shortfall = get_shortfall_recorded();
		assert!(shortfall > 0, "Should have shortfall when collateral < tab value");

		// Event should show shortfall
		System::assert_has_event(
			Event::AuctionCompleted {
				auction_type: AuctionType::Liquidation,
				id: auction_id,
				remaining: 0,
				shortfall,
			}
			.into(),
		);
	});
}

/// Verify restart with price increase resets top correctly
#[test]
fn restart_with_price_increase_sets_higher_starting_price() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let original_starting_price = auction.starting_price;

		// Advance past maximum_duration
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Increase oracle price by 50%
		let new_price = FixedU128::from_rational(631, 100); // 6.31 USD/DOT (up from 4.21)
		set_mock_price(Some(new_price));

		// Restart auction
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(KEEPER),
			auction_id,
			KEEPER,
		));

		// New starting_price should be higher (based on new price)
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert!(
			auction.starting_price > original_starting_price,
			"New starting_price {} should be higher than original {} after price increase",
			auction.starting_price,
			original_starting_price
		);
	});
}

/// Verify restart with price decrease sets lower `starting_price`
/// Verify auction count tracking via `CountedStorageMap`
#[test]
fn auction_count_tracking() {
	new_test_ext().execute_with(|| {
		// Start 3 auctions
		let id1 = start_test_auction(ALICE, 50 * DOT, 500 * PUSD_UNIT).unwrap();
		let id2 = start_test_auction(BOB, 75 * DOT, 750 * PUSD_UNIT).unwrap();
		let id3 = start_test_auction(CHARLIE, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Verify count
		assert_eq!(Auctions::<Test>::count(), 3);

		// Complete auction 2 (middle)
		let auction2 = Auctions::<Test>::get(id2).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction2);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			id2,
			75 * DOT,
			price,
			BOB,
		));

		// Count should be 2, remaining auctions still exist
		assert_eq!(Auctions::<Test>::count(), 2);
		assert!(Auctions::<Test>::get(id1).is_some());
		assert!(Auctions::<Test>::get(id2).is_none());
		assert!(Auctions::<Test>::get(id3).is_some());
	});
}

/// Verify `AllEnabled` circuit breaker allows all operations
#[test]
fn circuit_breaker_allenabled_allows_all() {
	new_test_ext().execute_with(|| {
		// Ensure AllEnabled (default)
		assert_eq!(Stopped::<Test>::get(), CircuitBreakerLevel::AllEnabled);

		// Start auction
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Take works
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			10 * DOT,
			price,
			BOB,
		));

		// Advance to allow restart
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		run_to_block(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Restart works
		assert_ok!(crate::Pallet::<Test>::restart_auction(
			RuntimeOrigin::signed(KEEPER),
			auction_id,
			KEEPER,
		));

		// New auction works
		let new_id = start_test_auction(ALICE, 50 * DOT, 500 * PUSD_UNIT).unwrap();
		assert_eq!(new_id, 2);
	});
}

/// Verify `AllDisabled` blocks all operations
#[test]
fn circuit_breaker_alldisabled_blocks_all() {
	new_test_ext().execute_with(|| {
		// Start auction first
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();

		// Set to AllDisabled
		Stopped::<Test>::put(CircuitBreakerLevel::AllDisabled);

		// Take fails
		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				auction_id,
				10 * DOT,
				FixedU128::max_value(),
				BOB,
			),
			Error::<Test>::TakeStopped
		);

		// New auction fails
		let result = start_test_auction(ALICE, 50 * DOT, 500 * PUSD_UNIT);
		assert_err!(result, Error::<Test>::AuctionsStopped);

		// Advance to make restart possible
		run_to_block(21602);

		// Restart also fails (restart checks RestartStopped before TakeStopped)
		assert_noop!(
			crate::Pallet::<Test>::restart_auction(
				RuntimeOrigin::signed(KEEPER),
				auction_id,
				KEEPER
			),
			Error::<Test>::RestartStopped
		);
	});
}

/// Verify zero-collateral auction fails or handles gracefully
/// Verify `set_minimum_price` works (config parameter)
/// Verify `set_chip` works (keeper incentive percentage)
/// Verify `set_tip` works (keeper fixed tip)
/// Parameters:
/// - `oracle_price`: 2.25 pUSD/DOT
/// - buffer: 1.2 (`starting_price` = 2.7 pUSD/DOT)
/// - cusp (`minimum_price`): 0.6
/// - `SlowedExponentialDecrease`: `cut_far`=0.97, `cut_near`=0.995, delta=0.05, sharpness=2
///
/// Initial auction:
/// - `auctionable_collateral`: 1000 DOT
/// - `principal_debt`: 1200 pUSD
/// - `accrued_interest`: 50 pUSD
/// - `liquidation_penalty`: 162.5 pUSD
/// - `keeper_incentives`: 2.4125 pUSD (tip=1, chip=0.1%)
/// - tab (without keeper): 1412.5 pUSD
///
/// Takes (from CSV):
/// - Block 11: slice=100 DOT, owe≈223.78 pUSD, remaining principal≈976.22
/// - Block 12: slice=200 DOT, owe≈445.19 pUSD, remaining principal≈531.03
/// - Block 13: slice=200 DOT, owe≈442.48 pUSD, remaining principal≈88.55
/// - Block 14: slice≈137.10 DOT, owe≈301.05 pUSD (final take, principal→0)
///
/// Result: 362.90 DOT returned to vault owner, 637.10 DOT liquidated
#[test]
fn sample_auction() {
	new_test_ext().execute_with(|| {
		// Configure auction parameters to match CSV
		crate::pallet::AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.buffer = FixedU128::from_rational(120, 100); // 1.2
			config.minimum_price = FixedU128::from_rational(60, 100); // 0.6 (cusp)
															 // Keeper incentive: tip=1 pUSD, chip=0.1%
			config.tip = 1_000_000; // 1 pUSD (6 decimals)
			config.chip = Permill::from_parts(1000); // 0.1%
											// SlowedExponentialDecrease curve from CSV
			config.curve = PriceCurve::SlowedExponentialDecrease {
				center: 10,
				scale_factor: FixedU128::from(1000),
				linear_coeff: FixedU128::from_rational(65, 10000), // 0.0065
				center_ratio: FixedU128::from_rational(99, 100),   // 0.99
				minimum_price: FixedU128::from_rational(6, 10),    // cusp = 0.6 (60% floor)
			};
		});

		// Set oracle price to 2.25 pUSD/DOT
		set_mock_price(Some(FixedU128::from_rational(225, 100)));

		// Initial auction parameters (from CSV)
		let collateral = 1000 * DOT;
		let principal = 1200 * PUSD_UNIT;
		let accrued_interest = 50 * PUSD_UNIT;
		let penalty = 162_500_000; // 162.5 pUSD

		// Give VAULT_OWNER extra balance to hold 1000 DOT collateral
		use frame_support::traits::fungible::Mutate;
		let _ = Balances::mint_into(&VAULT_OWNER, collateral);

		// Create seized hold (simulating vaults pallet behavior)
		create_seized_hold(VAULT_OWNER, collateral);

		// Start auction
		let auction_id = crate::Pallet::<Test>::start_auction(
			VAULT_OWNER,
			collateral,
			DebtComponents::new(principal, accrued_interest, penalty),
			KEEPER,
		)
		.unwrap();

		// Verify initial auction state
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		// Keeper incentive = tip + chip * tab = 1 + 0.001 * 1412.5 = 2.4125 pUSD
		assert_eq!(auction.keeper_incentive, 2_412_500);

		// Verify starting price = oracle * buffer = 2.25 * 1.2 = 2.7 (normalized)
		let oracle_price = MockCollateralManager::get_dot_price().unwrap();
		let expected_starting_price =
			oracle_price.saturating_mul(FixedU128::from_rational(120, 100));
		assert_eq!(auction.starting_price, expected_starting_price);

		// Record initial balances
		// CSV uses buyer1=1400, buyer2=3700 but we track deltas since mock has different initial
		// values
		let buyer1 = BOB;
		let buyer2 = CHARLIE;
		let buyer1_initial = Assets::balance(STABLECOIN_ASSET_ID, buyer1);
		let buyer2_initial = Assets::balance(STABLECOIN_ASSET_ID, buyer2);
		let keeper_initial = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);
		let insurance_fund_initial = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);

		// Track cumulative spending
		let mut buyer1_spent: u128 = 0;
		let mut buyer2_spent: u128 = 0;

		// ==== Take 1: elapsed 11 blocks since start (block 12), buyer1 takes 100 DOT, owe≈223.78
		// pUSD ==== CSV t counts elapsed blocks from auction start; auction starts at block 1 in
		// tests.
		run_to_block(12);
		let balance_before = Assets::balance(STABLECOIN_ASSET_ID, buyer1);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(buyer1),
			auction_id,
			100 * DOT,
			FixedU128::saturating_from_integer(10),
			buyer1,
		));
		let balance_after = Assets::balance(STABLECOIN_ASSET_ID, buyer1);
		buyer1_spent += balance_before - balance_after;

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, 900 * DOT);

		// ==== Take 2: elapsed 12 blocks (block 13), buyer2 takes 200 DOT, owe≈445.19 pUSD ====
		run_to_block(13);
		let balance_before = Assets::balance(STABLECOIN_ASSET_ID, buyer2);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(buyer2),
			auction_id,
			200 * DOT,
			FixedU128::saturating_from_integer(10),
			buyer2,
		));
		let balance_after = Assets::balance(STABLECOIN_ASSET_ID, buyer2);
		buyer2_spent += balance_before - balance_after;

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, 700 * DOT);

		// ==== Take 3: elapsed 13 blocks (block 14), buyer2 takes 200 DOT, owe≈442.48 pUSD ====
		run_to_block(14);
		let balance_before = Assets::balance(STABLECOIN_ASSET_ID, buyer2);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(buyer2),
			auction_id,
			200 * DOT,
			FixedU128::saturating_from_integer(10),
			buyer2,
		));
		let balance_after = Assets::balance(STABLECOIN_ASSET_ID, buyer2);
		buyer2_spent += balance_before - balance_after;

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.auctionable_collateral, 500 * DOT);

		// ==== Take 4: elapsed 14 blocks (block 15), buyer1 takes remaining, owe≈301.05 pUSD ====
		run_to_block(15);
		let balance_before = Assets::balance(STABLECOIN_ASSET_ID, buyer1);
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(buyer1),
			auction_id,
			500 * DOT,
			FixedU128::saturating_from_integer(10),
			buyer1,
		));
		let balance_after = Assets::balance(STABLECOIN_ASSET_ID, buyer1);
		buyer1_spent += balance_before - balance_after;

		// Auction should be complete
		assert!(Auctions::<Test>::get(auction_id).is_none());

		// ==== Verify final balances ====

		// Total spent by all buyers should equal tab = 1412.5 pUSD
		let total_spent = buyer1_spent + buyer2_spent;
		assert_eq!(total_spent, 1_412_500_000, "Total spent by buyers");

		// Verify buyer balances decreased by their spending
		let buyer1_final = Assets::balance(STABLECOIN_ASSET_ID, buyer1);
		let buyer2_final = Assets::balance(STABLECOIN_ASSET_ID, buyer2);
		assert_eq!(buyer1_final, buyer1_initial - buyer1_spent);
		assert_eq!(buyer2_final, buyer2_initial - buyer2_spent);

		// Keeper: received 2.4125 pUSD at auction completion (paid from IF)
		let keeper_final = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);
		assert_eq!(keeper_final - keeper_initial, 2_412_500, "Keeper incentive");

		// Insurance Fund: received penalty during takes, paid keeper at completion
		// Net = penalty - keeper = 162.5 - 2.4125 = 160.0875 pUSD
		// Note: Interest is BURNED (not transferred to IF) because interest was
		// already minted to IF upon accrual in the vaults pallet.
		let insurance_fund_final = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);
		let insurance_fund_received = insurance_fund_final - insurance_fund_initial;
		assert_eq!(insurance_fund_received, 160_087_500, "Insurance Fund received");

		// Check that auction completed event was emitted
		// Extract the AuctionCompleted event to verify remaining collateral
		let events = frame_system::Pallet::<Test>::events();
		let auction_completed_event = events.iter().find_map(|record| {
			if let crate::mock::RuntimeEvent::Auctions(Event::AuctionCompleted {
				auction_type: _,
				id,
				remaining,
				shortfall,
			}) = &record.event
			{
				Some((*id, *remaining, *shortfall))
			} else {
				None
			}
		});

		let (event_id, remaining_collateral, shortfall) =
			auction_completed_event.expect("AuctionCompleted event should be emitted");
		assert_eq!(event_id, auction_id);
		assert_eq!(shortfall, 0, "Should have no shortfall");
		// Remaining collateral depends on price curve; verify it's reasonable (some excess)
		assert!(remaining_collateral > 0, "Should have some remaining collateral");
		assert!(
			remaining_collateral < collateral,
			"Remaining should be less than initial collateral"
		);
	});
}

/// Helper to set up surplus auction prerequisites
fn setup_surplus_auction_conditions() {
	use frame_support::traits::fungibles::Mutate as FungiblesMutate;

	// Set surplus mode to Auction (since default is DirectTransfer)
	SurplusMode::<Test>::put(SurplusHandlingMode::Auction);

	// Set DOT price (e.g., $10 per DOT)
	set_mock_price(Some(FixedU128::saturating_from_integer(10)));

	// Set IF balance and pUSD supply such that surplus threshold is met
	// Threshold is 5%, so if supply is 1M pUSD, IF needs > 5% = 50,000 pUSD
	// Plus auction amount (10,000 pUSD), so IF needs > 60,000 pUSD
	set_mock_pusd_supply(1_000_000 * PUSD_UNIT);
	set_mock_if_balance(100_000 * PUSD_UNIT); // 10% surplus, well above threshold

	// Also give IF actual pUSD tokens for transfers
	Assets::mint_into(STABLECOIN_ASSET_ID, &INSURANCE_FUND, 100_000 * PUSD_UNIT).unwrap();
}

#[test]
fn start_surplus_auction_works() {
	new_test_ext().execute_with(|| {
		setup_surplus_auction_conditions();

		// Start a surplus auction
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));

		// Verify auction was created
		let auction = Auctions::<Test>::get(1).unwrap();
		assert_eq!(auction.auction_type, AuctionType::Surplus);

		// Verify ActiveSurplusAuctionId is set
		assert_eq!(crate::pallet::ActiveSurplusAuctionId::<Test>::get(), Some(1));
	});
}

#[test]
fn only_one_surplus_auction_allowed() {
	new_test_ext().execute_with(|| {
		setup_surplus_auction_conditions();

		// Start first surplus auction
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));

		// Verify ActiveSurplusAuctionId is set
		assert_eq!(crate::pallet::ActiveSurplusAuctionId::<Test>::get(), Some(1));

		// Attempt to start second surplus auction should fail
		assert_noop!(
			crate::Pallet::<Test>::start_surplus_auction(RuntimeOrigin::signed(BOB), BOB),
			Error::<Test>::SurplusAuctionAlreadyActive
		);

		// Original auction should still be active
		assert!(Auctions::<Test>::get(1).is_some());
		assert_eq!(crate::pallet::ActiveSurplusAuctionId::<Test>::get(), Some(1));
	});
}

#[test]
fn surplus_auction_completion_allows_new_auction() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::fungible::Mutate as FungibleMutate2;

		setup_surplus_auction_conditions();

		// Give buyer DOT to pay for pUSD
		Balances::mint_into(&BOB, 10_000 * DOT).unwrap();

		// Start first surplus auction
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));
		let first_auction_id = 1;

		// Verify active surplus auction is set
		assert_eq!(crate::pallet::ActiveSurplusAuctionId::<Test>::get(), Some(first_auction_id));

		// Get auction and current price
		let auction = Auctions::<Test>::get(first_auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Buy all pUSD in the surplus auction (auction amount is 10,000 pUSD)
		let pusd_amount = 10_000 * PUSD_UNIT;
		assert_ok!(crate::Pallet::<Test>::take_surplus(
			RuntimeOrigin::signed(BOB),
			first_auction_id,
			pusd_amount,
			price,
			BOB
		));

		// Auction should be completed and removed
		assert!(Auctions::<Test>::get(first_auction_id).is_none());

		// ActiveSurplusAuctionId should be cleared
		assert!(crate::pallet::ActiveSurplusAuctionId::<Test>::get().is_none());

		// Now we should be able to start a new surplus auction
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));

		// New auction should be created with next ID
		let second_auction_id = 2;
		assert!(Auctions::<Test>::get(second_auction_id).is_some());
		assert_eq!(crate::pallet::ActiveSurplusAuctionId::<Test>::get(), Some(second_auction_id));
	});
}

/// Verify keeper payment is capped to `penalty_collected` when auction ends with shortfall.
///
/// Scenario: Auction has large `keeper_incentive` but collateral runs out before
/// full penalty is collected. Keeper should only receive `penalty_collected`, not
/// the full `keeper_incentive`.
#[test]
fn keeper_payment_capped_to_penalty_collected_on_shortfall() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::fungible::Mutate as FungibleMutate2;

		// Configure auction with significant keeper incentive
		AuctionConfig::<Test>::mutate(AuctionType::Liquidation, |config| {
			config.tip = 10 * PUSD_UNIT; // 10 pUSD flat fee
			config.chip = Permill::from_percent(1); // 1% of tab
		});

		// Set oracle price: 1 DOT = 2 pUSD
		set_mock_price(Some(FixedU128::from_u32(2)));

		// Create auction with:
		// - Small collateral: 10 DOT (worth ~20 pUSD at oracle price)
		// - Large principal: 100 pUSD
		// - Large penalty: 50 pUSD
		// This guarantees shortfall - collateral can't cover full tab
		let collateral = 10 * DOT;
		let principal = 100 * PUSD_UNIT;
		let accrued_interest = 0;
		let penalty = 50 * PUSD_UNIT;
		let tab = principal + penalty; // 150 pUSD

		// Expected keeper_incentive = tip + chip * tab = 10 + 1% * 150 = 11.5 pUSD
		// But capped to penalty = 50 pUSD, so keeper_incentive = 11.5 pUSD
		let expected_keeper_incentive = 10 * PUSD_UNIT + Permill::from_percent(1).mul_floor(tab);

		// Give vault owner collateral
		Balances::mint_into(&VAULT_OWNER, collateral).unwrap();
		create_seized_hold(VAULT_OWNER, collateral);

		// Start auction
		let auction_id = crate::Pallet::<Test>::start_auction(
			VAULT_OWNER,
			collateral,
			DebtComponents::new(principal, accrued_interest, penalty),
			KEEPER,
		)
		.unwrap();

		// Verify keeper_incentive is set
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		assert_eq!(auction.keeper_incentive, expected_keeper_incentive);
		assert_eq!(auction.penalty_collected, 0, "No penalty collected yet");

		// Record keeper's initial balance
		let keeper_initial = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);

		// Advance time so price drops significantly
		// With buffer=1.2 and price=2, starting_price = 2.4 pUSD/DOT
		// After time, price will drop making collateral worth less
		run_to_block(100);

		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		// Take all collateral - this will result in shortfall
		// since collateral value < tab
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral, // Take all 10 DOT
			price,
			BOB,
		));

		// Auction should be completed (collateral exhausted)
		assert!(Auctions::<Test>::get(auction_id).is_none());

		// Shortfall should be recorded (principal not fully covered)
		let shortfall = get_shortfall_recorded();
		assert!(shortfall > 0, "Should have shortfall");

		// Calculate how much pUSD was actually paid by buyer
		// owe = collateral * price (this is how much buyer paid)
		let owe = price
			.saturating_mul(FixedU128::saturating_from_integer(collateral))
			.ceil()
			.saturating_mul_int(1u128);

		// Payment priority: principal -> interest -> penalty
		// If owe < principal, no penalty collected
		// If owe > principal, penalty_collected = min(owe - principal, penalty)
		let penalty_collected = if owe > principal { (owe - principal).min(penalty) } else { 0 };

		// Keeper should receive min(keeper_incentive, penalty_collected)
		let expected_keeper_payment = expected_keeper_incentive.min(penalty_collected);

		let keeper_final = Assets::balance(STABLECOIN_ASSET_ID, KEEPER);
		let keeper_received = keeper_final.saturating_sub(keeper_initial);

		assert_eq!(
			keeper_received, expected_keeper_payment,
			"Keeper should receive min(keeper_incentive={}, penalty_collected={})",
			expected_keeper_incentive, penalty_collected
		);

		// If penalty_collected < keeper_incentive, keeper was capped
		if penalty_collected < expected_keeper_incentive {
			assert!(
				keeper_received < expected_keeper_incentive,
				"Keeper payment should be capped when penalty_collected < keeper_incentive"
			);
		}
	});
}

#[test]
fn set_surplus_mode_works() {
	new_test_ext().execute_with(|| {
		// Default should be DirectTransfer mode (sends surplus to DAP)
		assert_eq!(SurplusMode::<Test>::get(), SurplusHandlingMode::DirectTransfer);

		// Change to Auction mode (requires Root origin)
		assert_ok!(crate::Pallet::<Test>::set_surplus_mode(
			RuntimeOrigin::root(),
			SurplusHandlingMode::Auction
		));

		// Verify mode changed
		assert_eq!(SurplusMode::<Test>::get(), SurplusHandlingMode::Auction);

		// Verify event emitted
		System::assert_has_event(
			Event::SurplusModeUpdated { mode: SurplusHandlingMode::Auction }.into(),
		);

		// Change back to DirectTransfer mode
		assert_ok!(crate::Pallet::<Test>::set_surplus_mode(
			RuntimeOrigin::root(),
			SurplusHandlingMode::DirectTransfer
		));
		assert_eq!(SurplusMode::<Test>::get(), SurplusHandlingMode::DirectTransfer);
	});
}

#[test]
fn set_surplus_mode_requires_root() {
	new_test_ext().execute_with(|| {
		// Signed origin should fail
		assert_noop!(
			crate::Pallet::<Test>::set_surplus_mode(
				RuntimeOrigin::signed(ALICE),
				SurplusHandlingMode::Auction
			),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn start_surplus_auction_fails_in_direct_transfer_mode() {
	new_test_ext().execute_with(|| {
		// Set up sufficient IF balance for surplus
		set_mock_if_balance(1_000_000 * PUSD_UNIT);
		set_mock_pusd_supply(10_000_000 * PUSD_UNIT);

		// Default is DirectTransfer mode - surplus auction should fail
		assert_eq!(SurplusMode::<Test>::get(), SurplusHandlingMode::DirectTransfer);

		// Try to start a surplus auction - should fail
		assert_noop!(
			crate::Pallet::<Test>::start_surplus_auction(RuntimeOrigin::signed(ALICE), KEEPER),
			Error::<Test>::SurplusAuctionsDisabled
		);
	});
}

#[test]
fn transfer_surplus_works() {
	new_test_ext().execute_with(|| {
		// Set up sufficient IF balance for surplus
		let if_balance = 1_000_000 * PUSD_UNIT;
		let pusd_supply = 10_000_000 * PUSD_UNIT;
		set_mock_if_balance(if_balance);
		set_mock_pusd_supply(pusd_supply);

		// Mint pUSD to IF account (so the mock transfer works)
		<Assets as frame_support::traits::fungibles::Mutate<_>>::mint_into(
			STABLECOIN_ASSET_ID,
			&INSURANCE_FUND,
			if_balance,
		)
		.unwrap();

		// Default is DirectTransfer mode - transfer should work
		assert_eq!(SurplusMode::<Test>::get(), SurplusHandlingMode::DirectTransfer);

		// Record initial Treasury balance
		let treasury_initial = Assets::balance(STABLECOIN_ASSET_ID, TREASURY);

		// Transfer surplus (anyone can trigger)
		assert_ok!(crate::Pallet::<Test>::transfer_surplus(RuntimeOrigin::signed(ALICE)));

		// Verify treasury received the transfer amount
		let surplus_amount = 10_000 * PUSD_UNIT; // SurplusAuctionAmount from mock
		let treasury_final = Assets::balance(STABLECOIN_ASSET_ID, TREASURY);
		assert_eq!(treasury_final - treasury_initial, surplus_amount);

		// Verify event emitted
		System::assert_has_event(Event::SurplusTransferred { amount: surplus_amount }.into());
	});
}

#[test]
fn transfer_surplus_fails_in_auction_mode() {
	new_test_ext().execute_with(|| {
		// Set up sufficient IF balance for surplus
		set_mock_if_balance(1_000_000 * PUSD_UNIT);
		set_mock_pusd_supply(10_000_000 * PUSD_UNIT);

		// Change to Auction mode
		assert_ok!(crate::Pallet::<Test>::set_surplus_mode(
			RuntimeOrigin::root(),
			SurplusHandlingMode::Auction
		));

		// In Auction mode - transfer should fail
		assert_noop!(
			crate::Pallet::<Test>::transfer_surplus(RuntimeOrigin::signed(ALICE)),
			Error::<Test>::DirectTransferDisabled
		);
	});
}

#[test]
fn transfer_surplus_respects_threshold() {
	new_test_ext().execute_with(|| {
		// Set up IF balance that is insufficient after transfer
		// Threshold is 5%, transfer is 10,000 pUSD
		// After transfer, IF must still have >= 5% of supply
		let pusd_supply = 10_000_000 * PUSD_UNIT;
		let threshold_amount = pusd_supply * 5 / 100; // 500,000 pUSD
		let transfer_amount = 10_000 * PUSD_UNIT;

		// Set IF balance to exactly threshold + transfer - 1 (so after transfer, below threshold)
		let if_balance = threshold_amount + transfer_amount - 1;
		set_mock_if_balance(if_balance);
		set_mock_pusd_supply(pusd_supply);

		// Default is DirectTransfer mode - transfer should fail due to insufficient surplus
		assert_noop!(
			crate::Pallet::<Test>::transfer_surplus(RuntimeOrigin::signed(ALICE)),
			Error::<Test>::InsufficientSurplus
		);

		// Now set sufficient balance
		let sufficient_if_balance = threshold_amount + transfer_amount + 1;
		set_mock_if_balance(sufficient_if_balance);

		// Mint pUSD to IF account
		<Assets as frame_support::traits::fungibles::Mutate<_>>::mint_into(
			STABLECOIN_ASSET_ID,
			&INSURANCE_FUND,
			sufficient_if_balance,
		)
		.unwrap();

		// Transfer should work now
		assert_ok!(crate::Pallet::<Test>::transfer_surplus(RuntimeOrigin::signed(ALICE)));
	});
}

#[test]
fn transfer_surplus_respects_circuit_breaker() {
	new_test_ext().execute_with(|| {
		// Set up sufficient IF balance
		let if_balance = 1_000_000 * PUSD_UNIT;
		set_mock_if_balance(if_balance);
		set_mock_pusd_supply(10_000_000 * PUSD_UNIT);

		// Default is DirectTransfer mode - set circuit breaker to NoNewAuctions
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctions);

		// Transfer should fail
		assert_noop!(
			crate::Pallet::<Test>::transfer_surplus(RuntimeOrigin::signed(ALICE)),
			Error::<Test>::AuctionsStopped
		);
	});
}

#[test]
fn take_liquidation_fails_on_surplus_auction() {
	new_test_ext().execute_with(|| {
		setup_surplus_auction_conditions();
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));

		let auction = Auctions::<Test>::get(1).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		assert_noop!(
			crate::Pallet::<Test>::take_liquidation(
				RuntimeOrigin::signed(BOB),
				1,
				10 * DOT,
				price,
				BOB
			),
			Error::<Test>::InvalidAuctionType
		);
	});
}

#[test]
fn take_surplus_fails_on_liquidation_auction() {
	new_test_ext().execute_with(|| {
		let auction_id = start_test_auction(VAULT_OWNER, 100 * DOT, 1000 * PUSD_UNIT).unwrap();
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		assert_noop!(
			crate::Pallet::<Test>::take_surplus(
				RuntimeOrigin::signed(BOB),
				auction_id,
				100 * PUSD_UNIT,
				price,
				BOB
			),
			Error::<Test>::InvalidAuctionType
		);
	});
}

#[test]
fn restart_auction_fails_on_surplus_auction() {
	new_test_ext().execute_with(|| {
		setup_surplus_auction_conditions();
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));

		// Make stale without triggering on_idle
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		System::set_block_number(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		assert_noop!(
			crate::Pallet::<Test>::restart_auction(RuntimeOrigin::signed(KEEPER), 1, KEEPER),
			Error::<Test>::InvalidAuctionType
		);
	});
}

#[test]
fn on_idle_processes_multiple_stale_auctions() {
	new_test_ext().execute_with(|| {
		// Create 5 auctions
		for i in 1..=5 {
			let owner = 100 + i as u64;
			let _ = Balances::mint_into(&owner, INITIAL_BALANCE);
			create_seized_hold(owner, 10 * DOT);
			crate::Pallet::<Test>::start_auction(
				owner,
				10 * DOT,
				DebtComponents::new(100 * PUSD_UNIT, 0, 10 * PUSD_UNIT),
				KEEPER,
			)
			.unwrap();
		}

		// Make stale, then enable on_idle
		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		System::set_block_number(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		crate::Pallet::<Test>::on_idle(21602, Weight::from_parts(u64::MAX, u64::MAX));

		// All should be restarted
		for id in 1..=5u32 {
			assert_eq!(Auctions::<Test>::get(id).unwrap().starting_block, 21602);
		}
	});
}

#[test]
fn on_idle_cursor_pagination_across_blocks() {
	new_test_ext().execute_with(|| {
		// Create 150 auctions (more than MaxOnIdleItems=100)
		for i in 1..=150 {
			let owner = 100 + i as u64;
			let _ = Balances::mint_into(&owner, INITIAL_BALANCE);
			create_seized_hold(owner, 10 * DOT);
			crate::Pallet::<Test>::start_auction(
				owner,
				10 * DOT,
				DebtComponents::new(100 * PUSD_UNIT, 0, 0),
				KEEPER,
			)
			.unwrap();
		}

		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		System::set_block_number(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// First pass: processes 100, sets cursor
		crate::Pallet::<Test>::on_idle(21602, Weight::from_parts(u64::MAX, u64::MAX));
		assert!(OnIdleCursor::<Test>::get().is_some());

		// Second pass: processes remaining 50, clears cursor
		System::set_block_number(21603);
		crate::Pallet::<Test>::on_idle(21603, Weight::from_parts(u64::MAX, u64::MAX));
		assert!(OnIdleCursor::<Test>::get().is_none());
	});
}

#[test]
fn on_idle_completes_stale_surplus_auction() {
	new_test_ext().execute_with(|| {
		setup_surplus_auction_conditions();
		assert_ok!(crate::Pallet::<Test>::start_surplus_auction(
			RuntimeOrigin::signed(ALICE),
			KEEPER
		));

		let auction = Auctions::<Test>::get(1).unwrap();
		let initial_tab = auction.tab.principal;

		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		System::set_block_number(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		crate::Pallet::<Test>::on_idle(21602, Weight::from_parts(u64::MAX, u64::MAX));

		// Surplus auction should be COMPLETED (removed), not restarted
		assert!(Auctions::<Test>::get(1).is_none());
		assert!(ActiveSurplusAuctionId::<Test>::get().is_none());

		System::assert_has_event(
			Event::AuctionCompleted {
				auction_type: AuctionType::Surplus,
				id: 1,
				remaining: initial_tab,
				shortfall: 0,
			}
			.into(),
		);
	});
}

#[test]
fn take_caps_owe_when_collateral_exceeds_debt() {
	new_test_ext().execute_with(|| {
		// Large collateral (100 DOT ~= 505 pUSD at buffered price), small debt (50 pUSD)
		let collateral = 100 * DOT;
		let tab = 50 * PUSD_UNIT;

		let auction_id = start_test_auction(VAULT_OWNER, collateral, tab).unwrap();
		let auction = Auctions::<Test>::get(auction_id).unwrap();
		let price = crate::Pallet::<Test>::current_price(&auction);

		let bob_pusd_before = Assets::balance(STABLECOIN_ASSET_ID, BOB);

		// Request all collateral, but owe should cap at tab
		assert_ok!(crate::Pallet::<Test>::take_liquidation(
			RuntimeOrigin::signed(BOB),
			auction_id,
			collateral,
			price,
			BOB
		));

		let paid = bob_pusd_before - Assets::balance(STABLECOIN_ASSET_ID, BOB);
		assert_eq!(paid, tab, "Should pay exactly tab, not full collateral value");
		assert!(Auctions::<Test>::get(auction_id).is_none(), "Auction completed");
	});
}

#[test]
fn needs_restart_true_for_zero_starting_price() {
	new_test_ext().execute_with(|| {
		let auction = Auction::<Test> {
			auction_type: AuctionType::Liquidation,
			tab: Tab::new(100 * PUSD_UNIT, 0, 0),
			auctionable_collateral: 100 * DOT,
			vault_owner: Some(VAULT_OWNER),
			starting_block: 1,
			starting_price: FixedU128::zero(),
			keeper: KEEPER,
			keeper_incentive: 0,
			penalty_collected: 0,
		};

		assert!(crate::Pallet::<Test>::needs_restart(&auction));
	});
}

#[test]
fn on_idle_respects_weight_limit() {
	new_test_ext().execute_with(|| {
		for i in 1..=10 {
			let owner = 100 + i as u64;
			let _ = Balances::mint_into(&owner, INITIAL_BALANCE);
			create_seized_hold(owner, 10 * DOT);
			crate::Pallet::<Test>::start_auction(
				owner,
				10 * DOT,
				DebtComponents::new(100 * PUSD_UNIT, 0, 10 * PUSD_UNIT),
				KEEPER,
			)
			.unwrap();
		}

		Stopped::<Test>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);
		System::set_block_number(21602);
		Stopped::<Test>::put(CircuitBreakerLevel::AllEnabled);

		// Minimal weight - only enough for base overhead
		let minimal = crate::Pallet::<Test>::on_idle_weight();
		crate::Pallet::<Test>::on_idle(21602, minimal);

		// Should have set cursor (couldn't process all)
		// Or processed none if weight insufficient
		let restarted = (1..=10u32)
			.filter(|id| Auctions::<Test>::get(*id).is_some_and(|a| a.starting_block == 21602))
			.count();

		assert!(restarted < 10, "Should not process all with minimal weight");
	});
}
