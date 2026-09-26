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
//! Shows how a runtime plugs into the pallet. Deliberately minimal.

use crate::pair::Pairs;
use alloc::vec::Vec;
use sp_price_oracle::PairId;

/// The pairs of a runtime. Discriminants are the wire identifiers and must never change.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum Pair {
	DotUsdt = 1,
	DotUsd = 2,
	UsdtUsd = 3,
}

impl TryFrom<PairId> for Pair {
	type Error = ();

	fn try_from(id: PairId) -> Result<Self, ()> {
		Ok(match id.0 {
			1 => Pair::DotUsdt,
			2 => Pair::DotUsd,
			3 => Pair::UsdtUsd,
			_ => return Err(()),
		})
	}
}

impl From<Pair> for PairId {
	fn from(pair: Pair) -> PairId {
		PairId(pair as u16)
	}
}

impl Pairs for Pair {
	fn all() -> Vec<PairId> {
		alloc::vec![Pair::DotUsdt.into(), Pair::DotUsd.into(), Pair::UsdtUsd.into()]
	}

	fn conversions(pair: PairId) -> Vec<(PairId, PairId)> {
		match Pair::try_from(pair) {
			Ok(Pair::DotUsd) => alloc::vec![(Pair::DotUsdt.into(), Pair::UsdtUsd.into())],
			_ => Vec::new(),
		}
	}
}
