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

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Contains;
use frame_system::RawOrigin;

/// Two public para ids no benchmark setup will collide on, and which are never system chains, so
/// the deposit path is the one being measured.
fn pair<T: Config>() -> (ParaId, ParaId) {
	// Deliberately not system paras, so the deposit path is the one being measured. Asserted
	// rather than assumed: a runtime whose `SystemParas` swallowed these would silently benchmark
	// the free path instead.
	let (sender, recipient) = (4_242, 4_243);
	assert!(
		!T::SystemParas::contains(&sender) && !T::SystemParas::contains(&recipient),
		"benchmark para ids must not be system chains"
	);
	(sender, recipient)
}

/// Fund both ends' sovereign accounts so every consideration this pallet takes can be paid.
fn fund<T: Config>(channel: ChannelId) {
	for para in [channel.sender, channel.recipient] {
		let who = T::SovereignAccountOf::convert(para);
		T::ChannelConsideration::ensure_successful(&who, Pallet::<T>::channel_footprint());
	}
}

/// A channel the relay chain is holding a request for.
fn pending<T: Config>(channel: ChannelId) -> Result<(), BenchmarkError> {
	fund::<T>(channel);
	Pallet::<T>::open_channel(
		RawOrigin::Root.into(),
		channel.sender,
		channel.recipient,
		T::MaxCapacity::get(),
		T::MaxMessageSize::get(),
	)?;
	Pallet::<T>::receive(
		RawOrigin::Root.into(),
		MessageToPara::V1(MessageToParaV1::OpenResponse {
			channel,
			message_id: 0,
			outcome: Ok(()),
		}),
	)?;
	Ok(())
}

/// A channel that is open at both ends.
fn opened<T: Config>(channel: ChannelId) -> Result<(), BenchmarkError> {
	pending::<T>(channel)?;
	Pallet::<T>::accept_open_channel(
		RawOrigin::Root.into(),
		channel.sender,
		channel.recipient,
	)?;
	Pallet::<T>::receive(
		RawOrigin::Root.into(),
		MessageToPara::V1(MessageToParaV1::AcceptResponse {
			channel,
			message_id: 1,
			outcome: Ok(()),
		}),
	)?;
	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Requesting a channel: one deposit taken, one write, one message.
	#[benchmark]
	fn open_channel() -> Result<(), BenchmarkError> {
		let (sender, recipient) = pair::<T>();
		let channel = ChannelId { sender, recipient };
		fund::<T>(channel);

		#[extrinsic_call]
		_(
			RawOrigin::Root,
			sender,
			recipient,
			T::MaxCapacity::get(),
			T::MaxMessageSize::get(),
		);

		assert!(matches!(
			Channels::<T>::get(channel).map(|c| c.state),
			Some(ChannelState::Opening { .. })
		));
		Ok(())
	}

	/// Accepting: the second deposit, one write, one message.
	#[benchmark]
	fn accept_open_channel() -> Result<(), BenchmarkError> {
		let (sender, recipient) = pair::<T>();
		let channel = ChannelId { sender, recipient };
		pending::<T>(channel)?;

		#[extrinsic_call]
		_(RawOrigin::Root, sender, recipient);

		assert!(matches!(
			Channels::<T>::get(channel).map(|c| c.state),
			Some(ChannelState::Accepting { .. })
		));
		Ok(())
	}

	/// Closing: nothing is released here, so this is the state write plus the message.
	#[benchmark]
	fn close_channel() -> Result<(), BenchmarkError> {
		let (sender, recipient) = pair::<T>();
		let channel = ChannelId { sender, recipient };
		opened::<T>(channel)?;

		#[extrinsic_call]
		_(RawOrigin::Root, sender, recipient);

		assert!(matches!(
			Channels::<T>::get(channel).map(|c| c.state),
			Some(ChannelState::Closing { .. })
		));
		Ok(())
	}

	/// Cancelling: same shape as closing.
	#[benchmark]
	fn cancel_open_request() -> Result<(), BenchmarkError> {
		let (sender, recipient) = pair::<T>();
		let channel = ChannelId { sender, recipient };
		pending::<T>(channel)?;

		#[extrinsic_call]
		_(RawOrigin::Root, sender, recipient);

		assert!(matches!(
			Channels::<T>::get(channel).map(|c| c.state),
			Some(ChannelState::Cancelling { .. })
		));
		Ok(())
	}

	/// The worst case of the messages this call serves is a confirmed close, which releases both
	/// deposits and removes the entry.
	#[benchmark]
	fn receive() -> Result<(), BenchmarkError> {
		let (sender, recipient) = pair::<T>();
		let channel = ChannelId { sender, recipient };
		opened::<T>(channel)?;
		Pallet::<T>::close_channel(RawOrigin::Root.into(), sender, recipient)?;

		#[extrinsic_call]
		_(
			RawOrigin::Root,
			MessageToPara::V1(MessageToParaV1::CloseResponse {
				channel,
				message_id: 2,
				outcome: Ok(()),
			}),
		);

		assert!(Channels::<T>::get(channel).is_none());
		Ok(())
	}

	/// A system channel: two writes and one message, no deposits.
	#[benchmark]
	fn establish_system_channel() -> Result<(), BenchmarkError> {
		let (_, recipient) = pair::<T>();
		let here = T::SelfParaId::get();

		#[extrinsic_call]
		_(RawOrigin::Root, here, recipient);

		assert_eq!(
			Channels::<T>::get(ChannelId { sender: here, recipient }).map(|c| c.state),
			Some(ChannelState::Open)
		);
		Ok(())
	}

	/// Governance tearing a channel down. Worst case releases both deposits.
	#[benchmark]
	fn force_remove_channel() -> Result<(), BenchmarkError> {
		let (sender, recipient) = pair::<T>();
		let channel = ChannelId { sender, recipient };
		opened::<T>(channel)?;

		#[extrinsic_call]
		_(RawOrigin::Root, sender, recipient);

		assert!(Channels::<T>::get(channel).is_none());
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
