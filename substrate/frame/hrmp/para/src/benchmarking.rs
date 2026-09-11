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

//! Benchmarks for `pallet-hrmp-para`.
//!
//! One benchmark per [`WeightInfo`] method, named to match. Every path they drive is still a
//! `todo!()`, so they compile but panic when run. What each one has to arrange first lands with
//! the extrinsic body it covers, and so does `impl_benchmark_test_suite!`.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;

/// The para the benchmarks open channels from.
const SENDER: ParaId = 2000;
/// The para at the other end.
const RECIPIENT: ParaId = 2001;
/// A system chain. The runtime's `IsSystemPara` has to contain it, or every system path fails.
const SYSTEM: ParaId = 1000;
/// A second system chain, for the channel two system chains share.
const SYSTEM_PEER: ParaId = 1001;

/// The channel the benchmarks work on.
const CHANNEL: ChannelId = ChannelId { sender: SENDER, recipient: RECIPIENT };

fn relay_origin<T: Config>() -> Result<T::RuntimeOrigin, BenchmarkError> {
	T::RelayOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)
}

fn manager_origin<T: Config>() -> Result<T::RuntimeOrigin, BenchmarkError> {
	T::ChannelManager::try_successful_origin().map_err(|_| BenchmarkError::Weightless)
}

