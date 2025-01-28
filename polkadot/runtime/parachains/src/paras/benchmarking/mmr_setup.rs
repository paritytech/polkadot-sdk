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

//! Implements benchmarking setup for the `merkle-mountain-range` pallet.

use crate::paras::*;
use pallet_mmr::BenchmarkHelper;
use sp_std::vec;

/// Struct to setup benchmarks for the `merkle-mountain-range` pallet.
pub struct MmrSetup<T>(core::marker::PhantomData<T>);

impl<T> BenchmarkHelper for MmrSetup<T>
where
	T: Config,
{
	fn setup() {
		// Create a head with 1024 bytes of data.
		let head = vec![42u8; 1024];

		for para in 0..MAX_PARA_HEADS {
			let id = (para as u32).into();
			let h = head.clone().into();
			Pallet::<T>::heads_insert(&id, h);
		}
	}
}
