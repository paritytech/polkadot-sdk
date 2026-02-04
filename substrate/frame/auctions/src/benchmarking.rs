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

//! Benchmarking setup for pallet-auctions
//!
//! Provides comprehensive benchmarks for all extrinsics and the on_idle hook.

use crate::{
	pallet::{
		ActiveSurplusAuctionId, AuctionConfig, AuctionType, Auctions, BalanceOf,
		CircuitBreakerLevel, OnIdleCursor, Stopped, SurplusHandlingMode, SurplusMode,
	},
	Pallet as AuctionsPallet, *,
};
use frame_benchmarking::v2::*;
use frame_support::{pallet_prelude::*, traits::Hooks};
use frame_system::{pallet_prelude::*, RawOrigin};
use sp_pusd::{AuctionsHandler, DebtComponents};
use sp_runtime::{FixedPointNumber, FixedU128, Permill, SaturatedConversion, Saturating};

const DOT_UNIT: u128 = 10_000_000_000; // 10^10 (DOT has 10 decimals)
const PUSD_UNIT: u128 = 1_000_000; // 10^6 (pUSD has 6 decimals)

fn large_collateral<T: Config>() -> BalanceOf<T> {
	(100 * DOT_UNIT).try_into().unwrap_or_else(|_| 1u32.into())
}

fn standard_tab<T: Config>() -> BalanceOf<T> {
	(50_000 * PUSD_UNIT).try_into().unwrap_or_else(|_| 1u32.into())
}

fn bidder_balance<T: Config>() -> BalanceOf<T> {
	(1_000_000 * PUSD_UNIT).try_into().unwrap_or_else(|_| 1u32.into())
}

fn insurance_fund_balance<T: Config>() -> BalanceOf<T> {
	(200_000 * PUSD_UNIT).try_into().unwrap_or_else(|_| 1u32.into())
}

fn default_price() -> FixedU128 {
	FixedU128::from_rational(421, 100) // $4.21 per DOT
}

fn create_liquidation_auction<T: Config>(
	vault_owner: T::AccountId,
	collateral: BalanceOf<T>,
	tab: BalanceOf<T>,
	keeper: T::AccountId,
) -> Result<u32, BenchmarkError> {
	T::BenchmarkHelper::set_price(default_price());
	T::BenchmarkHelper::setup_liquidation(&vault_owner, collateral, tab);

	// Split tab into: 80% principal, 10% interest, 10% penalty
	let principal = Permill::from_percent(80).mul_floor(tab);
	let interest = Permill::from_percent(10).mul_floor(tab);
	let penalty = tab.saturating_sub(principal).saturating_sub(interest);

	AuctionsPallet::<T>::start_auction(
		vault_owner,
		collateral,
		DebtComponents::new(principal, interest, penalty),
		keeper,
	)
	.map_err(|_| BenchmarkError::Stop("Failed to create auction"))
}

fn advance_to_stale<T: Config>() {
	let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
	let blocks_needed: u32 = config.maximum_duration.saturated_into::<u32>() + 1;
	let current = frame_system::Pallet::<T>::block_number();
	frame_system::Pallet::<T>::set_block_number(current + blocks_needed.into());
}