/// Give `para`'s sovereign account enough to put up either deposit for a channel this size.
fn fund<T: Config>(para: ParaId, max_capacity: u32) {
	let account = T::SovereignAccountOf::convert(para);
	let footprint = Pallet::<T>::channel_footprint(max_capacity);
	T::SenderConsideration::ensure_successful(&account, footprint);
	T::RecipientConsideration::ensure_successful(&account, footprint);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn receive_open_channel_response() -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		// TODO: leave an accepted request for `CHANNEL` in `Requests`.
		let report = MessageToPara::V1(MessageToParaV1::OpenChannelResponse {
			channel: CHANNEL,
			message_id: 0,
			outcome: Ok((T::MaxCapacity::get(), T::MaxMessageSize::get())),
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, report);

		assert!(Channels::<T>::contains_key(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn receive_close_response() -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		// TODO: leave an open `CHANNEL` and its close request in storage.
		let report = MessageToPara::V1(MessageToParaV1::CloseResponse {
			channel: CHANNEL,
			message_id: 0,
			outcome: Ok(()),
		});

		#[extrinsic_call]
		receive(origin as T::RuntimeOrigin, report);

		assert!(!Channels::<T>::contains_key(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn hrmp_init_open_channel() -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		let max_capacity = T::MaxCapacity::get();
		fund::<T>(SENDER, max_capacity);
		let request = ParaRequest::V1(ParaRequestV1::InitOpenChannel {
			recipient: RECIPIENT,
			proposed_max_capacity: max_capacity,
			proposed_max_message_size: T::MaxMessageSize::get(),
		});

		#[extrinsic_call]
		receive_request(origin as T::RuntimeOrigin, SENDER, request);

		assert!(Requests::<T>::contains_key(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn hrmp_accept_open_channel() -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		fund::<T>(RECIPIENT, T::MaxCapacity::get());
		// TODO: leave `SENDER`'s open request for `CHANNEL` in `Requests`.
		let request = ParaRequest::V1(ParaRequestV1::AcceptOpenChannel { sender: SENDER });

		#[extrinsic_call]
		receive_request(origin as T::RuntimeOrigin, RECIPIENT, request);

		assert!(Requests::<T>::contains_key(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn hrmp_close_channel() -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		// TODO: leave an open `CHANNEL` in `Channels`.
		let request = ParaRequest::V1(ParaRequestV1::CloseChannel { channel: CHANNEL });

		#[extrinsic_call]
		receive_request(origin as T::RuntimeOrigin, SENDER, request);

		assert!(CloseRequests::<T>::contains_key(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn hrmp_cancel_open_request(
		// Open requests `SENDER` has initiated, the one being withdrawn among them.
		c: Linear<1, { T::MaxOutboundChannels::get() }>,
	) -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		// TODO: leave `c` of `SENDER`'s open requests in `Requests`, `CHANNEL` among them.
		let request = ParaRequest::V1(ParaRequestV1::CancelOpenRequest {
			channel: CHANNEL,
			open_requests: c,
		});

		#[extrinsic_call]
		receive_request(origin as T::RuntimeOrigin, SENDER, request);

		assert!(!Requests::<T>::contains_key(CHANNEL));
		Ok(())
	}

	#[benchmark]
	fn establish_channel_with_system() -> Result<(), BenchmarkError> {
		let origin = relay_origin::<T>()?;
		let request = ParaRequest::V1(ParaRequestV1::EstablishChannelWithSystem {
			target_system_chain: SYSTEM,
		});

		#[extrinsic_call]
		receive_request(origin as T::RuntimeOrigin, SENDER, request);

		assert!(Requests::<T>::contains_key(ChannelId { sender: SENDER, recipient: SYSTEM }));
		Ok(())
	}

	#[benchmark]
	fn force_clean_hrmp(
		// Inbound channels `SENDER` has, all of which are dropped.
		i: Linear<0, { T::MaxInboundChannels::get() }>,
		// Outbound channels `SENDER` has, all of which are dropped.
		e: Linear<0, { T::MaxOutboundChannels::get() }>,
	) -> Result<(), BenchmarkError> {
		let origin = manager_origin::<T>()?;
		// TODO: give `SENDER` `i` inbound and `e` outbound channels.

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, SENDER, i, e);

		assert!(IngressIndex::<T>::get(SENDER).is_empty());
		assert!(EgressIndex::<T>::get(SENDER).is_empty());
		Ok(())
	}

	#[benchmark]
	fn force_process_hrmp_open(
		// Accepted requests waiting to be opened.
		c: Linear<0, { T::MaxOutboundChannels::get() }>,
	) -> Result<(), BenchmarkError> {
		let origin = manager_origin::<T>()?;
		// TODO: leave `c` accepted requests in `Requests`.

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, c);

		Ok(())
	}

	#[benchmark]
	fn force_process_hrmp_close(
		// Close requests waiting to be enacted.
		c: Linear<0, { T::MaxOutboundChannels::get() }>,
	) -> Result<(), BenchmarkError> {
		let origin = manager_origin::<T>()?;
		// TODO: leave `c` close requests in `CloseRequests`.

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, c);

		Ok(())
	}

	#[benchmark]
	fn force_open_hrmp_channel(
		// `1` when a request for the channel already exists and has to be cleared first.
		c: Linear<0, 1>,
	) -> Result<(), BenchmarkError> {
		let origin = manager_origin::<T>()?;
		let max_capacity = T::MaxCapacity::get();
		fund::<T>(SENDER, max_capacity);
		// TODO: when `c` is 1, leave `SENDER`'s open request for `CHANNEL` in `Requests`.
		let _ = c;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, SENDER, RECIPIENT, max_capacity, T::MaxMessageSize::get());

		assert!(Requests::<T>::contains_key(CHANNEL));
		Ok(())
	}

	// Permissionless, as on the relay chain: both ends being system chains is what authorizes it.
	#[benchmark]
	fn establish_system_channel() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), SYSTEM, SYSTEM_PEER);

		assert!(Requests::<T>::contains_key(ChannelId { sender: SYSTEM, recipient: SYSTEM_PEER }));
		Ok(())
	}

	#[benchmark]
	fn poke_channel_deposits() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let max_capacity = T::MaxCapacity::get();
		fund::<T>(SENDER, max_capacity);
		fund::<T>(RECIPIENT, max_capacity);
		// TODO: leave an open `CHANNEL` whose held deposits are below the current price.

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), SENDER, RECIPIENT);

		assert!(Channels::<T>::contains_key(CHANNEL));
		Ok(())
	}
}
