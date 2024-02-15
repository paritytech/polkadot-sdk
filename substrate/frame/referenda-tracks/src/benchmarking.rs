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

//! Benchmarks for remarks pallet

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{Event, OriginToTrackId, Pallet as ReferendaTracks, Tracks, TracksIds};
use frame_benchmarking::v2::*;
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use pallet_referenda::{Curve, TrackInfo, TrackInfoOf};
use sp_runtime::{str_array as s, traits::AtLeast32Bit, Perbill};

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type BalanceOf<T, I> =
	<<T as pallet_referenda::Config<I>>::Currency as frame_support::traits::Currency<
		AccountIdOf<T>,
	>>::Balance;

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn track_info_of<T, I: 'static>() -> TrackInfoOf<T, I>
where
	T: pallet_referenda::Config<I>,
	BalanceOf<T, I>: AtLeast32Bit,
{
	TrackInfo {
		name: s("Test Track"),
		max_deciding: 1,
		decision_deposit: 0u32.into(),
		prepare_period: 10u32.into(),
		decision_period: 100u32.into(),
		confirm_period: 10u32.into(),
		min_enactment_period: 2u32.into(),
		min_approval: Curve::LinearDecreasing {
			length: Perbill::from_percent(100),
			floor: Perbill::from_percent(50),
			ceil: Perbill::from_percent(100),
		},
		min_support: Curve::LinearDecreasing {
			length: Perbill::from_percent(100),
			floor: Perbill::from_percent(0),
			ceil: Perbill::from_percent(50),
		},
	}
}

fn max_tracks<T: Config<I>, I: 'static>() -> u32 {
	T::MaxTracks::get()
}

fn max_track_id<T: Config<I>, I: 'static>() -> TrackIdOf<T, I> {
	T::BenchmarkHelper::track_id(max_tracks::<T, I>())
}

fn prepare_tracks<T: Config<I>, I: 'static>(full: bool) {
	let ids = (0..max_tracks::<T, I>() - 1)
		.map(|x| T::BenchmarkHelper::track_id(x))
		.collect::<Vec<TrackIdOf<T, I>>>();
	let track = track_info_of::<T, I>();
	let origin: PalletsOriginOf<T> = RawOrigin::Signed(whitelisted_caller()).into();

	TracksIds::<T, I>::mutate(|tracks_ids| {
		*tracks_ids = BoundedVec::truncate_from(ids.clone());
	});
	ids.iter().for_each(|id| {
		Tracks::<T, I>::insert(id.clone(), track.clone());
		OriginToTrackId::<T, I>::insert(origin.clone(), id.clone());
	});

	if full {
		ReferendaTracks::<T, I>::insert(
			RawOrigin::Root.into(),
			max_track_id::<T, I>(),
			track,
			origin,
		)
		.expect("inserts last track");
	}
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	pub fn insert() {
		// Setup code
		prepare_tracks::<T, I>(false);

		let id = max_track_id::<T, I>();
		let track = track_info_of::<T, I>();
		let origin: PalletsOriginOf<T> = RawOrigin::Signed(whitelisted_caller()).into();

		#[extrinsic_call]
		_(RawOrigin::Root, id, track, origin);

		// Verification code
		assert_last_event::<T, I>(Event::Created { id }.into());
	}

	#[benchmark]
	pub fn update() {
		// Setup code
		prepare_tracks::<T, I>(true);

		let id = max_track_id::<T, I>();
		let caller = whitelisted_caller();
		let track = track_info_of::<T, I>();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), id, track);

		// Verification code
		assert_last_event::<T, I>(Event::Updated { id }.into());
	}

	#[benchmark]
	pub fn remove() {
		// Setup code
		prepare_tracks::<T, I>(true);

		let id = max_track_id::<T, I>();
		let origin = RawOrigin::Signed(whitelisted_caller()).into();

		#[extrinsic_call]
		_(RawOrigin::Root, id, origin);

		// Verification code
		assert_last_event::<T, I>(Event::Removed { id }.into());
	}

	impl_benchmark_test_suite!(ReferendaTracks, crate::mock::new_test_ext(None), crate::mock::Test);
}
