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
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use polkadot_runtime_parachains::hrmp::deposits::DepositChange;

/// A request that changes both sides of a channel.
fn both_sides_request() -> DepositChangeRequest {
	let change = DepositChange { current: 0, target: 1_000 };
	DepositChangeRequest {
		channel_id: HrmpChannelId { sender: 1.into(), recipient: 2.into() },
		sender: Some(change),
		recipient: Some(change),
		action: DepositAction::InitAndAcceptOpenChannel {
			max_capacity: u32::MAX,
			max_message_size: u32::MAX,
			max_total_size: u32::MAX,
		},
	}
}

#[benchmarks(where <T as frame_system::Config>::RuntimeOrigin: From<ParachainOrigin>)]
mod benchmarks {
	use super::*;

	/// The answer to a request for both sides. The answer is a refusal, so that
	/// `OnDepositSet` does the least: its own weight is charged on top.
	#[benchmark]
	fn receive() {
		T::BenchmarkHelper::ensure_send_to_para();
		let response = Pallet::<T>::try_set_deposit(both_sides_request());
		assert_eq!(response, Ok(DepositChangeResponse::Pending));
		let id = NextRequestId::<T>::get().wrapping_sub(1);
		let origin = ParachainOrigin::Parachain(T::HolderParaId::get());
		let message = MessageToRelay::V1(MessageToRelayV1::DepositsSet {
			id,
			result: Err(WireDepositError::Other),
		});

		#[extrinsic_call]
		_(origin, message);

		assert!(PendingRequests::<T>::get(id).is_none());
	}

	/// A request for both sides. See [`try_set_deposit_release`] for a release.
	#[benchmark]
	fn try_set_deposit_both_sides() {
		T::BenchmarkHelper::ensure_send_to_para();
		let request = both_sides_request();
		let response;

		#[block]
		{
			response = Pallet::<T>::try_set_deposit(request);
		}

		assert_eq!(response, Ok(DepositChangeResponse::Pending));
	}

	/// A release for both sides that joins a [`FailedReleases`] with room for one more. A
	/// release that is sent instead reads an empty [`FailedReleases`] and writes nothing.
	#[benchmark]
	fn try_set_deposit_release() {
		let waiting = T::MaxFailedReleases::get().saturating_sub(1);
		let releases: Vec<_> = (0..waiting)
			.map(|i| {
				let channel = ChannelId { sender: i, recipient: i + 1 };
				ReleaseMessageToPara { channel, sender: true, recipient: true }
			})
			.collect();
		FailedReleases::<T>::put(
			BoundedVec::try_from(releases).expect("less than `MaxFailedReleases`; qed"),
		);
		let request =
			DepositChangeRequest { action: DepositAction::Release, ..both_sides_request() };
		let response;

		#[block]
		{
			response = Pallet::<T>::try_set_deposit(request);
		}

		assert_eq!(response, Ok(DepositChangeResponse::Applied));
		assert_eq!(FailedReleases::<T>::get().len() as u32, waiting + 1);
	}

	/// `n` failed releases waiting, all sent.
	#[benchmark]
	fn retry_failed_releases(n: Linear<0, { T::MaxFailedReleases::get() }>) {
		T::BenchmarkHelper::ensure_send_to_para();
		let releases: Vec<_> = (0..n)
			.map(|i| {
				let channel = ChannelId { sender: i, recipient: i + 1 };
				ReleaseMessageToPara { channel, sender: true, recipient: true }
			})
			.collect();
		FailedReleases::<T>::put(
			BoundedVec::try_from(releases).expect("at most `MaxFailedReleases`; qed"),
		);

		#[block]
		{
			Pallet::<T>::retry_failed_releases(&mut WeightMeter::new());
		}

		assert!(FailedReleases::<T>::get().is_empty());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::build_ext(), crate::mock::Test);
}
