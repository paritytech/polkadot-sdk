#![cfg(test)]

use crate::{
	mock::*,
	pallet::{AuctionClearingPrice, Configuration, CurrentPhase, SaleInfo},
	InitData, SalePhase,
};
use frame_support::weights::WeightMeter;
use pallet_broker::{
	market::{AdjustBidResult, Market, OrderResult, RenewalOrderResult, TickAction},
	PotentialRenewalId, Timeslice,
};

type CoretimeMarket = crate::Pallet<Test>;
type Error = crate::pallet::Error<Test>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn start_sales(reserve_price: u64) {
	let init = InitData { reserve_price };
	<CoretimeMarket as Market<u64, u64, u64>>::start_sales(0, init)
		.expect("start_sales should succeed");
}

fn tick(block_number: u64) -> Vec<TickAction<u64, u64, u64>> {
	let mut meter = WeightMeter::new();
	<CoretimeMarket as Market<u64, u64, u64>>::tick(block_number, &mut meter)
}

fn tick_with_ts(block_number: u64, latest_ready_ts: Timeslice) -> Vec<TickAction<u64, u64, u64>> {
	TestTimesliceProvider::set_latest_ready(latest_ready_ts);
	tick(block_number)
}

fn place_bid(
	block_number: u64,
	who: u64,
	price_limit: u64,
) -> Result<OrderResult<u64, u32>, Error> {
	<CoretimeMarket as Market<u64, u64, u64>>::place_order(block_number, &who, price_limit)
}

fn place_renewal(
	block_number: u64,
	who: u64,
	core: u16,
	when: u32,
) -> Result<RenewalOrderResult<u64, u32>, Error> {
	let renewal_id = PotentialRenewalId { core, when };
	<CoretimeMarket as Market<u64, u64, u64>>::place_renewal_order(
		block_number, &who, renewal_id,
	)
}

fn adjust_bid(
	block_number: u64,
	id: u32,
	who: u64,
	new_price: Option<u64>,
) -> Result<AdjustBidResult<u64>, Error> {
	<CoretimeMarket as Market<u64, u64, u64>>::adjust_bid(block_number, id, &who, new_price)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn start_sales_works() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		assert!(SaleInfo::<Test>::get().is_some());
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Market));
	});
}

#[test]
fn place_bid_works() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let result = place_bid(0, 1, 500).expect("bid should succeed");
		match result {
			OrderResult::BidPlaced { id, bid_price } => {
				assert_eq!(id, 0);
				// Bid price should be min(price_limit, current_price).
				// At block 0, current_price = opening_price = reserve * multiplier = 200.
				assert_eq!(bid_price, 200);
			},
			_ => panic!("Expected BidPlaced"),
		}
	});
}

#[test]
fn place_bid_wrong_phase() {
	TestExt::new().execute_with(|| {
		// No sales started.
		assert!(place_bid(0, 1, 100).is_err());
	});
}

#[test]
fn adjust_bid_raise_works() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, bid_price: _ } =
			place_bid(0, 1, 150).unwrap() else { panic!() };

		// Raise bid (still within descending price at block 0 = 200).
		let result = adjust_bid(0, id, 1, Some(180)).unwrap();
		match result {
			AdjustBidResult::Lock { amount } => {
				assert_eq!(amount, 30); // 180 - 150
			},
			_ => panic!("Expected Lock"),
		}
	});
}

#[test]
fn adjust_bid_withdraw_not_allowed() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		let OrderResult::BidPlaced { id, .. } = place_bid(0, 1, 150).unwrap() else { panic!() };

		// Withdrawal should fail (RFC-17: binding bids).
		assert!(matches!(adjust_bid(0, id, 1, None), Err(Error::NotAllowed)));
	});
}

#[test]
fn auction_settles_on_tick() {
	TestExt::new().execute_with(|| {
		start_sales(100);

		// Place 2 bids (2 cores offered).
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 150).unwrap();

		// Tick at market_end (market_period = 20).
		let actions = tick(20);

		// Should contain settlement actions (refunds) and ProcessAutoRenewals.
		assert!(CurrentPhase::<Test>::get() == Some(SalePhase::Renewal));
		assert!(AuctionClearingPrice::<Test>::get().is_some());

		let has_process = actions.iter().any(|a| matches!(a, TickAction::ProcessAutoRenewals { .. }));
		assert!(has_process, "Should have ProcessAutoRenewals action");
	});
}

