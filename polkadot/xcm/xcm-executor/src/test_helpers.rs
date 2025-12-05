// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Helper datatypes for XCM.

use super::*;
use frame_support::traits::tokens::imbalance::{
	ImbalanceAccounting, UnsafeConstructorDestructor, UnsafeManualAccounting,
};

/// Mock credit for tests
pub struct MockCredit(pub u128);

impl UnsafeConstructorDestructor<u128> for MockCredit {
	fn unsafe_clone(&self) -> Box<dyn ImbalanceAccounting<u128>> {
		Box::new(MockCredit(self.0))
	}
	fn forget_imbalance(&mut self) -> u128 {
		let amt = self.0;
		self.0 = 0;
		amt
	}
}

impl UnsafeManualAccounting<u128> for MockCredit {
	fn subsume_other(&mut self, mut other: Box<dyn ImbalanceAccounting<u128>>) {
		self.0 = self.0.saturating_add(other.forget_imbalance());
	}
}

impl ImbalanceAccounting<u128> for MockCredit {
	fn amount(&self) -> u128 {
		self.0
	}
	fn saturating_take(&mut self, amount: u128) -> Box<dyn ImbalanceAccounting<u128>> {
		let taken = self.0.min(amount);
		self.0 -= taken;
		Box::new(MockCredit(taken))
	}
}

pub fn mock_asset_to_holding(asset: Asset) -> AssetsInHolding {
	let mut holding = AssetsInHolding::new();
	match asset.fun {
		Fungible(amount) => {
			holding.fungible.insert(asset.id, Box::new(MockCredit(amount)));
		},
		NonFungible(instance) => {
			holding.non_fungible.insert((asset.id, instance));
		},
	}
	holding
}
