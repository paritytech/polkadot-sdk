#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use pallet_broker::{
	market::{CoreRangeProvider, Market},
	PotentialRenewalId,
};

const SEED: u32 = 0;

fn default_config<T: Config>() -> ConfigRecordOf<T> {
	ConfigRecord {
		advance_notice: 2u32.into(),
		market_period: 20u32.into(),
		renewal_period: 10u32.into(),
		ideal_bulk_proportion: sp_arithmetic::Perbill::from_percent(100),
		limit_cores_offered: None,
		region_length: 3,
		penalty: sp_arithmetic::Perbill::from_percent(30),
		contribution_timeout: 5,
		price_multiplier: 2,
		min_opening_price: 10u32.into(),
		target_consumption_rate: sp_arithmetic::Perbill::from_percent(90),
		sensitivity_millis: 2500,
		min_reserve_price: 1u32.into(),
		min_increment: 100u32.into(),
	}
}

fn setup_sale<T: Config>() -> Result<(), BenchmarkError> {
	let config = default_config::<T>();
	Pallet::<T>::configure(config).map_err(|_| BenchmarkError::Weightless)?;
	let init = InitData { reserve_price: 100u32.into() };
	Pallet::<T>::start_sales(0u32.into(), init).map_err(|_| BenchmarkError::Weightless)?;
	Ok(())
}

fn fill_bids<T: Config>(n: u32) -> Result<(), BenchmarkError> {
	for i in 0..n {
		let who: T::AccountId = account("bidder", i, SEED);
		// Vary prices so bids spread across the sorted vec.
		let price: u32 = 200u32.saturating_sub(i % 100);
		Pallet::<T>::place_order(0u32.into(), &who, price.into())
			.map_err(|_| BenchmarkError::Weightless)?;
	}
	Ok(())
}

fn advance_to_renewal<T: Config>(n_bids: u32) -> Result<(), BenchmarkError> {
	setup_sale::<T>()?;
	fill_bids::<T>(n_bids)?;
	let mut meter = frame_support::weights::WeightMeter::new();
	Pallet::<T>::tick(20u32.into(), &mut meter);
	assert_eq!(SaleInfo::<T>::get().map(|s| s.phase), Some(SalePhase::Renewal));
	Ok(())
}

#[benchmarks]
mod benches {
	use super::*;

	#[benchmark]
	fn place_order() -> Result<(), BenchmarkError> {
		setup_sale::<T>()?;
		let max = T::MaxBids::get();
		fill_bids::<T>(max - 1)?;

		let caller: T::AccountId = account("caller", 0, SEED);

		#[block]
		{
			Pallet::<T>::place_order(0u32.into(), &caller, 150u32.into())
				.map_err(|_| BenchmarkError::Weightless)?;
		}

		assert_eq!(pallet::Bids::<T>::get().len(), max as usize);
		Ok(())
	}

	#[benchmark]
	fn adjust_bid() -> Result<(), BenchmarkError> {
		setup_sale::<T>()?;
		let max = T::MaxBids::get();
		// Pick the last-inserted bidder — they hold the lowest price and their bid
		// will shift across the whole sorted vec when raised (worst-case cost).
		let caller: T::AccountId = account("bidder", max - 1, SEED);
		fill_bids::<T>(max)?;

		let bid_id = pallet::Bids::<T>::get()
			.iter()
			.find(|b| b.who == caller)
			.map(|b| b.bid_id)
			.ok_or(BenchmarkError::Weightless)?;

		#[block]
		{
			Pallet::<T>::adjust_bid(0u32.into(), bid_id, &caller, Some(200u32.into()))
				.map_err(|_| BenchmarkError::Weightless)?;
		}

		Ok(())
	}

	#[benchmark]
	fn place_renewal_order_renewal() -> Result<(), BenchmarkError> {
		advance_to_renewal::<T>(1)?;
		let sale = pallet::SaleInfo::<T>::get().ok_or(BenchmarkError::Weightless)?;
		let caller: T::AccountId = account("renewer", 0, SEED);
		T::RenewalRights::set_rights_count(&caller, sale.region_begin, 1);
		let renewal_id = PotentialRenewalId { core: 0, when: sale.region_begin };

		#[block]
		{
			Pallet::<T>::place_renewal_order(25u32.into(), &caller, renewal_id)
				.map_err(|_| BenchmarkError::Weightless)?;
		}

		Ok(())
	}

	#[benchmark]
	fn place_renewal_order_displacement() -> Result<(), BenchmarkError> {
		let cores = T::CoreRangeProvider::core_range().map(|r| r.to - r.from).unwrap_or(2);
		advance_to_renewal::<T>(cores as u32)?;
		let sale = pallet::SaleInfo::<T>::get().ok_or(BenchmarkError::Weightless)?;
		let caller: T::AccountId = account("renewer", 0, SEED);
		T::RenewalRights::set_rights_count(&caller, sale.region_begin, 1);
		let renewal_id = PotentialRenewalId { core: 0, when: sale.region_begin };

		#[block]
		{
			Pallet::<T>::place_renewal_order(25u32.into(), &caller, renewal_id)
				.map_err(|_| BenchmarkError::Weightless)?;
		}

		Ok(())
	}

	#[benchmark]
	fn settle_auction() -> Result<(), BenchmarkError> {
		setup_sale::<T>()?;
		fill_bids::<T>(T::MaxBids::get())?;
		let sale = pallet::SaleInfo::<T>::get().ok_or(BenchmarkError::Weightless)?;

		#[block]
		{
			super::settle_auction::<T>(&sale);
		}

		assert!(pallet::Bids::<T>::get().is_empty());
		Ok(())
	}

	#[benchmark]
	fn finalize_sale() -> Result<(), BenchmarkError> {
		let cores = T::CoreRangeProvider::core_range().map(|r| r.to - r.from).unwrap_or(2);
		advance_to_renewal::<T>(cores as u32)?;
		let sale = pallet::SaleInfo::<T>::get().ok_or(BenchmarkError::Weightless)?;

		#[block]
		{
			super::finalize_sale::<T>(&sale);
		}

		assert!(pallet::Allocations::<T>::get().is_empty());
		Ok(())
	}

	#[benchmark]
	fn rotate_sale() -> Result<(), BenchmarkError> {
		setup_sale::<T>()?;
		fill_bids::<T>(2)?;
		let mut meter = frame_support::weights::WeightMeter::new();
		Pallet::<T>::tick(20u32.into(), &mut meter);
		Pallet::<T>::tick(30u32.into(), &mut meter);

		let sale = pallet::SaleInfo::<T>::get().ok_or(BenchmarkError::Weightless)?;
		let config = pallet::Configuration::<T>::get().ok_or(BenchmarkError::Weightless)?;
		let range = T::CoreRangeProvider::core_range().ok_or(BenchmarkError::Weightless)?;

		#[block]
		{
			super::rotate_sale::<T>(&sale, &config, &range, 35u32.into());
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