#[test]
fn full_sale_cycle() {
	TestExt::new().execute_with(|| {
		start_sales(100);

		// Market phase: place bids.
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 2, 180).unwrap();

		// Tick past market_end (20) → settles auction, transitions to Renewal.
		let _actions = tick(20);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Renewal));

		// Tick past renewal_end (20 + 10 = 30) → finalizes sale, transitions to Settlement.
		let actions = tick(30);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Settlement));

		// Should have SellRegion actions for the 2 winners.
		let sell_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::SellRegion { .. }))
			.count();
		assert_eq!(sell_count, 2);

		// Tick past region_begin with committed timeslice → rotates to new Market.
		let sale = SaleInfo::<Test>::get().unwrap();
		let actions = tick_with_ts(35, sale.region_begin);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Market));

		let has_rotated = actions.iter().any(|a| matches!(a, TickAction::SaleRotated { .. }));
		assert!(has_rotated, "Should have SaleRotated action");
	});
}

#[test]
fn renewal_during_renewal_phase() {
	TestExt::new().execute_with(|| {
		start_sales(100);

		// Place 1 bid (out of 2 cores offered).
		place_bid(0, 1, 200).unwrap();

		// Settle auction.
		tick(20);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Renewal));

		let sale = SaleInfo::<Test>::get().unwrap();

		// Give user 2 a renewal right for this sale.
		TestRenewalRights::set(2, sale.region_begin, 1);

		// Renew — should succeed since 1 of 2 cores allocated and user has renewal right.
		let result = place_renewal(25, 2, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { price, region_id, effective_to } => {
				assert!(price > 0);
				assert_eq!(region_id.begin, sale.region_begin);
				assert_eq!(effective_to, sale.region_end);
			},
			_ => panic!("Expected Renewed"),
		}
	});
}

/// The region_begin of the first sale with default config.
/// Computed as: old_region_end = commit_ts + region_length = (0+2)/2 + 3 = 4, new_begin = 4.
const FIRST_REGION_BEGIN: Timeslice = 4;

/// Helper: run a sale through Market→Renewal, returning the sale info at Renewal phase.
fn setup_renewal_phase(bids: &[(u64, u64)]) -> crate::SaleInfoRecord<u64, u64> {
	start_sales(100);
	for &(who, price) in bids {
		place_bid(0, who, price).unwrap();
	}
	tick(20); // settle auction → Renewal
	assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Renewal));
	SaleInfo::<Test>::get().unwrap()
}

#[test]
fn renewal_without_rights_fails() {
	TestExt::new().execute_with(|| {
		let sale = setup_renewal_phase(&[(1, 200)]);

		// User 2 has NO renewal rights.
		let result = place_renewal(25, 2, 0, sale.region_begin);
		assert!(matches!(result, Err(Error::Unavailable)));
	});
}

#[test]
fn renewal_wrong_phase_fails() {
	TestExt::new().execute_with(|| {
		start_sales(100);
		// Still in Market phase.
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Market));

		TestRenewalRights::set(1, 3, 1);
		let result = place_renewal(5, 1, 0, 3);
		assert!(matches!(result, Err(Error::WrongPhase)));
	});
}

#[test]
fn double_renewal_prevented() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 0 bids → both free for renewal.
		let sale = setup_renewal_phase(&[]);

		// User 1 has exactly 1 renewal right.
		TestRenewalRights::set(1, sale.region_begin, 1);

		// First renewal succeeds.
		assert!(place_renewal(25, 1, 0, sale.region_begin).is_ok());

		// Second renewal fails — right already consumed.
		let result = place_renewal(25, 1, 0, sale.region_begin);
		assert!(matches!(result, Err(Error::Unavailable)));
	});
}

#[test]
fn multiple_renewal_rights_respected() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 0 bids → both free for renewal.
		let sale = setup_renewal_phase(&[]);

		// User 1 has 2 renewal rights.
		TestRenewalRights::set(1, sale.region_begin, 2);

		// Both renewals succeed.
		assert!(place_renewal(25, 1, 0, sale.region_begin).is_ok());
		assert!(place_renewal(25, 1, 1, sale.region_begin).is_ok());

		// Third fails — only had 2 rights.
		assert!(matches!(
			place_renewal(25, 1, 2, sale.region_begin),
			Err(Error::Unavailable)
		));
	});
}

