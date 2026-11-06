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
//! Traits for accessing prices.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_arithmetic::FixedU128;

/// A price with its age.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct PricePoint<BlockNumber> {
	/// The price, in quote asset units per one base asset unit, with 18 decimal places.
	pub price: FixedU128,
	/// Block number when the price was last computed. Advances on every computation, whether or
	/// not the price changed.
	pub updated_at: BlockNumber,
	/// Number of independent sources the price was computed from.
	pub signers: u32,
}

/// Read access to current prices.
pub trait PriceProvider {
	/// Identifies a pair of assets.
	type Pair;
	/// The block number type prices are stamped with.
	type BlockNumber;

	/// The current price of `pair`, or `None` if there is none.
	fn price(pair: Self::Pair) -> Option<PricePoint<Self::BlockNumber>>;
}
