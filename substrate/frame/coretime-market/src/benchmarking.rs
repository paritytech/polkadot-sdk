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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use fp_coretime::{
	market::{Market, TimesliceProvider},
	PotentialRenewalId,
};
use frame_benchmarking::v2::*;

const SEED: u32 = 0;

fn default_config<T: Config>() -> ConfigRecordOf<T> {
	ConfigRecord {
		market_period: 20u32.into(),
		renewal_period: 10u32.into(),
		ideal_bulk_proportion: sp_arithmetic::Perbill::from_percent(100),
		limit_cores_offered: None,
		region_length: 3,
		penalty: sp_arithmetic::Perbill::from_percent(30),
		price_multiplier: 2,
		min_opening_price: 10u32.into(),
		target_consumption_rate: sp_arithmetic::Perbill::from_percent(90),
		sensitivity_millis: 2500,
		min_reserve_price: 1u32.into(),
		min_increment: 100u32.into(),
	}
}

fn setup_sale<T: Config>() {
	let config = default_config::<T>();
	Pallet::<T>::configure(config).unwrap();
	let init = InitData { reserve_price: 100u32.into() };
	Pallet::<T>::start_sales(0u32.into(), init).unwrap();
}

/// Make the current sale offer `cores` cores, independent of the runtime's core range.
fn set_cores_offered<T: Config>(cores: u32) {
	SaleInfo::<T>::mutate_extant(|sale| sale.cores_offered = cores.saturated_into());
}

fn fill_bids<T: Config>(n: u32) {
	for i in 0..n {
		let who: T::AccountId = account("bidder", i, SEED);
		// Vary prices so bids spread across the sorted vec.
		let price: u32 = 200u32.saturating_sub(i % 100);
		Pallet::<T>::place_order(0u32.into(), &who, price.into()).unwrap();
	}
}

fn advance_to_renewal<T: Config>() {
	let mut meter = frame_support::weights::WeightMeter::new();
	Pallet::<T>::tick(20u32.into(), &mut meter);
	assert_eq!(SaleInfo::<T>::get().map(|s| s.phase), Some(SalePhase::Renewal));
}

fn market_events<T: Config>() -> Vec<Event<T>> {
	frame_system::Pallet::<T>::read_events_for_pallet::<Event<T>>()
}

#[benchmarks]
mod benches {
	use frame_support::assert_ok;

	use super::*;

	#[benchmark]
	fn configure() {
		let config = default_config::<T>();

		#[block]
		{
			Pallet::<T>::configure(config).unwrap();
		}

		assert!(Configuration::<T>::get().is_some());
	}

	#[benchmark]
	fn start_sales() {
		let config = default_config::<T>();
		Pallet::<T>::configure(config).unwrap();
		let init = InitData { reserve_price: 100u32.into() };

		#[block]
		{
			Pallet::<T>::start_sales(0u32.into(), init).unwrap();
		}

		assert!(market_events::<T>()
			.into_iter()
			.any(|event| matches!(event, Event::<T>::SaleInitialized { .. })));
	}

	#[benchmark]
	fn place_order() {
		setup_sale::<T>();
		fill_bids::<T>(T::MaxBids::get());

		let caller: T::AccountId = account("caller", 0, SEED);

		#[block]
		{
			Pallet::<T>::place_order(0u32.into(), &caller, 200u32.into()).unwrap();
		}

		assert_eq!(
			market_events::<T>()
				.into_iter()
				.filter(|event| { matches!(event, Event::BidPlaced { .. }) })
				.count(),
			1
		);
	}

	#[benchmark]
	fn place_renewal_order() {
		setup_sale::<T>();

		// Worst case: a full book of winners where only the highest-priced one is displaceable,
		// so displacement scans all `MaxBids` allocations.
		set_cores_offered::<T>(T::MaxBids::get());
		fill_bids::<T>(T::MaxBids::get());

		advance_to_renewal::<T>();

		let region_begin = pallet::SaleInfo::<T>::get().unwrap().region_begin;
		let allocations = pallet::Allocations::<T>::get();
		for allocation in allocations.iter().skip(1) {
			T::RenewalRights::set_rights_count(&allocation.who, region_begin, 1);
		}
		let top = allocations.first().unwrap().who.clone();
		let caller: T::AccountId = account("renewer", 0, SEED);
		T::RenewalRights::set_rights_count(&caller, region_begin, 1);
		let renewal_id = PotentialRenewalId { core: 0, when: region_begin };

		#[block]
		{
			Pallet::<T>::place_renewal_order(0u32.into(), &caller, renewal_id).unwrap();
		}

		assert!(market_events::<T>()
			.into_iter()
			.any(|event| matches!(event, Event::BidDisplaced { who, .. } if who == top)));
	}

