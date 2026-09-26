// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Benchmarks for `pallet-hrmp-relay`.

use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use hrmp_primitives::{ChannelId, DepositSide};

fn key(n: u32) -> DepositKey {
	DepositKey {
		channel: ChannelId { sender: 4_000 + n, recipient: 5_000 + n },
		side: DepositSide::Sender,
	}
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn receive() {
		let message = MessageToRelay::V1(MessageToRelayV1::HoldResult { key: key(0), held: true });

		#[extrinsic_call]
		_(RawOrigin::Root, message);
	}

	#[benchmark]
	fn relay_request() -> Result<(), BenchmarkError> {
		let origin =
			T::ParachainOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let request = ParaRequest::V1(ParaRequestV1::CloseChannel { channel: key(0).channel });

		#[block]
		{
			let _ = Pallet::<T>::relay_request(origin, request);
		}

		Ok(())
	}

	#[benchmark]
	fn flush_releases(n: Linear<0, 100>) {
		for i in 0..n {
			Pallet::<T>::release(key(i), None);
		}

		#[block]
		{
			Pallet::<T>::flush_releases();
		}

		assert!(PendingReleases::<T>::get().is_empty());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
