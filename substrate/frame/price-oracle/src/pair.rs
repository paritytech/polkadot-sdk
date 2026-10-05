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
//! What the runtime knows about pairs.

use alloc::vec::Vec;
use sp_price_oracle::PairId;

/// What the runtime knows about pairs.
///
/// The set of pairs is defined by the runtime, typically as an enum mapped to [`PairId`]s.
pub trait Pairs {
	/// All known pairs.
	fn all() -> Vec<PairId>;

	/// Whether `pair` is known. Votes and markets on unknown pairs are rejected.
	fn is_known(pair: PairId) -> bool {
		Self::all().contains(&pair)
	}

	/// How `pair` is derived from other pairs, besides the markets quoting it directly.
	///
	/// Each `(source, rate)` entry contributes the prices of the `source` markets multiplied by
	/// the median price of `rate`. E.g. DOT/USD derives from `(DOT/USDT, USDT/USD)`. Rate pairs
	/// are priced from their direct markets only.
	fn conversions(pair: PairId) -> Vec<(PairId, PairId)>;
}
