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
//!
//! One benchmark per [`WeightInfo`] method, named to match, so each `receive` variant is weighed
//! by its own. Only `relay_request` and `receive_notify_para` have bodies to measure; the rest
//! compile but panic until their handler and the registry behind it land, and so does
//! `impl_benchmark_test_suite!`.

use super::*;
use frame_benchmarking::v2::*;
use hrmp_primitives::ParaRequestV1;

/// The para that sends on the channel the benchmarks work on.
const SENDER: ParaId = 2000;
/// The para that receives on it.
const RECIPIENT: ParaId = 2001;

/// The channel the benchmarks work on.
const CHANNEL: ChannelId = ChannelId { sender: SENDER, recipient: RECIPIENT };

/// Sizes asked for. The relay half holds no limits of its own: they arrive on the wire and the
/// registry is what accepts or refuses them.
const MAX_CAPACITY: u32 = 1_000;
const MAX_MESSAGE_SIZE: u32 = 1_024;

fn para_origin<T: Config>() -> Result<T::RuntimeOrigin, BenchmarkError> {
	T::ParaOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)
}

fn assert_last_event<T: Config>(event: Event<T>) {
	let event = <T as Config>::RuntimeEvent::from(event);
	frame_system::Pallet::<T>::assert_last_event(event.into());
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn receive_open_channel() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		T::Registry::ensure_openable(CHANNEL);
		let message = MessageToRelay::V1(MessageToRelayV1::OpenChannel {
			channel: CHANNEL,
			message_id: 0,
			max_capacity: MAX_CAPACITY,
			max_message_size: MAX_MESSAGE_SIZE,
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		assert!(T::Registry::exists(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn receive_force_open_channel() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		T::Registry::ensure_openable(CHANNEL);
		let message = MessageToRelay::V1(MessageToRelayV1::ForceOpenChannel {
			channel: CHANNEL,
			message_id: 0,
			max_capacity: MAX_CAPACITY,
			max_message_size: MAX_MESSAGE_SIZE,
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		assert!(T::Registry::exists(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn receive_open_system_channel() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		T::Registry::ensure_openable(CHANNEL);
		let message = MessageToRelay::V1(MessageToRelayV1::OpenSystemChannel {
			channel: CHANNEL,
			message_id: 0,
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		assert!(T::Registry::exists(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn receive_open_system_pair() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		T::Registry::ensure_openable(CHANNEL);
		T::Registry::ensure_openable(CHANNEL.reversed());
		let message = MessageToRelay::V1(MessageToRelayV1::OpenSystemPair {
			channel: CHANNEL,
			message_id: 0,
			max_capacity: MAX_CAPACITY,
			max_message_size: MAX_MESSAGE_SIZE,
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		assert!(T::Registry::exists(CHANNEL));
		assert!(T::Registry::exists(CHANNEL.reversed()));
		Ok(())
	}

	#[benchmark]
	fn receive_close_channel() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		T::Registry::ensure_openable(CHANNEL);
		T::Registry::open_channel(CHANNEL, MAX_CAPACITY, MAX_MESSAGE_SIZE)
			.map_err(|_| BenchmarkError::Stop("the registry refused to open the channel"))?;
		let message = MessageToRelay::V1(MessageToRelayV1::CloseChannel {
			channel: CHANNEL,
			message_id: 0,
			initiator: SENDER,
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		assert!(!T::Registry::exists(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn receive_force_clean() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		T::Registry::ensure_openable(CHANNEL);
		T::Registry::open_channel(CHANNEL, MAX_CAPACITY, MAX_MESSAGE_SIZE)
			.map_err(|_| BenchmarkError::Stop("the registry refused to open the channel"))?;
		let message =
			MessageToRelay::V1(MessageToRelayV1::ForceClean { para_id: SENDER, message_id: 0 });

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		assert!(!T::Registry::exists(CHANNEL));
		Ok(())
	}

	// The closing notification, which carries the most.
	#[benchmark]
	fn receive_notify_para() -> Result<(), BenchmarkError> {
		let origin = para_origin::<T>()?;
		let message = MessageToRelay::V1(MessageToRelayV1::NotifyPara {
			para_id: RECIPIENT,
			notification: ParaNotification::ChannelClosing {
				initiator: SENDER,
				sender: SENDER,
				recipient: RECIPIENT,
			},
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, message);

		Ok(())
	}

	#[benchmark]
	fn relay_request() -> Result<(), BenchmarkError> {
		let origin =
			T::ParachainOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let para_id = T::ParachainOrigin::ensure_origin(origin.clone())
			.map_err(|_| BenchmarkError::Weightless)?;
		let request = ParaRequest::V1(ParaRequestV1::InitOpenChannel {
			recipient: RECIPIENT,
			proposed_max_capacity: MAX_CAPACITY,
			proposed_max_message_size: MAX_MESSAGE_SIZE,
		});

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, request);

		assert_last_event::<T>(Event::RequestForwarded { para_id });
		Ok(())
	}
}
