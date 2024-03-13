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

//! Tests for the sorted list provider.

use super::*;
use frame_election_provider_support::SortedListProvider;

#[test]
fn re_nominate_does_not_change_counters_or_list() {
	ExtBuilder::default().nominate(true).build_and_execute(|| {
		// given
		let pre_insert_voter_count =
			(Nominators::<Test>::count() + Validators::<Test>::count()) as u32;
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);

		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31, 101]);

		// when account 101 renominates
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![41]));

		// then counts don't change
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);
		// and the list is the same
		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31, 101]);
	});
}

#[test]
fn re_validate_does_not_change_counters_or_list() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// given
		let pre_insert_voter_count =
			(Nominators::<Test>::count() + Validators::<Test>::count()) as u32;
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);

		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31]);

		// when account 11 re-validates
		assert_ok!(Staking::validate(RuntimeOrigin::signed(11), Default::default()));

		// then counts don't change
		assert_eq!(<Test as Config>::VoterList::count(), pre_insert_voter_count);
		// and the list is the same
		assert_eq!(<Test as Config>::VoterList::iter().collect::<Vec<_>>(), vec![11, 21, 31]);
	});
}
