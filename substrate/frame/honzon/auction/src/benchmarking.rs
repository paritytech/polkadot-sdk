// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

//! Benchmarks for auction pallet
//!
//! Note: The benchmarking code in this file serves as a mock implementation. The true worst-case
//! scenarios depend on the behavior of the [`AuctionHandler`], which is specific to each runtime.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as AuctionModule;
use frame_benchmarking::v2::*;
use frame_support::traits::{Auction, AuctionInfo};
use frame_system::{Pallet as System, RawOrigin};

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark the `bid` extrinsic with the worst case scenario:
	/// - Auction exists and has started
	/// - Previous bid exists requiring validation
	/// - Handler accepts the bid
	#[benchmark]
	fn bid() {
		let caller: T::AccountId = whitelisted_caller();
		let bid_amount = T::Balance::from(1000u32);

		// Create an auction that has started
		let start_block = System::<T>::block_number();
		let end_block = start_block + 100u32.into();
		let auction_id =
			AuctionModule::<T>::new_auction(start_block, Some(end_block)).expect("auction created");
		// Place previous bid directly
		let auction_info = AuctionInfo {
			bid: Some((caller.clone(), bid_amount / 2u32.into())), // Previous bid
			start: start_block,
			end: Some(end_block),
		};
		AuctionModule::<T>::update_auction(auction_id, auction_info).expect("auction updated");

		// Bid via
		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), auction_id, bid_amount);

		// For test runtime, we expect bid updated event
		System::<T>::assert_last_event(
			crate::Event::Bid { auction_id, bidder: caller, amount: bid_amount }.into(),
		);
	}

	/// Benchmark the `on_finalize` hook with multiple auctions ending at the same block
	#[benchmark]
	fn on_finalize(c: Linear<1, 1000>) {
		let end_block = System::<T>::block_number() + 1u32.into();

		// Create multiple auctions ending at the same block
		for _i in 0..c {
			let start = System::<T>::block_number();
			let id =
				AuctionModule::<T>::new_auction(start, Some(end_block)).expect("auction created");
			let info = AuctionInfo {
				bid: Some((whitelisted_caller(), T::Balance::from(1000u32))),
				start,
				end: Some(end_block),
			};
			AuctionModule::<T>::update_auction(id, info).expect("auction updated");
		}

		// Set the block number to trigger on_finalize
		System::<T>::set_block_number(end_block);

		#[block]
		{
			// Explicitly call the pallet's on_finalize so any cross-pallet
			// effects in the configured Handler are included in the measurement.
			AuctionModule::<T>::on_finalize(end_block);
		}

		// Verify all auctions have been processed
		assert!(Auctions::<T>::iter().count() == 0);
	}

	impl_benchmark_test_suite!(AuctionModule, crate::mock::new_test_ext(), crate::mock::Runtime);
}