#[test]
fn renewal_emits_renew_region_in_finalize() {
	TestExt::new().execute_with(|| {
		// 2 cores, 1 bid.
		let sale = setup_renewal_phase(&[(1, 200)]);

		// User 2 renews.
		TestRenewalRights::set(2, sale.region_begin, 1);
		place_renewal(25, 2, 0, sale.region_begin).unwrap();

		// Finalize (tick past renewal_end = 30).
		let actions = tick(30);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Settlement));

		// Should have SellRegion for the auction winner.
		let sell_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::SellRegion { .. }))
			.count();
		assert_eq!(sell_count, 1);

		// Should have RenewRegion for the renewal.
		let renew_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::RenewRegion { .. }))
			.count();
		assert_eq!(renew_count, 1);

		// Verify the RenewRegion has correct owner.
		let renew_action = actions
			.iter()
			.find(|a| matches!(a, TickAction::RenewRegion { .. }))
			.unwrap();
		match renew_action {
			TickAction::RenewRegion { owner, .. } => assert_eq!(*owner, 2),
			_ => unreachable!(),
		}
	});
}

#[test]
fn displacement_works_when_oversubscribed() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 2 bids → fully subscribed.
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		// User 3 has renewal right, users 1 and 2 do NOT.
		TestRenewalRights::set(3, sale.region_begin, 1);

		// Renew should succeed via displacement.
		let result = place_renewal(25, 3, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { price, region_id, effective_to } => {
				assert!(price > 0);
				assert_eq!(region_id.begin, sale.region_begin);
				assert_eq!(effective_to, sale.region_end);
			},
			_ => panic!("Expected Renewed via displacement"),
		}

		// Finalize — should have 1 SellRegion (remaining winner), 1 RenewRegion,
		// and 1 Refund (displaced bidder).
		let actions = tick(30);

		let sell_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::SellRegion { .. }))
			.count();
		let renew_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::RenewRegion { .. }))
			.count();
		let refund_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::Refund { .. }))
			.count();

		assert_eq!(sell_count, 1, "1 remaining auction winner");
		assert_eq!(renew_count, 1, "1 renewal");
		assert_eq!(refund_count, 1, "1 displaced refund");
	});
}

#[test]
fn displacement_targets_lowest_non_tenant_bidder() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 2 bids: user 1 at 200, user 2 at 150.
		// Neither is an existing tenant.
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		TestRenewalRights::set(3, sale.region_begin, 1);

		// User 3 renews → should displace user 2 (lowest non-tenant bid).
		place_renewal(25, 3, 0, sale.region_begin).unwrap();

		let actions = tick(30);

		// The displaced refund should be for user 2 (lower bid).
		let refund = actions.iter().find(|a| matches!(a, TickAction::Refund { .. }));
		match refund {
			Some(TickAction::Refund { who, .. }) => assert_eq!(*who, 2),
			_ => panic!("Expected refund for displaced user 2"),
		}
	});
}

#[test]
fn existing_tenant_protected_from_displacement() {
	TestExt::new().execute_with(|| {
		// 2 cores offered. User 1 (tenant) has the LOWER bid. Without protection,
		// they'd be the displacement target.
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 1);
		let sale = setup_renewal_phase(&[(1, 150), (2, 200)]);

		TestRenewalRights::set(3, sale.region_begin, 1);

		// User 3 renews → should displace user 2 (non-tenant), NOT user 1 (tenant),
		// even though user 1 has the lower bid.
		place_renewal(25, 3, 0, sale.region_begin).unwrap();

		let actions = tick(30);

		// User 2 should be displaced (non-tenant) despite having the higher bid.
		let refund = actions.iter().find(|a| matches!(a, TickAction::Refund { .. }));
		match refund {
			Some(TickAction::Refund { who, .. }) => assert_eq!(*who, 2),
			_ => panic!("Expected refund for non-tenant user 2"),
		}

		// User 1 should still have a SellRegion (protected tenant).
		let sell = actions.iter().find(|a| matches!(a, TickAction::SellRegion { owner, .. } if *owner == 1));
		assert!(sell.is_some(), "Tenant user 1 should keep their allocation");
	});
}

#[test]
fn displacement_fails_when_all_winners_are_tenants() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 2 bids. Both bidders are existing tenants.
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 1);
		TestRenewalRights::set(2, FIRST_REGION_BEGIN, 1);
		let sale = setup_renewal_phase(&[(1, 200), (2, 150)]);

		// User 3 is a tenant wanting to renew, but all auction winners are tenants too.
		TestRenewalRights::set(3, sale.region_begin, 1);

		let result = place_renewal(25, 3, 0, sale.region_begin);
		assert!(matches!(result, Err(Error::Unavailable)));
	});
}