fn setup_surplus_auction<T: Config>() -> Result<(), BenchmarkError> {
	T::BenchmarkHelper::set_price(default_price());
	SurplusMode::<T>::put(SurplusHandlingMode::Auction);

	let if_balance = insurance_fund_balance::<T>();
	let pusd_supply: BalanceOf<T> =
		(1_000_000 * PUSD_UNIT).try_into().unwrap_or_else(|_| 1u32.into());

	T::BenchmarkHelper::setup_surplus_threshold(if_balance, pusd_supply);

	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn take_liquidation() -> Result<(), BenchmarkError> {
		let vault_owner: T::AccountId = account("vault_owner", 0, 0);
		let buyer: T::AccountId = whitelisted_caller();
		let keeper: T::AccountId = account("keeper", 0, 0);

		let collateral = large_collateral::<T>();
		let tab = standard_tab::<T>();

		let auction_id =
			create_liquidation_auction::<T>(vault_owner.clone(), collateral, tab, keeper.clone())?;

		// Fund buyer with pUSD
		T::BenchmarkHelper::fund_pusd(&buyer, bidder_balance::<T>());

		// Get current price for max parameter
		let auction =
			Auctions::<T>::get(auction_id).ok_or(BenchmarkError::Stop("Auction not found"))?;
		let price = AuctionsPallet::<T>::current_price(&auction);

		#[extrinsic_call]
		_(RawOrigin::Signed(buyer.clone()), auction_id, collateral, price, buyer.clone());

		// Verify auction was completed
		assert!(Auctions::<T>::get(auction_id).is_none());

		Ok(())
	}

	#[benchmark]
	fn take_surplus() -> Result<(), BenchmarkError> {
		let buyer: T::AccountId = whitelisted_caller();
		let keeper: T::AccountId = account("keeper", 0, 0);

		setup_surplus_auction::<T>()?;

		// Start surplus auction
		AuctionsPallet::<T>::do_start_surplus_auction(keeper.clone())
			.map_err(|_| BenchmarkError::Stop("Failed to start surplus auction"))?;

		let auction_id = ActiveSurplusAuctionId::<T>::get()
			.ok_or(BenchmarkError::Stop("No active surplus auction"))?;

		// Get current price to calculate how much DOT we need
		let auction =
			Auctions::<T>::get(auction_id).ok_or(BenchmarkError::Stop("Auction not found"))?;
		let price = AuctionsPallet::<T>::current_price(&auction);

		// Calculate a reasonable purchase amount that the buyer can afford
		// DOT payment = price * pusd_amount, so we need to fund buyer accordingly
		let pusd_amount = T::SurplusAuctionAmount::get();
		let dot_needed = price
			.saturating_mul(FixedU128::saturating_from_integer(pusd_amount))
			.saturating_mul_int(1u128);
		let dot_with_margin: BalanceOf<T> =
			(dot_needed * 2).try_into().unwrap_or_else(|_| 1u32.into());

		// Fund buyer with enough DOT for payment (with margin)
		T::BenchmarkHelper::fund_account(&buyer, dot_with_margin);

		#[extrinsic_call]
		_(RawOrigin::Signed(buyer.clone()), auction_id, pusd_amount, price, buyer.clone());

		// Verify auction was completed
		assert!(Auctions::<T>::get(auction_id).is_none());
		assert!(ActiveSurplusAuctionId::<T>::get().is_none());

		Ok(())
	}

	#[benchmark]
	fn restart_auction() -> Result<(), BenchmarkError> {
		let vault_owner: T::AccountId = account("vault_owner", 0, 0);
		let keeper: T::AccountId = whitelisted_caller();
		let new_keeper: T::AccountId = account("new_keeper", 0, 0);

		let collateral = large_collateral::<T>();
		let tab = standard_tab::<T>();

		let auction_id =
			create_liquidation_auction::<T>(vault_owner.clone(), collateral, tab, keeper.clone())?;

		// Block restarts via on_idle while advancing
		Stopped::<T>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);

		// Advance to make auction stale
		advance_to_stale::<T>();

		// Re-enable restarts
		Stopped::<T>::put(CircuitBreakerLevel::AllEnabled);

		// Verify auction needs restart
		let auction =
			Auctions::<T>::get(auction_id).ok_or(BenchmarkError::Stop("Auction not found"))?;
		assert!(AuctionsPallet::<T>::needs_restart(&auction));

		#[extrinsic_call]
		_(RawOrigin::Signed(keeper.clone()), auction_id, new_keeper.clone());

		// Verify auction was restarted
		let auction = Auctions::<T>::get(auction_id)
			.ok_or(BenchmarkError::Stop("Auction not found after restart"))?;
		assert!(!AuctionsPallet::<T>::needs_restart(&auction));
		assert_eq!(auction.keeper, new_keeper);

		Ok(())
	}

	#[benchmark]
	fn start_surplus_auction() -> Result<(), BenchmarkError> {
		let keeper: T::AccountId = whitelisted_caller();

		setup_surplus_auction::<T>()?;

		#[extrinsic_call]
		_(RawOrigin::Signed(keeper.clone()), keeper.clone());

		// Verify surplus auction was started
		assert!(ActiveSurplusAuctionId::<T>::get().is_some());

		Ok(())
	}

	#[benchmark]
	fn set_buffer() -> Result<(), BenchmarkError> {
		let new_buffer = FixedU128::from_rational(130, 100);

		#[extrinsic_call]
		_(RawOrigin::Root, AuctionType::Liquidation, new_buffer);

		let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
		assert_eq!(config.buffer, new_buffer);

		Ok(())
	}

	#[benchmark]
	fn set_maximum_duration() -> Result<(), BenchmarkError> {
		let new_duration: BlockNumberFor<T> = 43200u32.into();

		#[extrinsic_call]
		_(RawOrigin::Root, AuctionType::Liquidation, new_duration);

		let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
		assert_eq!(config.maximum_duration, new_duration);

		Ok(())
	}

	#[benchmark]
	fn set_minimum_price() -> Result<(), BenchmarkError> {
		let new_min_price = FixedU128::from_rational(50, 100);

		#[extrinsic_call]
		_(RawOrigin::Root, AuctionType::Liquidation, new_min_price);

		let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
		assert_eq!(config.minimum_price, new_min_price);

		Ok(())
	}

	#[benchmark]
	fn set_chip() -> Result<(), BenchmarkError> {
		let new_chip = Permill::from_parts(5000);

		#[extrinsic_call]
		_(RawOrigin::Root, AuctionType::Liquidation, new_chip);

		let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
		assert_eq!(config.chip, new_chip);

		Ok(())
	}

	#[benchmark]
	fn set_tip() -> Result<(), BenchmarkError> {
		let new_tip: BalanceOf<T> = (200 * PUSD_UNIT).try_into().unwrap_or_else(|_| 1u32.into());

		#[extrinsic_call]
		_(RawOrigin::Root, AuctionType::Liquidation, new_tip);

		let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
		assert_eq!(config.tip, new_tip);

		Ok(())
	}

	#[benchmark]
	fn set_curve() -> Result<(), BenchmarkError> {
		let new_curve = PriceCurve::default();

		#[extrinsic_call]
		_(RawOrigin::Root, AuctionType::Liquidation, new_curve);

		let config = AuctionConfig::<T>::get(AuctionType::Liquidation);
		assert_eq!(config.curve, new_curve);

		Ok(())
	}

	#[benchmark]
	fn set_stopped() -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(RawOrigin::Root, CircuitBreakerLevel::NoNewAuctions);

		assert_eq!(Stopped::<T>::get(), CircuitBreakerLevel::NoNewAuctions);

		Ok(())
	}

	#[benchmark]
	fn set_surplus_mode() -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(RawOrigin::Root, SurplusHandlingMode::Auction);

		assert_eq!(SurplusMode::<T>::get(), SurplusHandlingMode::Auction);

		Ok(())
	}

	#[benchmark]
	fn transfer_surplus() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		// Set mode to DirectTransfer
		SurplusMode::<T>::put(SurplusHandlingMode::DirectTransfer);
		T::BenchmarkHelper::set_price(default_price());

		let if_balance = insurance_fund_balance::<T>();
		let pusd_supply: BalanceOf<T> =
			(1_000_000 * PUSD_UNIT).try_into().unwrap_or_else(|_| 1u32.into());

		T::BenchmarkHelper::setup_surplus_threshold(if_balance, pusd_supply);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller));

		Ok(())
	}

	#[benchmark]
	fn on_idle_one_auction() -> Result<(), BenchmarkError> {
		let vault_owner: T::AccountId = account("vault_owner", 0, 0);
		let keeper: T::AccountId = account("keeper", 0, 0);

		let collateral = large_collateral::<T>();
		let tab = standard_tab::<T>();

		let auction_id =
			create_liquidation_auction::<T>(vault_owner.clone(), collateral, tab, keeper.clone())?;

		// Block restarts via on_idle while advancing
		Stopped::<T>::put(CircuitBreakerLevel::NoNewAuctionsOrRestarts);

		// Advance to make auction stale
		advance_to_stale::<T>();

		// Re-enable all operations
		Stopped::<T>::put(CircuitBreakerLevel::AllEnabled);

		// Clear cursor to start fresh
		OnIdleCursor::<T>::kill();

		let current_block = frame_system::Pallet::<T>::block_number();

		#[block]
		{
			AuctionsPallet::<T>::on_idle(current_block, Weight::MAX);
		}

		// Verify auction was restarted (or processed)
		let auction = Auctions::<T>::get(auction_id)
			.ok_or(BenchmarkError::Stop("Auction should still exist"))?;
		assert!(!AuctionsPallet::<T>::needs_restart(&auction));

		Ok(())
	}

	impl_benchmark_test_suite!(AuctionsPallet, crate::mock::new_test_ext(), crate::mock::Test);
}
