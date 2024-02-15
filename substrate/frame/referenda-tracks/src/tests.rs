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

//! Tests for referenda tracks pallet.

use super::{Error, Event, Pallet as ReferendaTracks, Tracks};
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
use frame_system::{EventRecord, Phase, RawOrigin};
use pallet_referenda::TrackInfo;
use sp_runtime::{str_array as s, traits::BadOrigin, Perbill};

const TRACK: pallet_referenda::TrackInfoOf<Test, ()> = TrackInfo {
	name: s("Test Track"),
	max_deciding: 1,
	decision_deposit: 0,
	prepare_period: 10,
	decision_period: 100,
	confirm_period: 10,
	min_enactment_period: 2,
	min_approval: pallet_referenda::Curve::LinearDecreasing {
		length: Perbill::from_percent(100),
		floor: Perbill::from_percent(50),
		ceil: Perbill::from_percent(100),
	},
	min_support: pallet_referenda::Curve::LinearDecreasing {
		length: Perbill::from_percent(100),
		floor: Perbill::from_percent(0),
		ceil: Perbill::from_percent(50),
	},
};

const ORIGIN_SIGNED_1: OriginCaller = OriginCaller::system(RawOrigin::Signed(1));
const ORIGIN_SIGNED_2: OriginCaller = OriginCaller::system(RawOrigin::Signed(2));

mod insert {
	use super::*;

	#[test]
	fn fails_if_incorrect_origin() {
		new_test_ext(None).execute_with(|| {
			assert_noop!(
				ReferendaTracks::<Test, ()>::insert(
					RuntimeOrigin::signed(1),
					1,
					TRACK,
					ORIGIN_SIGNED_1
				),
				BadOrigin
			);
		});
	}

	#[test]
	fn it_works() {
		new_test_ext(None).execute_with(|| {
			System::set_block_number(1);

			assert_ok!(ReferendaTracks::<Test, ()>::insert(
				RuntimeOrigin::root(),
				1,
				TRACK,
				ORIGIN_SIGNED_1
			));

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Tracks(Event::Created { id: 1 }),
					topics: vec![],
				}],
			);

			assert_eq!(Tracks::<Test, ()>::get(1), Some(TRACK));
		});
	}

	#[test]
	fn it_fails_if_inserting_an_already_existing_track() {
		new_test_ext(None).execute_with(|| {
			assert_ok!(ReferendaTracks::<Test, ()>::insert(
				RuntimeOrigin::root(),
				1,
				TRACK,
				ORIGIN_SIGNED_1
			));

			assert_noop!(
				ReferendaTracks::<Test, ()>::insert(
					RuntimeOrigin::root(),
					1,
					TRACK,
					ORIGIN_SIGNED_2
				),
				Error::<Test, ()>::TrackIdAlreadyExisting
			);
		});
	}

	#[test]
	fn fails_if_exceeds_max_tracks() {
		new_test_ext(None).execute_with(|| {
			for i in 0..MaxTracks::get() {
				let origin_signed = OriginCaller::system(RawOrigin::Signed(i as u64));
				assert_ok!(ReferendaTracks::<Test, ()>::insert(
					RuntimeOrigin::root(),
					i,
					TRACK,
					origin_signed,
				));
			}

			let origin_signed_n = OriginCaller::system(RawOrigin::Signed(MaxTracks::get() as u64));
			assert_noop!(
				ReferendaTracks::<Test, ()>::insert(
					RuntimeOrigin::root(),
					MaxTracks::get(),
					TRACK,
					origin_signed_n
				),
				Error::<Test, ()>::MaxTracksExceeded
			);
		});
	}
}

mod update {
	use super::*;

	#[test]
	fn fails_if_incorrect_origin() {
		new_test_ext(None).execute_with(|| {
			assert_noop!(
				ReferendaTracks::<Test, ()>::update(RuntimeOrigin::signed(1), 1, TRACK),
				BadOrigin
			);
		});
	}

	#[test]
	fn it_works() {
		new_test_ext(Some(vec![(1, TRACK, ORIGIN_SIGNED_1)])).execute_with(|| {
			let mut track = TRACK.clone();
			track.max_deciding = 2;

			assert_ok!(ReferendaTracks::<Test, ()>::update(
				RuntimeOrigin::signed(1),
				1,
				track.clone()
			));

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Tracks(Event::Updated { id: 1 }),
					topics: vec![],
				}],
			);

			assert_eq!(Tracks::<Test, ()>::get(1), Some(track));
		});
	}
}

mod remove {
	use super::*;

	#[test]
	fn fails_if_incorrect_origin() {
		new_test_ext(None).execute_with(|| {
			assert_noop!(
				ReferendaTracks::<Test, ()>::remove(RuntimeOrigin::signed(1), 1, ORIGIN_SIGNED_1),
				BadOrigin
			);
		});
	}

	#[test]
	fn fails_if_non_existing() {
		new_test_ext(None).execute_with(|| {
			assert_noop!(
				ReferendaTracks::<Test, ()>::remove(RuntimeOrigin::root(), 1, ORIGIN_SIGNED_1),
				Error::<Test, ()>::TrackIdNotFound,
			);
		});
	}

	#[test]
	fn it_works() {
		new_test_ext(Some(vec![(1, TRACK, ORIGIN_SIGNED_1)])).execute_with(|| {
			assert_ok!(ReferendaTracks::<Test, ()>::remove(
				RuntimeOrigin::root(),
				1,
				ORIGIN_SIGNED_1
			));

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: RuntimeEvent::Tracks(Event::Removed { id: 1 }),
					topics: vec![],
				}],
			);

			assert_eq!(Tracks::<Test, ()>::get(1), None);
		});
	}
}
