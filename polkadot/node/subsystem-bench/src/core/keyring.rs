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

pub use sp_core::sr25519;
use sp_core::{
	sr25519::{Pair, Public},
	Pair as PairT,
};
/// Set of test accounts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Keyring {
	name: String,
}

impl Keyring {
	pub fn new(name: String) -> Keyring {
		Self { name }
	}

	pub fn pair(self) -> Pair {
		Pair::from_string(&format!("//{}", self.name), None).expect("input is always good; qed")
	}

	pub fn public(self) -> Public {
		self.pair().public()
	}
}
