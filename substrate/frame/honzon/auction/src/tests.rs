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

//! Unit tests for the auction module.

#![cfg(test)]

use super::*;
use frame_support::{
	assert_noop, assert_ok,
	traits::honzon::{Auction, AuctionInfo},
};
use mock::*;

#[test]
fn cannot_add_new_auction_when_no_available_id() {
	new_test_ext().execute_with(|| {
		<AuctionsIndex<Runtime>>::put(AuctionId::MAX);
		assert_noop!(AuctionModule::new_auction(0, None), Error::<Runtime>::NoAvailableAuctionId);
	});
}

#[test]
fn new_auction_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(AuctionModule::new_auction(10, Some(100)), 0);
		// AuctionEndTime should be set for block 100 and auction id 0
		assert!(AuctionEndTime::<Runtime>::contains_key(100, 0));
	});
}

#[test]
fn update_auction_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(AuctionModule::new_auction(10, Some(100)), 0);
		assert_noop!(
			AuctionModule::update_auction(
				1,
				AuctionInfo { bid: Some((ALICE, 100)), start: 10, end: Some(100) }
			),
			Error::<Runtime>::AuctionNotExist,
		);
		assert_ok!(AuctionModule::update_auction(
			0,
			AuctionInfo { bid: Some((ALICE, 100)), start: 10, end: Some(100) }
		));
	});
}

#[test]
fn auction_info_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(AuctionModule::new_auction(10, Some(100)), 0);
		assert_eq!(
			AuctionModule::auction_info(0),
			Some(AuctionInfo { bid: None, start: 10, end: Some(100) })
		);
	});
}

#[test]
fn bid_should_work() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(AuctionModule::new_auction(0, Some(5)), 0);
		assert_eq!(
			AuctionModule::auction_info(0),
			Some(AuctionInfo { bid: None, start: 0, end: Some(5) })
		);
		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(ALICE), 0, 20));
		System::assert_last_event(RuntimeEvent::AuctionModule(crate::Event::Bid {
			auction_id: 0,
			bidder: ALICE,
			amount: 20,
		}));
		assert_eq!(
			AuctionModule::auction_info(0),
			Some(AuctionInfo { bid: Some((ALICE, 20)), start: 0, end: Some(11) })
		);
	});
}

#[test]
fn bid_should_fail() {
	new_test_ext().execute_with(|| {
		assert_ok!(AuctionModule::new_auction(10, Some(100)), 0);
		assert_ok!(AuctionModule::new_auction(0, Some(100)), 1);
		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(ALICE), 0, 20),
			Error::<Runtime>::AuctionNotStarted
		);
		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(BOB), 1, 20),
			Error::<Runtime>::BidNotAccepted,
		);
		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(ALICE), 1, 0),
			Error::<Runtime>::InvalidBidPrice,
		);
		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(ALICE), 2, 10),
			Error::<Runtime>::AuctionNotExist,
		);
	});
}

#[test]
fn remove_auction_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(AuctionModule::new_auction(10, Some(100)), 0);
		assert_eq!(AuctionModule::auctions_index(), 1);
		assert!(AuctionModule::auctions(0).is_some());
		assert_eq!(AuctionModule::auction_end_time(100, 0), Some(()));
		AuctionModule::remove_auction(0);
		assert_eq!(AuctionModule::auctions(0), None);
		assert_eq!(AuctionModule::auction_end_time(100, 0), None);
	});
}

#[test]
fn cleanup_auction_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(AuctionModule::new_auction(10, Some(100)), 0);
		assert_eq!(AuctionModule::auctions_index(), 1);
		assert_ok!(AuctionModule::new_auction(10, Some(50)), 1);
		assert_eq!(AuctionModule::auctions_index(), 2);
		assert!(AuctionModule::auctions(0).is_some());
		assert!(AuctionModule::auctions(1).is_some());

		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(0).count(), 0);
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(50).count(), 1);
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(100).count(), 1);

		AuctionModule::on_finalize(50);
		assert!(AuctionModule::auctions(0).is_some());
		assert!(AuctionModule::auctions(1).is_none());
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(0).count(), 0);
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(50).count(), 0);
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(100).count(), 1);

		AuctionModule::on_finalize(100);
		assert!(AuctionModule::auctions(0).is_none());
		assert!(AuctionModule::auctions(1).is_none());
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(0).count(), 0);
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(50).count(), 0);
		assert_eq!(<AuctionEndTime<Runtime>>::iter_prefix(100).count(), 0);
	});
}
