// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Benchmarking for auctions pallet

#![cfg(feature = "runtime-benchmarks")]
use super::{Pallet as Auctions, *};
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, OnInitialize},
};
use frame_system::RawOrigin;
use polkadot_runtime_parachains::paras;
use sp_runtime::{traits::Bounded, SaturatedConversion};

use frame_benchmarking::v2::*;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

fn fill_winners<T: Config + paras::Config>(lease_period_index: LeasePeriodOf<T>) {
	let auction_index = AuctionCounter::<T>::get();
	let minimum_balance = CurrencyOf::<T>::minimum_balance();

	for n in 1..=SlotRange::SLOT_RANGE_COUNT as u32 {
		let owner = account("owner", n, 0);
		let worst_validation_code = T::Registrar::worst_validation_code();
		let worst_head_data = T::Registrar::worst_head_data();
		CurrencyOf::<T>::make_free_balance_be(&owner, BalanceOf::<T>::max_value());

		assert!(T::Registrar::register(
			owner,
			ParaId::from(n),
			worst_head_data,
			worst_validation_code
		)
		.is_ok());
	}
	assert_ok!(paras::Pallet::<T>::add_trusted_validation_code(
		frame_system::Origin::<T>::Root.into(),
		T::Registrar::worst_validation_code(),
	));

	T::Registrar::execute_pending_transitions();

	for n in 1..=SlotRange::SLOT_RANGE_COUNT as u32 {
		let bidder = account("bidder", n, 0);
		CurrencyOf::<T>::make_free_balance_be(&bidder, BalanceOf::<T>::max_value());

		let slot_range = SlotRange::n((n - 1) as u8).unwrap();
		let (start, end) = slot_range.as_pair();

		assert!(Auctions::<T>::bid(
			RawOrigin::Signed(bidder).into(),
			ParaId::from(n),
			auction_index,
			lease_period_index + start.into(),        // First Slot
			lease_period_index + end.into(),          // Last slot
			minimum_balance.saturating_mul(n.into()), // Amount
		)
		.is_ok());
	}
}

