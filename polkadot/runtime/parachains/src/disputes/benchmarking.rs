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

use super::*;

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::One;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn force_unfreeze() {
		Frozen::<T>::set(Some(One::one()));

		#[extrinsic_call]
		_(RawOrigin::Root);

		assert!(Frozen::<T>::get().is_none())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
