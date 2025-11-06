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

//! Pallet that serves no other purpose than benchmarking the `conviction-voting-precompiles` crate.
#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
mod mock;

#[frame_support::pallet]
pub mod pallet {
	#[pallet::config]
	pub trait Config<I: 'static = ()>:
		frame_system::Config + pallet_conviction_voting::Config<I> + pallet_revive::Config
	{
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);
}
