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

//! Tests for the module.

use crate::{frame_system::Origin, mock::*, *};
use frame::{deps::sp_runtime::bounded_vec, testing_prelude::*};

use Test as T;

fn friends(friends: impl IntoIterator<Item = u64>) -> FriendsOf<T> {
	friends.into_iter().map(|f| f.into()).collect::<Vec<_>>().try_into().unwrap()
}

fn signed(account: u64) -> RuntimeOrigin {
	RuntimeOrigin::signed(account)
}

#[test]
fn basic_flow_works() {
	new_test_ext().execute_with(|| {
		// Alice configures one friend group with Bob, Charlie and Dave
		let fg = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([BOB, CHARLIE, DAVE]),
			friends_needed: 2,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};

		assert_ok!(Recovery::set_friend_groups(signed(ALICE), vec![fg]));

		// Bob initiates the attempt
		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		// Bob and Charlie vote
		assert_ok!(Recovery::approve_attempt(signed(BOB), ALICE, 0));
		assert_ok!(Recovery::approve_attempt(signed(CHARLIE), ALICE, 0));

		// Eve finishes the attempt too early (10 inheritance delay)
		assert_noop!(
			Recovery::finish_attempt(signed(EVE), ALICE, 0),
			Error::<T>::NotYetInheritable
		);

		// Advance the block number to 11
		System::set_block_number(11);

		// Eve finishes the attempt
		assert_ok!(Recovery::finish_attempt(signed(EVE), ALICE, 0));

		// Eve finishes the attempt and Ferdie should be the inheritor
		assert_eq!(Recovery::inheritor(ALICE), Some(FERDIE));
	});
}
