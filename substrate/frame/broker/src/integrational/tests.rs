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

use fp_coretime::{RegionId, TaskId, market::RenewalRightsProvider};
use frame_support::{assert_err, assert_noop, assert_ok};
use pallet_coretime_market::{AccountQuota, Error as MarketError, Event as MarketEvent, InitData, Quotas, SalePhase};
use sp_runtime::TokenError;

use crate::{
	Error as BrokerError, Event as BrokerEvent, Finality, PotentialRenewals, integrational::{
		Broker, MarketPallet, RuntimeOrigin, System, Test, TestExt, advance_one_block, advance_to, balance
	}, mock::REGION_LENGTH
};

#[test]
fn can_start_sales() {
	TestExt::new().execute_with(|| {
		advance_to(2);

		let init_data = InitData { reserve_price: 10 };
		assert_ok!(Broker::do_start_sales(init_data.clone(), 0));

		System::assert_has_event(
			BrokerEvent::<Test>::SalesStarted { init_data, core_count: 0 }.into(),
		);
	});
}

#[test]
fn can_place_order() {
	const PURCHASER: u64 = 1;

	TestExt::new().endow(PURCHASER, 1000).execute_with(|| {
		advance_to(2);
		assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 0));

		assert_ok!(Broker::do_purchase(PURCHASER, 100));

		assert!(market_events()
			.into_iter()
			.any(|event| matches!(event, MarketEvent::BidPlaced { who, ..} if who == PURCHASER)));
	});
}

#[test]
fn can_place_renewal_order() {
	const PURCHASER: u64 = 1;

	TestExt::new().endow(PURCHASER, 1000).execute_with(|| {
		advance_to(2);
		assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 1));

		assert_ok!(Broker::do_purchase(PURCHASER, 100));
		assert!(market_events()
			.into_iter()
			.any(|event| matches!(event, MarketEvent::BidPlaced { who, ..} if who == PURCHASER)));

		advance_to_market_phase(SalePhase::Settlement);

		let region_id = get_region_id_from_latest_purchased_event(PURCHASER);
		let Some(region_id) = region_id else {
			panic!("Expected the bid to be executed at market settlement phase");
		};

		assert_ok!(Broker::do_assign(region_id, Some(PURCHASER), 1001, Finality::Final));

		advance_to_market_phase(SalePhase::Renewal);

		assert_ok!(Broker::do_renew(PURCHASER, region_id.core));

		assert!(market_events().into_iter().any(
			|event| matches!(event, MarketEvent::RenewalExercised { who, ..} if who == PURCHASER)
		));
		assert!(broker_events().into_iter().any(
			|event| matches!(event, BrokerEvent::Renewed { who, old_core, ..} if who == PURCHASER && old_core == region_id.core)
		));
	});
}

#[test]
fn auto_renewals_at_renewal_phase_work() {
	test_auto_renewal_at_sale_phase(SalePhase::Renewal);
}

#[test]
fn auto_renewals_at_settlement_phase_work() {
	test_auto_renewal_at_sale_phase(SalePhase::Settlement);
}

