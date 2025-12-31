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

//! Helper datatypes for XCM.

use super::*;
use alloc::boxed::Box;
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
