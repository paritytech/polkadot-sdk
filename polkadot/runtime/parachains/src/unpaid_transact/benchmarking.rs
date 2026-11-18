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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Authorize;
use frame_system::RawOrigin;

/// Gives a window to a parachain that is not registered, and returns it.
fn offboarded_para<T: Config>() -> ParaId {
	let para = ParaId::from(1_000_000);
	Windows::<T>::insert(para, (BlockNumberFor::<T>::zero(), 1));
	para
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn remove_offboarded() {
		let para = offboarded_para::<T>();

		#[extrinsic_call]
		_(RawOrigin::Authorized, para);

		assert!(!Windows::<T>::contains_key(para));
	}

	#[benchmark]
	fn authorize_remove_offboarded() -> Result<(), BenchmarkError> {
		let call = Call::<T>::remove_offboarded { para: offboarded_para::<T>() };

		#[block]
		{
			call.authorize(TransactionSource::InBlock)
				.expect("`remove_offboarded` has authorize logic")
				.map_err(|_| "authorize failed")?;
		}

		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