fn test_auto_renewal_at_sale_phase(test_at_phase: SalePhase) {
	const PURCHASER: u64 = 1001;
	const TASK_ID: TaskId = 1001;

	TestExt::new().endow(PURCHASER, 1000).execute_with(|| {
		advance_to(2);
		assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 1));

		assert_ok!(Broker::do_purchase(PURCHASER, 100));

		advance_to_market_phase(SalePhase::Settlement);

		let region_id = get_region_id_from_latest_purchased_event(PURCHASER);
		let Some(region_id) = region_id else {
			panic!("Expected the bid to be executed at market settlement phase");
		};

		assert_ok!(Broker::do_assign(region_id, Some(PURCHASER), TASK_ID, Finality::Final));
		
		let (potential_renewal_id, _) = PotentialRenewals::<Test>::iter()
			.next()
			.expect("Potential renewal expected after do_assign call");
 
		advance_to_market_phase(test_at_phase);

		assert_ok!(Broker::do_enable_auto_renew(PURCHASER, region_id.core, TASK_ID, Some(potential_renewal_id.when)));

		advance_to_market_phase(SalePhase::Renewal);

		println!("Market events: {:?}", market_events().into_iter().collect::<Vec<_>>());
		println!("Broker events: {:?}", broker_events().into_iter().collect::<Vec<_>>());

		assert!(market_events().into_iter().any(
			|event| matches!(event, MarketEvent::RenewalExercised { who, ..} if who == PURCHASER)
		));
		assert!(broker_events().into_iter().any(
			|event| matches!(event, BrokerEvent::Renewed { who, old_core, ..} if who == PURCHASER && old_core == region_id.core)
		));

		advance_to_market_phase(SalePhase::Settlement);
		advance_to_market_phase(SalePhase::Renewal);

		assert!(market_events().into_iter().any(
			|event| matches!(event, MarketEvent::RenewalExercised { who, ..} if who == PURCHASER)
		));
		assert!(broker_events().into_iter().any(
			|event| matches!(event, BrokerEvent::Renewed { who, old_core, ..} if who == PURCHASER && old_core == region_id.core)
		));
	});
}

#[test]
fn bid_displacement_works() {
	const PURCHASER_1: u64 = 1;
	const PURCHASER_2: u64 = 2;
	const PURCHASER_3: u64 = 3;

	const INITIAL_BALANCE: u64 = 1000;

	TestExt::new()
		.endow(PURCHASER_1, INITIAL_BALANCE)
		.endow(PURCHASER_2, INITIAL_BALANCE)
		.endow(PURCHASER_3, INITIAL_BALANCE)
		.execute_with(|| {
			advance_to(2);
			assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 2));

			advance_to_market_phase(SalePhase::Market);

			let price = MarketPallet::current_price(System::block_number())
				.expect("The price should be known");

			assert_ok!(Broker::do_purchase(PURCHASER_1, price));
			assert_ok!(Broker::do_purchase(PURCHASER_2, price - 1));
			assert_ok!(Broker::do_purchase(PURCHASER_3, price));

			System::assert_has_event(
				MarketEvent::BidPlaced { who: PURCHASER_1, bid_id: 0, amount: price }.into(),
			);
			System::assert_has_event(
				MarketEvent::BidPlaced { who: PURCHASER_2, bid_id: 1, amount: price - 1 }.into(),
			);
			System::assert_has_event(
				MarketEvent::BidPlaced { who: PURCHASER_3, bid_id: 2, amount: price }.into(),
			);
			assert_eq!(balance(PURCHASER_1), INITIAL_BALANCE - price);
			assert_eq!(balance(PURCHASER_2), INITIAL_BALANCE - (price - 1));
			assert_eq!(balance(PURCHASER_3), INITIAL_BALANCE - price);

			advance_to_market_phase(SalePhase::Renewal);

			System::assert_has_event(
				BrokerEvent::Refunded { who: PURCHASER_2, amount: price - 1 }.into(),
			);
			assert_eq!(balance(PURCHASER_1), INITIAL_BALANCE - price);
			assert_eq!(balance(PURCHASER_2), INITIAL_BALANCE);
			assert_eq!(balance(PURCHASER_3), INITIAL_BALANCE - price);
		});
}

#[test]
fn purchase_with_not_enough_funds_reverts_state_of_both_pallets() {
	TestExt::new().execute_with(|| {
		advance_to(2);
		assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 0));

		assert_noop!(Broker::purchase(RuntimeOrigin::signed(1), 100), TokenError::FundsUnavailable);

		assert_eq!(
			market_events()
				.into_iter()
				.any(|event| matches!(event, MarketEvent::BidPlaced { .. })),
			false
		);
	});
}

