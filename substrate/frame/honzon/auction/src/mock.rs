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

//! Mocks for the auction module.

#![cfg(test)]

use super::*;
use frame_support::{
	construct_runtime, derive_impl,
	traits::{AuctionHandler, Change, OnNewBidResult},
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

use crate as pallet_auction;

pub type AccountId = u128;
pub type Balance = u64;
pub type BlockNumber = u64;
pub type AuctionId = u64;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Nonce = u64;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const BID_EXTEND_BLOCK: BlockNumber = 10;
pub struct Handler;

impl AuctionHandler<AccountId, Balance, BlockNumber, AuctionId> for Handler {
	fn on_new_bid(
		now: BlockNumber,
		_id: AuctionId,
		new_bid: (AccountId, Balance),
		_last_bid: Option<(AccountId, Balance)>,
	) -> OnNewBidResult<BlockNumber> {
		if should_accept_bid(&new_bid.0) {
			OnNewBidResult {
				accept_bid: true,
				auction_end_change: Change::NewValue(Some(now + BID_EXTEND_BLOCK)),
			}
		} else {
			OnNewBidResult { accept_bid: false, auction_end_change: Change::NoChange }
		}
	}

	fn on_auction_ended(_id: AuctionId, _winner: Option<(AccountId, Balance)>) {}
}

impl Config for Runtime {
	type Balance = Balance;
	type AuctionId = AuctionId;
	type Handler = Handler;
	type WeightInfo = ();
}

type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		AuctionModule: pallet_auction,
	}
);

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	t.into()
}

fn should_accept_bid(bidder: &AccountId) -> bool {
	if *bidder == ALICE {
		return true;
	}

	// For mock benchmarking
	#[cfg(feature = "runtime-benchmarks")]
	{
		if *bidder == frame_benchmarking::whitelisted_caller::<AccountId>() {
			return true;
		}
	}

	false
}
