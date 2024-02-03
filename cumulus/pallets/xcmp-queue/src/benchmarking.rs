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

//! Benchmarking setup for cumulus-pallet-xcmp-queue

use crate::*;

use codec::DecodeAll;
use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use frame_system::RawOrigin;
use xcm::v3::MAX_INSTRUCTIONS_TO_DECODE;

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
		Pallet::<T>::update_resume_threshold(RawOrigin::Root, 1);
	}

	#[benchmark]
	fn enqueue_xcmp_message() {
		assert!(QueueConfig::<T>::get().drop_threshold * MaxXcmpMessageLenOf::<T>::get() > 1000);
		let msg = BoundedVec::<u8, MaxXcmpMessageLenOf<T>>::default();

		#[block]
		{
			Pallet::<T>::enqueue_xcmp_message(0.into(), msg, &mut WeightMeter::new()).unwrap();
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
			OutboundXcmpStatus::<T>::get().iter().all(|p| p.recipient != para),
			"No messages in the channel; therefore removed."
		);
	}

	/// Split a singular XCM.
	#[benchmark]
	fn take_first_concatenated_xcm() {
		let max_downward_message_size = MaxXcmpMessageLenOf::<T>::get() as usize;

		assert!(MAX_INSTRUCTIONS_TO_DECODE as u32 > MAX_XCM_DECODE_DEPTH, "Preconditon failed");
		let max_instrs = MAX_INSTRUCTIONS_TO_DECODE as u32 - MAX_XCM_DECODE_DEPTH;
		let mut xcm = Xcm::<T>(vec![ClearOrigin; max_instrs as usize]);

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
			Pallet::<T>::take_first_concatenated_xcm(&mut &data[..], &mut WeightMeter::new())
				.unwrap();
		}
	}

	/// Benchmark the migration for a maximal sized message.
	#[benchmark]
	fn on_idle_good_msg() {
		use migration::v3;

		let block = 5;
		let para = ParaId::from(4);
		let message = vec![123u8; MaxXcmpMessageLenOf::<T>::get() as usize];
		let message_metadata = vec![(block, XcmpMessageFormat::ConcatenatedVersionedXcm)];

		v3::InboundXcmpMessages::<T>::insert(para, block, message);
		v3::InboundXcmpStatus::<T>::set(Some(vec![v3::InboundChannelDetails {
			sender: para,
			state: v3::InboundState::Ok,
			message_metadata,
		}]));

		#[block]
		{
			Pallet::<T>::on_idle(0u32.into(), Weight::MAX);
		}
	}

	/// Benchmark the migration with a 64 KiB message that will not be possible to enqueue.
	#[benchmark]
	fn on_idle_large_msg() {
		use migration::v3;

		let block = 5;
		let para = ParaId::from(4);
		let message = vec![123u8; 1 << 16]; // 64 KiB message
		let message_metadata = vec![(block, XcmpMessageFormat::ConcatenatedVersionedXcm)];

		v3::InboundXcmpMessages::<T>::insert(para, block, message);
		v3::InboundXcmpStatus::<T>::set(Some(vec![v3::InboundChannelDetails {
			sender: para,
			state: v3::InboundState::Ok,
			message_metadata,
		}]));

		#[block]
		{
			Pallet::<T>::on_idle(0u32.into(), Weight::MAX);
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
