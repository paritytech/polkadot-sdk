// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Benchmarking setup for cumulus-pallet-xcmp-queue

use crate::*;

use codec::DecodeAll;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Modify any of the `QueueConfig` fields with a new `u32` value.
	///
	/// Used as weight for:
	/// - update_suspend_threshold
	/// - update_drop_threshold
	/// - update_resume_threshold
	#[benchmark]
	fn set_config_with_u32() {
		#[extrinsic_call]
		Pallet::<T>::update_resume_threshold(RawOrigin::Root, 100);
	}

	#[benchmark]
	fn enqueue_xcmp_messages(n: Linear<0, 1000>) {
		assert!(QueueConfig::<T>::get().drop_threshold > 1000);
		let msg = BoundedVec::<u8, MaxXcmpMessageLenOf<T>>::default();
		let msgs = vec![msg; n as usize];

		#[block]
		{
			Pallet::<T>::enqueue_xcmp_messages(0.into(), msgs, &mut WeightMeter::max_limit());
		}
	}

	#[benchmark]
	fn suspend_channel() {
		let para = 123.into();
		let data = ChannelSignal::Suspend.encode();

		#[block]
		{
			ChannelSignal::decode_all(&mut &data[..]).unwrap();
			Pallet::<T>::suspend_channel(para);
		}

		assert_eq!(
			OutboundXcmpStatus::<T>::get()
				.iter()
				.find(|p| p.recipient == para)
				.unwrap()
				.state,
			OutboundState::Suspended
		);
	}

	#[benchmark]
	fn resume_channel() {
		let para = 123.into();
		let data = ChannelSignal::Resume.encode();

		Pallet::<T>::suspend_channel(para);

		#[block]
		{
			ChannelSignal::decode_all(&mut &data[..]).unwrap();
			Pallet::<T>::resume_channel(para);
		}

		assert!(
			OutboundXcmpStatus::<T>::get().iter().find(|p| p.recipient == para).is_none(),
			"No messages in the channel; therefore removed."
		);
	}

	/// Split a singular XCM.
	#[benchmark]
	fn split_concatenated_xcm() {
		let max_downward_message_size = MaxXcmpMessageLenOf::<T>::get() as usize;

		// A nested XCM of length 100:
		// NOTE: If this fails because of a custom XCM decoder then you need to reduce it.
		let mut xcm = Xcm::<T>(vec![ClearOrigin; 100]);

		for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
			xcm = Xcm::<T>(vec![Instruction::SetAppendix(xcm)]);
		}

		let data = VersionedXcm::<T>::from(xcm).encode();
		assert!(data.len() < max_downward_message_size, "Page size is too small");
		// Verify that decoding works with the exact recursion limit:
		VersionedXcm::<T::RuntimeCall>::decode_with_depth_limit(
			MAX_XCM_DECODE_DEPTH,
			&mut &data[..],
		)
		.unwrap();
		VersionedXcm::<T::RuntimeCall>::decode_with_depth_limit(
			MAX_XCM_DECODE_DEPTH - 1,
			&mut &data[..],
		)
		.unwrap_err();

		#[block]
		{
			Pallet::<T>::split_concatenated_xcms(&mut &data[..], &mut WeightMeter::max_limit())
				.unwrap();
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