#[benchmarks(
		where T: pallet_babe::Config + paras::Config,
	)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn new_auction() -> Result<(), BenchmarkError> {
		let duration = BlockNumberFor::<T>::max_value();
		let lease_period_index = LeasePeriodOf::<T>::max_value();
		let origin =
			T::InitiateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, duration, lease_period_index);

		assert_last_event::<T>(
			Event::<T>::AuctionStarted {
				auction_index: AuctionCounter::<T>::get(),
				lease_period: LeasePeriodOf::<T>::max_value(),
				ending: BlockNumberFor::<T>::max_value(),
			}
			.into(),
		);

		Ok(())
	}

	// Worst case scenario a new bid comes in which kicks out an existing bid for the same slot.
	#[benchmark]
	fn bid() -> Result<(), BenchmarkError> {
		// If there is an offset, we need to be on that block to be able to do lease things.
		let (_, offset) = T::Leaser::lease_period_length();
		frame_system::Pallet::<T>::set_block_number(offset + One::one());

		// Create a new auction
		let duration = BlockNumberFor::<T>::max_value();
		let lease_period_index = LeasePeriodOf::<T>::zero();
		let origin = T::InitiateOrigin::try_successful_origin()
			.expect("InitiateOrigin has no successful origin required for the benchmark");
		Auctions::<T>::new_auction(origin, duration, lease_period_index)?;

		let para = ParaId::from(0);
		let new_para = ParaId::from(1_u32);

		// Register the paras
		let owner = account("owner", 0, 0);
		CurrencyOf::<T>::make_free_balance_be(&owner, BalanceOf::<T>::max_value());
		let worst_head_data = T::Registrar::worst_head_data();
		let worst_validation_code = T::Registrar::worst_validation_code();
		T::Registrar::register(
			owner.clone(),
			para,
			worst_head_data.clone(),
			worst_validation_code.clone(),
		)?;
		T::Registrar::register(owner, new_para, worst_head_data, worst_validation_code.clone())?;
		assert_ok!(paras::Pallet::<T>::add_trusted_validation_code(
			frame_system::Origin::<T>::Root.into(),
			worst_validation_code,
		));

		T::Registrar::execute_pending_transitions();

		// Make an existing bid
		let auction_index = AuctionCounter::<T>::get();
		let first_slot = AuctionInfo::<T>::get().unwrap().0;
		let last_slot = first_slot + 3u32.into();
		let first_amount = CurrencyOf::<T>::minimum_balance();
		let first_bidder: T::AccountId = account("first_bidder", 0, 0);
		CurrencyOf::<T>::make_free_balance_be(&first_bidder, BalanceOf::<T>::max_value());
		Auctions::<T>::bid(
			RawOrigin::Signed(first_bidder.clone()).into(),
			para,
			auction_index,
			first_slot,
			last_slot,
			first_amount,
		)?;

		let caller: T::AccountId = whitelisted_caller();
		CurrencyOf::<T>::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		let bigger_amount = CurrencyOf::<T>::minimum_balance().saturating_mul(10u32.into());
		assert_eq!(CurrencyOf::<T>::reserved_balance(&first_bidder), first_amount);

		#[extrinsic_call]
		_(
			RawOrigin::Signed(caller.clone()),
			new_para,
			auction_index,
			first_slot,
			last_slot,
			bigger_amount,
		);

		// Confirms that we unreserved funds from a previous bidder, which is worst case
		// scenario.
		assert_eq!(CurrencyOf::<T>::reserved_balance(&caller), bigger_amount);

		Ok(())
	}

	// Worst case: 10 bidders taking all wining spots, and we need to calculate the winner for
	// auction end. Entire winner map should be full and removed at the end of the benchmark.
	#[benchmark]
	fn on_initialize() -> Result<(), BenchmarkError> {
		// If there is an offset, we need to be on that block to be able to do lease things.
		let (lease_length, offset) = T::Leaser::lease_period_length();
		frame_system::Pallet::<T>::set_block_number(offset + One::one());

		// Create a new auction
		let duration: BlockNumberFor<T> = lease_length / 2u32.into();
		let lease_period_index = LeasePeriodOf::<T>::zero();
		let now = frame_system::Pallet::<T>::block_number();
		let origin = T::InitiateOrigin::try_successful_origin()
			.expect("InitiateOrigin has no successful origin required for the benchmark");
		Auctions::<T>::new_auction(origin, duration, lease_period_index)?;

		fill_winners::<T>(lease_period_index);

		for winner in Winning::<T>::get(BlockNumberFor::<T>::from(0u32)).unwrap().iter() {
			assert!(winner.is_some());
		}

		let winning_data = Winning::<T>::get(BlockNumberFor::<T>::from(0u32)).unwrap();
		// Make winning map full
		for i in 0u32..(T::EndingPeriod::get() / T::SampleLength::get()).saturated_into() {
			Winning::<T>::insert(BlockNumberFor::<T>::from(i), winning_data.clone());
		}

		// Move ahead to the block we want to initialize
		frame_system::Pallet::<T>::set_block_number(duration + now + T::EndingPeriod::get());

		// Trigger epoch change for new random number value:
		{
			pallet_babe::EpochStart::<T>::set((Zero::zero(), u32::MAX.into()));
			pallet_babe::Pallet::<T>::on_initialize(duration + now + T::EndingPeriod::get());
			let authorities = pallet_babe::Pallet::<T>::authorities();
			// Check for non empty authority set since it otherwise emits a No-OP warning.
			if !authorities.is_empty() {
				pallet_babe::Pallet::<T>::enact_epoch_change(
					authorities.clone(),
					authorities,
					None,
				);
			}
		}

		#[block]
		{
			Auctions::<T>::on_initialize(duration + now + T::EndingPeriod::get());
		}

		let auction_index = AuctionCounter::<T>::get();
		assert_last_event::<T>(Event::<T>::AuctionClosed { auction_index }.into());
		assert!(Winning::<T>::iter().count().is_zero());

		Ok(())
	}

	// Worst case: 10 bidders taking all wining spots, and winning data is full.
	#[benchmark]
	fn cancel_auction() -> Result<(), BenchmarkError> {
		// If there is an offset, we need to be on that block to be able to do lease things.
		let (lease_length, offset) = T::Leaser::lease_period_length();
		frame_system::Pallet::<T>::set_block_number(offset + One::one());

		// Create a new auction
		let duration: BlockNumberFor<T> = lease_length / 2u32.into();
		let lease_period_index = LeasePeriodOf::<T>::zero();
		let origin = T::InitiateOrigin::try_successful_origin()
			.expect("InitiateOrigin has no successful origin required for the benchmark");
		Auctions::<T>::new_auction(origin, duration, lease_period_index)?;

		fill_winners::<T>(lease_period_index);

		let winning_data = Winning::<T>::get(BlockNumberFor::<T>::from(0u32)).unwrap();
		for winner in winning_data.iter() {
			assert!(winner.is_some());
		}

		// Make winning map full
		for i in 0u32..(T::EndingPeriod::get() / T::SampleLength::get()).saturated_into() {
			Winning::<T>::insert(BlockNumberFor::<T>::from(i), winning_data.clone());
		}
		assert!(AuctionInfo::<T>::get().is_some());

		#[extrinsic_call]
		_(RawOrigin::Root);

		assert!(AuctionInfo::<T>::get().is_none());
		Ok(())
	}

	impl_benchmark_test_suite!(
		Auctions,
		crate::integration_tests::new_test_ext(),
		crate::integration_tests::Test,
	);
}
