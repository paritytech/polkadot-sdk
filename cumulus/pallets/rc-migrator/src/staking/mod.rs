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

pub mod bags_list;
pub mod fast_unstake;
pub mod nom_pools;
pub mod nom_pools_alias;

#[cfg(feature = "ahm-staking-migration")]
pub mod message;
#[cfg(feature = "ahm-staking-migration")]
pub mod staking;
#[cfg(feature = "ahm-staking-migration")]
pub use staking::*;

// Copy&paster of Convert trait so that we can implement it here on external types
/// Infallible conversion trait. Generic over both source and destination types.
pub trait IntoAh<A, B> {
	/// Make conversion.
	fn intoAh(a: A) -> B;
}
