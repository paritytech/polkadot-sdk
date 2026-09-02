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

//! Types used by this pallet.

use crate::{Config, RelayBlockNumberOf};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use fp_coretime::TaskId;
use frame_support::traits::fungible::Inspect;
use frame_system::Config as SConfig;
use scale_info::TypeInfo;
use sp_arithmetic::Perbill;

pub type BalanceOf<T> = <<T as Config>::Currency as Inspect<<T as SConfig>::AccountId>>::Balance;

/// The parameters used for pricing on-demand orders.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct PriceParameters<Balance> {
	/// The maximum number of outstanding orders beyond which we reject new orders.
	pub order_cap: u32,
	/// The number of outstanding orders assumed to be drained per Relay-chain block.
	pub drain_rate_per_block: u32,
	/// The spot price increase per outstanding order in the queue.
	pub price_step: Perbill,
	/// The base spot price when the queue is empty.
	pub base_fee: Balance,
}

pub type PriceParametersOf<T> = PriceParameters<BalanceOf<T>>;

/// Data about a placed on-demand order.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct EnqueuedOrder<RelayBlockNumber> {
	/// The parachain the order was placed for.
	pub para_id: TaskId,
	/// The Relay-chain block number the order came in at.
	pub ordered_at: RelayBlockNumber,
}

/// The locally tracked, estimated state of the Relay chain's on-demand order queue.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct QueueTracker<RelayBlockNumber> {
	/// The current estimated number of outstanding orders.
	pub outstanding_orders: u32,
	/// When the queue state was last updated.
	pub last_updated: RelayBlockNumber,
}

pub type QueueTrackerOf<T> = QueueTracker<RelayBlockNumberOf<T>>;
