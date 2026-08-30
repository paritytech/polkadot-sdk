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

/// A channel no benchmark setup will collide on.
const CHANNEL: ChannelId = ChannelId { sender: 4_242, recipient: 4_243 };

const CAPACITY: u32 = 8;
const MESSAGE_SIZE: u32 = 1_024;

fn init_msg() -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::InitOpenChannel {
		channel: CHANNEL,
		message_id: 0,
		max_capacity: CAPACITY,
		max_message_size: MESSAGE_SIZE,
	})
}

/// Put a request on the registry, so the calls that act on one have something to find.
fn request<T: Config>() -> Result<(), BenchmarkError> {
	T::Registry::ensure_openable(CHANNEL);
	Pallet::<T>::receive(RawOrigin::Root.into(), init_msg())?;
	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Recording a request. Bounds checks in the registry plus one report.
	#[benchmark]
	fn init_open_channel() -> Result<(), BenchmarkError> {
		T::Registry::ensure_openable(CHANNEL);

		#[extrinsic_call]
		receive(RawOrigin::Root, init_msg());

		assert!(T::Registry::exists(CHANNEL));
		Ok(())
	}

	/// Confirming a request.
	#[benchmark]
	fn accept_open_channel() -> Result<(), BenchmarkError> {
		request::<T>()?;

		#[extrinsic_call]
		receive(
			RawOrigin::Root,
			MessageToRelay::V1(MessageToRelayV1::AcceptOpenChannel {
				channel: CHANNEL,
				message_id: 1,
			}),
		);

		Ok(())
	}

	/// Closing a channel.
	#[benchmark]
	fn close_channel() -> Result<(), BenchmarkError> {
		request::<T>()?;
		Pallet::<T>::receive(
			RawOrigin::Root.into(),
			MessageToRelay::V1(MessageToRelayV1::AcceptOpenChannel {
				channel: CHANNEL,
				message_id: 1,
			}),
		)?;

		#[extrinsic_call]
		receive(
			RawOrigin::Root,
			MessageToRelay::V1(MessageToRelayV1::CloseChannel {
				channel: CHANNEL,
				message_id: 2,
				initiator: CHANNEL.sender,
			}),
		);

		Ok(())
	}

	/// Dropping an unconfirmed request.
	#[benchmark]
	fn cancel_open_request() -> Result<(), BenchmarkError> {
		request::<T>()?;

		#[extrinsic_call]
		receive(
			RawOrigin::Root,
			MessageToRelay::V1(MessageToRelayV1::CancelOpenRequest {
				channel: CHANNEL,
				message_id: 2,
			}),
		);

		Ok(())
	}

	/// A system channel: two channels opened in one call, and no report sent.
	#[benchmark]
	fn establish_system_channel() -> Result<(), BenchmarkError> {
		T::Registry::ensure_openable(CHANNEL);

		#[extrinsic_call]
		receive(
			RawOrigin::Root,
			MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel {
				channel: CHANNEL,
				message_id: 3,
			}),
		);

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
