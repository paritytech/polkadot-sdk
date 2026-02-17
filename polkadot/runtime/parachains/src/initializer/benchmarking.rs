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
use polkadot_primitives::ConsensusLog;
use sp_runtime::DigestItem;

// Random large number for the digest
const DIGEST_MAX_LEN: u32 = 65536;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn force_approve(d: Linear<0, DIGEST_MAX_LEN>) -> Result<(), BenchmarkError> {
		for _ in 0..d {
			frame_system::Pallet::<T>::deposit_log(ConsensusLog::ForceApprove(d).into());
		}

		#[extrinsic_call]
		_(RawOrigin::Root, d + 1);

		assert_eq!(
			frame_system::Pallet::<T>::digest().logs.last().unwrap(),
			&DigestItem::from(ConsensusLog::ForceApprove(d + 1)),
		);

		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