	#[benchmark]
	fn adjust_bid() {
		setup_sale::<T>();
		let max = T::MaxBids::get();
		// Pick the last-inserted bidder — they hold the lowest price and their bid
		// will shift across the whole sorted vec when raised (worst-case cost).
		let caller: T::AccountId = account("bidder", max - 1, SEED);
		fill_bids::<T>(max);

		let bid_id = pallet::Bids::<T>::get()
			.iter()
			.find(|b| b.who == caller)
			.map(|b| b.bid_id)
			.unwrap();

		#[block]
		{
			Pallet::<T>::adjust_bid(0u32.into(), bid_id, &caller, Some(200u32.into())).unwrap();
		}

		assert!(market_events::<T>()
			.into_iter()
			.any(|event| matches!(event, Event::BidRaised { .. })));
	}

	#[benchmark]
	fn tick_base() {
		setup_sale::<T>();

		assert!(SaleInfo::<T>::exists());
		assert!(Configuration::<T>::exists());

		// The most expensive check is performed at this phase.
		SaleInfo::<T>::mutate_extant(|sale| sale.phase = SalePhase::Settlement);

		let mut meter = WeightMeter::new();
		#[block]
		{
			Pallet::<T>::tick(0u32.into(), &mut meter);
		}
	}

	#[benchmark]
	fn sale_phase_transition_to_renewal(c: Linear<1, 100>) {
		setup_sale::<T>();
		set_cores_offered::<T>(c);

		for i in 0..T::MaxBids::get() {
			let who: T::AccountId = account("bidder", i, SEED);
			// Equal bids will result in the worst-case scenario for marginal bids shuffle.
			let price: u32 = 200u32;
			assert_ok!(Pallet::<T>::place_order(0u32.into(), &who, price.into()));
		}

		let sale = pallet::SaleInfo::<T>::get().unwrap();
		let config = pallet::Configuration::<T>::get().unwrap();

		let market_end = sale.sale_start.saturating_add(config.market_period);

		frame_system::Pallet::<T>::reset_events();
		let mut weight_meter = WeightMeter::new();
		#[block]
		{
			Pallet::<T>::tick(market_end, &mut weight_meter);
		}

		frame_system::Pallet::<T>::assert_has_event(
			Event::<T>::PhaseTransitioned { from: SalePhase::Market, to: SalePhase::Renewal }
				.into(),
		);
	}

	#[benchmark]
	fn sale_phase_transition_to_settlement(c: Linear<1, 100>) {
		setup_sale::<T>();

		// Worst case: `c` winners all displaced by `c` distinct renewers, leaving `2c` quotas
		// to clear.
		let cores = c.min(T::MaxBids::get());
		set_cores_offered::<T>(cores);
		fill_bids::<T>(cores);

		advance_to_renewal::<T>();

		let sale = pallet::SaleInfo::<T>::get().unwrap();
		let config = pallet::Configuration::<T>::get().unwrap();

		for i in 0..cores {
			let renewer: T::AccountId = account("renewer", i, SEED);
			T::RenewalRights::set_rights_count(&renewer, sale.region_begin, 1);

			assert_ok!(Pallet::<T>::place_renewal_order(
				0u32.into(),
				&renewer,
				PotentialRenewalId { core: 0, when: sale.region_begin }
			));
		}

		assert_eq!(pallet::Quotas::<T>::iter_keys().count() as u32, 2 * cores);

		let market_end = sale.sale_start.saturating_add(config.market_period);
		let renewal_end = market_end.saturating_add(config.renewal_period);

		frame_system::Pallet::<T>::reset_events();
		let mut weight_meter = WeightMeter::new();
		#[block]
		{
			Pallet::<T>::tick(renewal_end, &mut weight_meter);
		}

		frame_system::Pallet::<T>::assert_has_event(
			Event::<T>::PhaseTransitioned { from: SalePhase::Renewal, to: SalePhase::Settlement }
				.into(),
		);
	}

	#[benchmark]
	fn sale_phase_transition_to_market() {
		setup_sale::<T>();
		let ready = T::TimesliceProvider::latest_timeslice_ready_to_commit().unwrap();
		let config = Configuration::<T>::get().unwrap();
		SaleInfo::<T>::mutate_extant(|sale| {
			sale.phase = SalePhase::Settlement;
			sale.region_begin = ready;
			sale.region_end = ready.saturating_add(config.region_length);
		});

		frame_system::Pallet::<T>::reset_events();
		let mut meter = frame_support::weights::WeightMeter::new();
		#[block]
		{
			Pallet::<T>::tick(0u32.into(), &mut meter);
		}

		frame_system::Pallet::<T>::assert_has_event(
			Event::<T>::PhaseTransitioned { from: SalePhase::Settlement, to: SalePhase::Market }
				.into(),
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
