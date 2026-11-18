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

/// Gives a window to `p` parachains that are not registered, and returns them sorted.
fn offboarded_paras<T: Config>(p: u32) -> OffboardedParas {
	let paras = (0..p).map(|i| ParaId::from(1_000_000 + i)).collect::<Vec<_>>();
	for para in &paras {
		Windows::<T>::insert(para, (BlockNumberFor::<T>::zero(), 1));
	}
	OffboardedParas::truncate_from(paras)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn remove_offboarded(p: Linear<1, MAX_REMOVED_PER_CALL>) {
		let paras = offboarded_paras::<T>(p);

		#[extrinsic_call]
		_(RawOrigin::Authorized, paras);

		assert_eq!(Windows::<T>::iter().count(), 0);
	}

	#[benchmark]
	fn authorize_remove_offboarded(
		p: Linear<1, MAX_REMOVED_PER_CALL>,
	) -> Result<(), BenchmarkError> {
		let paras = offboarded_paras::<T>(p);
		let call = Call::<T>::remove_offboarded { paras };

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