#[test]
fn renewal_quota_reduced_by_auction_wins() {
	TestExt::new().execute_with(|| {
		// 3 cores offered. User 1 is an existing tenant with 3 renewal rights.
		TestCoreRangeProvider::set(0, 3);
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 3);

		// User 1 wins 2 cores in the auction.
		start_sales(100);
		place_bid(0, 1, 200).unwrap();
		place_bid(0, 1, 180).unwrap();
		tick(20);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Renewal));
		let sale = SaleInfo::<Test>::get().unwrap();

		// remaining = 3 total - 2 auction wins = 1 renewal allowed.
		assert!(place_renewal(25, 1, 0, sale.region_begin).is_ok());

		// Second renewal should fail — quota exhausted.
		assert!(matches!(
			place_renewal(25, 1, 1, sale.region_begin),
			Err(Error::Unavailable)
		));
	});
}

#[test]
fn auction_wins_plus_renewals_exhaust_quota() {
	TestExt::new().execute_with(|| {
		// 4 cores offered. User 1 has 3 renewal rights, wins 1 in auction.
		TestCoreRangeProvider::set(0, 4);
		TestRenewalRights::set(1, FIRST_REGION_BEGIN, 3);

		start_sales(100);
		place_bid(0, 1, 200).unwrap(); // 1 auction win
		tick(20);
		let sale = SaleInfo::<Test>::get().unwrap();

		// remaining = 3 total - 1 auction win = 2 renewals allowed.
		assert!(place_renewal(25, 1, 0, sale.region_begin).is_ok());
		assert!(place_renewal(25, 1, 1, sale.region_begin).is_ok());

		// Third renewal fails: 1 auction + 2 renewals = 3 = total rights.
		assert!(matches!(
			place_renewal(25, 1, 2, sale.region_begin),
			Err(Error::Unavailable)
		));
	});
}

#[test]
fn no_displacement_when_not_oversubscribed() {
	TestExt::new().execute_with(|| {
		// 2 cores offered, 1 bid → not oversubscribed, 1 free core.
		let sale = setup_renewal_phase(&[(1, 200)]);

		TestRenewalRights::set(2, sale.region_begin, 1);

		// Renew should succeed via direct allocation (not displacement).
		let result = place_renewal(25, 2, 0, sale.region_begin).unwrap();
		match result {
			RenewalOrderResult::Renewed { .. } => {},
			_ => panic!("Expected direct Renewed"),
		}

		// No displacement refunds should exist.
		let actions = tick(30);
		let refund_count = actions
			.iter()
			.filter(|a| matches!(a, TickAction::Refund { .. }))
			.count();
		assert_eq!(refund_count, 0, "No displacement refunds");
	});
}

#[test]
fn renewal_rights_reset_after_sale_cycle() {
	TestExt::new().execute_with(|| {
		// 2 cores, 0 bids.
		let sale = setup_renewal_phase(&[]);

		TestRenewalRights::set(1, sale.region_begin, 1);
		place_renewal(25, 1, 0, sale.region_begin).unwrap();

		// Finalize → Settlement.
		tick(30);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Settlement));

		// Rotate to new Market.
		let sale = SaleInfo::<Test>::get().unwrap();
		tick_with_ts(35, sale.region_begin);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Market));

		// Settle new auction (no bids).
		tick(55);
		assert_eq!(CurrentPhase::<Test>::get(), Some(SalePhase::Renewal));

		let new_sale = SaleInfo::<Test>::get().unwrap();

		// User 1 gets a new renewal right for the new sale.
		TestRenewalRights::set(1, new_sale.region_begin, 1);

		// Should succeed — used rights were cleared at the previous sale boundary.
		assert!(place_renewal(60, 1, 0, new_sale.region_begin).is_ok());
	});
}

#[test]
fn configure_works() {
	TestExt::new().execute_with(|| {
		let mut config = new_config();
		config.market_period = 50;
		<CoretimeMarket as Market<u64, u64, u64>>::configure(config.clone()).unwrap();

		assert_eq!(Configuration::<Test>::get(), Some(config));
	});
}

#[test]
fn configure_rejects_invalid() {
	TestExt::new().execute_with(|| {
		let mut config = new_config();
		config.market_period = 0; // Invalid.
		assert!(<CoretimeMarket as Market<u64, u64, u64>>::configure(config).is_err());
	});
}