#[test]
fn do_renew_with_not_enough_funds_reverts_state_of_both_pallets() {
	const PURCHASER: u64 = 1;

	TestExt::new().endow(PURCHASER, 10).execute_with(|| {
		advance_to(2);
		assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 1));
		assert_ok!(Broker::do_purchase(PURCHASER, 10));

		advance_to_market_phase(SalePhase::Settlement);

		let region_id = get_region_id_from_latest_purchased_event(PURCHASER);
		let Some(region_id) = region_id else {
			panic!("Expected the bid to be executed at market settlement phase");
		};

		assert_ok!(Broker::do_assign(region_id, Some(PURCHASER), 1001, Finality::Final));

		advance_to_market_phase(SalePhase::Renewal);

		assert_noop!(Broker::renew(RuntimeOrigin::signed(PURCHASER), region_id.core), TokenError::FundsUnavailable);
	});
}

#[test]
fn start_sales_without_config_reverts_state_of_both_pallets() {
	TestExt::new().execute_with(|| {
		advance_to(2);

		pallet_coretime_market::Configuration::<Test>::kill();

		let init_data = InitData { reserve_price: 10 };
		assert_err!(Broker::start_sales(RuntimeOrigin::root(), init_data, 0), MarketError::<Test>::Uninitialized);
	});
}

#[test]
fn renewal_rights_work() {
	const PURCHASER: u64 = 1;

	TestExt::new().endow(PURCHASER, 1000).execute_with(|| {
		advance_to(2);
		assert_ok!(Broker::do_start_sales(InitData { reserve_price: 10 }, 3));

		assert_ok!(Broker::do_purchase(PURCHASER, 100));
		assert_ok!(Broker::do_purchase(PURCHASER, 100));
		assert_ok!(Broker::do_purchase(PURCHASER, 100));

		advance_to_market_phase(SalePhase::Settlement);

		let regions: Vec<_> = broker_events()
		.into_iter()
		.filter_map(|event| {
			if let BrokerEvent::Purchased { region_id, .. } = event {
				Some(region_id)
			} else {
				None
			}
		}).collect();

		assert_ok!(Broker::do_assign(regions[0], Some(PURCHASER), 1001, Finality::Provisional));
		assert_ok!(Broker::do_assign(regions[1], Some(PURCHASER), 1001, Finality::Final));
		assert_ok!(Broker::do_assign(regions[2], Some(PURCHASER), 1001, Finality::Final));

		let regions_end = regions[0].begin + REGION_LENGTH;
		assert_eq!(Broker::renewal_rights_count(&PURCHASER, regions_end), 2);

		advance_to_market_phase(SalePhase::Market);

		assert_ok!(Broker::do_purchase(PURCHASER, 200));

		advance_to_market_phase(SalePhase::Renewal);

		assert_eq!(Quotas::<Test>::get(&PURCHASER), AccountQuota { auction_wins: 1, renewals_used: 0 });

		assert_ok!(Broker::do_renew(PURCHASER, regions[1].core));
		assert_eq!(Quotas::<Test>::get(&PURCHASER), AccountQuota { auction_wins: 1, renewals_used: 1 });

		assert_noop!(Broker::do_renew(PURCHASER, regions[2].core), MarketError::<Test>::Unavailable);
	});
}

fn advance_to_market_phase(phase: SalePhase) {
	use pallet_coretime_market::SaleInfo;

	loop {
		let correct_phase =
			SaleInfo::<Test>::get().map(|info| info.phase == phase).unwrap_or(false);
		if correct_phase {
			break;
		}

		advance_one_block();
	}
}

fn get_region_id_from_latest_purchased_event(purchaser: u64) -> Option<RegionId> {
	broker_events()
		.into_iter()
		.rev()
		.filter_map(|event| {
			if let BrokerEvent::Purchased { who, region_id, .. } = event {
				if who == purchaser {
					Some(region_id)
				} else {
					None
				}
			} else {
				None
			}
		})
		.next()
}

fn market_events() -> Vec<MarketEvent<Test>> {
	frame_system::Pallet::<Test>::read_events_for_pallet::<MarketEvent<Test>>()
}

fn broker_events() -> Vec<BrokerEvent<Test>> {
	frame_system::Pallet::<Test>::read_events_for_pallet::<BrokerEvent<Test>>()
}
