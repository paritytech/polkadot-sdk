// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

//! The pallet benchmarks.

use super::{Pallet as CollectiveContent, *};
use frame_benchmarking::{impl_benchmark_test_suite, v2::*};
use frame_support::traits::EnsureOrigin;

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

/// returns CID hash of 68 bytes of given `i`.
fn create_cid(i: u8) -> OpaqueCid {
	let cid: OpaqueCid = [i; 68].to_vec().try_into().unwrap();
	cid
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_charter() -> Result<(), BenchmarkError> {
		let cid: OpaqueCid = create_cid(1);
		let origin =
			T::CharterOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, cid.clone());

		assert_eq!(Charter::<T, I>::get(), Some(cid.clone()));
		assert_last_event::<T, I>(Event::NewCharterSet { cid }.into());
		Ok(())
	}

	#[benchmark]
	fn announce() -> Result<(), BenchmarkError> {
		let expire_at = DispatchTime::<_>::At(10u32.into());
		let now = frame_system::Pallet::<T>::block_number();
		let cid: OpaqueCid = create_cid(1);
		let origin = T::AnnouncementOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, cid.clone(), Some(expire_at));

		assert_eq!(<Announcements<T, I>>::count(), 1);
		assert_last_event::<T, I>(
			Event::AnnouncementAnnounced { cid, expire_at: expire_at.evaluate(now) }.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn remove_announcement() -> Result<(), BenchmarkError> {
		let cid: OpaqueCid = create_cid(1);
		let origin = T::AnnouncementOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		CollectiveContent::<T, I>::announce(origin.clone(), cid.clone(), None)
			.expect("could not publish an announcement");
		assert_eq!(<Announcements<T, I>>::count(), 1);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, cid.clone());

		assert_eq!(<Announcements<T, I>>::count(), 0);
		assert_last_event::<T, I>(Event::AnnouncementRemoved { cid }.into());

		Ok(())
	}

	impl_benchmark_test_suite!(CollectiveContent, super::mock::new_bench_ext(), super::mock::Test);
}
