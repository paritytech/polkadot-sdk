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

		// Renew — should succeed since 1 of 2 cores allocated.
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
