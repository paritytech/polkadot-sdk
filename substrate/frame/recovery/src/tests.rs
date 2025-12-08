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

use crate::{frame_system::Origin, mock::*, Call as RecoveryCall, *};
use frame::{deps::sp_runtime::bounded_vec, prelude::fungible::InspectHold, testing_prelude::*};
use pallet_balances::Call as BalancesCall;

use Test as T;

fn friends(friends: impl IntoIterator<Item = u64>) -> FriendsOf<T> {
	friends.into_iter().map(|f| f.into()).collect::<Vec<_>>().try_into().unwrap()
}

fn fg(fs: impl IntoIterator<Item = u64>) -> FriendGroupOf<T> {
	FriendGroupOf::<T> {
		deposit: 10,
		friends: friends(fs),
		friends_needed: 2,
		inheritor: FERDIE,
		inheritance_delay: 10,
		inheritance_order: 0,
		abort_delay: 10,
	}
}

fn signed(account: u64) -> RuntimeOrigin {
	RuntimeOrigin::signed(account)
}

fn assert_fg_deposit(who: u64, deposit: u128) {
	assert_eq!(
		<T as crate::Config>::Currency::balance_on_hold(
			&crate::HoldReason::FriendGroups.into(),
			&who
		),
		deposit
	);
}

fn clear_events() {
	frame_system::Pallet::<T>::reset_events();
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
		assert_eq!(Recovery::inheritance(FERDIE), vec![ALICE]);

		// Ferdie can control the lost account
		// In order to withdraw everything, Ferdie has to first remove the friend group deposit
		dbg!("Second time");
		assert_ok!(Recovery::control_inherited_account(
			signed(FERDIE),
			ALICE,
			Box::new(RecoveryCall::set_friend_groups { friend_groups: vec![] }.into())
		));

		assert_ok!(Recovery::control_inherited_account(
			signed(FERDIE),
			ALICE,
			Box::new(BalancesCall::transfer_all { dest: FERDIE, keep_alive: false }.into())
		));

		assert_eq!(<Test as Config>::Currency::total_balance(&ALICE), 0);
		assert_eq!(<Test as Config>::Currency::total_balance(&FERDIE), 2000);
	});
}

/// Setting multiple friend groups works.
#[test]
fn set_friend_groups_multiple_works() {
	new_test_ext().execute_with(|| {
		// Alice configures two friend groups with Bob, Charlie and Dave
		let fg1 = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([BOB, CHARLIE, DAVE]),
			friends_needed: 2,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};
		let fg2 = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([CHARLIE, DAVE]),
			friends_needed: 2,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};
		let friend_groups = vec![fg1, fg2];

		// Alice configures two friend groups
		assert_ok!(Recovery::set_friend_groups(signed(ALICE), friend_groups));
		// Deposit taken for both friend groups
		assert_fg_deposit(ALICE, 144);
	});
}

/// Setting a friend group twice fails.
#[test]
fn set_friend_groups_duplicate_fails() {
	new_test_ext().execute_with(|| {
		let fg1 = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([BOB, CHARLIE, DAVE]),
			friends_needed: 2,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};
		let friend_groups = vec![fg1.clone(), fg1];

		assert_noop!(
			Recovery::set_friend_groups(signed(ALICE), friend_groups),
			Error::<T>::DuplicateFriendGroup
		);
	});
}

/// Setting a friend group with too many `friends_needed` fails.
#[test]
fn set_friend_groups_too_many_friends_needed_fails() {
	new_test_ext().execute_with(|| {
		let fg = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([BOB, CHARLIE, DAVE]),
			friends_needed: 4,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};
		let friend_groups = vec![fg];

		assert_noop!(
			Recovery::set_friend_groups(signed(ALICE), friend_groups),
			Error::<T>::TooManyFriendsNeeded
		);
	});
}

/// Setting a friend group with no friends fails.
#[test]
fn set_friend_groups_no_friends_fails() {
	new_test_ext().execute_with(|| {
		let fg = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([]),
			friends_needed: 0,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};

		assert_noop!(Recovery::set_friend_groups(signed(ALICE), vec![fg]), Error::<T>::NoFriends);
	});
}

/// Can remove all friend groups.
#[test]
fn set_friend_groups_remove_works() {
	new_test_ext().execute_with(|| {
		let fg = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([BOB, CHARLIE, DAVE]),
			friends_needed: 2,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};

		assert_ok!(Recovery::set_friend_groups(signed(ALICE), vec![fg.clone()]));
		assert_eq!(Recovery::friend_groups(ALICE), vec![fg]);
		assert_fg_deposit(ALICE, 79);

		assert_ok!(Recovery::set_friend_groups(signed(ALICE), vec![]));
		assert_eq!(Recovery::friend_groups(ALICE), vec![]);
		assert_fg_deposit(ALICE, 0);
	});
}

/// Cannot change friend groups if there are ongoing attempts.
#[test]
fn set_friend_groups_ongoing_attempt_fails() {
	new_test_ext().execute_with(|| {
		let fg = FriendGroupOf::<T> {
			deposit: 10,
			friends: friends([BOB, CHARLIE, DAVE]),
			friends_needed: 2,
			inheritor: FERDIE,
			inheritance_delay: 10,
			inheritance_order: 0,
			abort_delay: 10,
		};

		assert_ok!(Recovery::set_friend_groups(signed(ALICE), vec![fg.clone()]));
		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		assert_noop!(
			Recovery::set_friend_groups(signed(ALICE), vec![fg]),
			Error::<T>::HasOngoingAttempts
		);
	});
}

/// Setup the friend groups for Alice.
fn setup_alice_fgs(fs: impl IntoIterator<Item = impl IntoIterator<Item = u64>>) {
	let fgs = fs.into_iter().map(fg).collect::<Vec<_>>();
	assert_ok!(Recovery::set_friend_groups(signed(ALICE), fgs));
}

/// Cannot initiate more than one attempt per friend group.
#[test]
fn initiate_attempt_multiple_fails() {
	new_test_ext().execute_with(|| {
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

		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		assert_noop!(
			Recovery::initiate_attempt(signed(BOB), ALICE, 0),
			Error::<T>::AlreadyInitiated
		);
		assert_noop!(
			Recovery::initiate_attempt(signed(CHARLIE), ALICE, 0),
			Error::<T>::AlreadyInitiated
		);
	});
}

/// Cannot initiate without any friend groups.
#[test]
fn initiate_attempt_no_friend_groups_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(Recovery::initiate_attempt(signed(BOB), ALICE, 0), Error::<T>::NoFriendGroups);
	});
}

/// Can initiate attempts for different friend groups.
#[test]
fn initiate_attempt_different_friend_groups_works() {
	new_test_ext().execute_with(|| {
		setup_alice_fgs([[BOB, CHARLIE], [CHARLIE, DAVE]]);

		frame_system::Pallet::<T>::set_block_number(10);

		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		assert_ok!(Recovery::initiate_attempt(signed(CHARLIE), ALICE, 1));

		assert_eq!(
			Recovery::attempts(ALICE),
			vec![
				(
					fg([BOB, CHARLIE]),
					AttemptOf::<T> {
						friend_group_index: 0,
						init_block: 10,
						last_approval_block: 10,
						approvals: ApprovalBitfield::default(),
					}
				),
				(
					fg([CHARLIE, DAVE]),
					AttemptOf::<T> {
						friend_group_index: 1,
						init_block: 10,
						last_approval_block: 10,
						approvals: ApprovalBitfield::default(),
					}
				)
			]
		);
	});
}

/// Approving an attempt works as expected.
#[test]
fn approve_attempt_works() {
	new_test_ext().execute_with(|| {
		setup_alice_fgs([[BOB, CHARLIE, DAVE]]);

		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		assert_eq!(
			Recovery::attempts(ALICE),
			vec![(
				fg([BOB, CHARLIE, DAVE]),
				AttemptOf::<T> {
					friend_group_index: 0,
					init_block: 1,
					last_approval_block: 1,
					approvals: ApprovalBitfield::default(),
				}
			)]
		);

		// Bob votes at block 2
		frame_system::Pallet::<T>::set_block_number(2);
		assert_ok!(Recovery::approve_attempt(signed(BOB), ALICE, 0));
		assert_noop!(Recovery::approve_attempt(signed(BOB), ALICE, 0), Error::<T>::AlreadyVoted);
		assert_eq!(
			Recovery::attempts(ALICE),
			vec![(
				fg([BOB, CHARLIE, DAVE]),
				AttemptOf::<T> {
					friend_group_index: 0,
					init_block: 1,
					last_approval_block: 2,
					approvals: ApprovalBitfield::default().with_bits([0]), // Bob is index 0
				}
			)]
		);

		// Dave votes at block 3
		frame_system::Pallet::<T>::set_block_number(3);
		assert_ok!(Recovery::approve_attempt(signed(DAVE), ALICE, 0));
		assert_noop!(
			Recovery::approve_attempt(signed(DAVE), ALICE, 0),
			Error::<T>::AlreadyApproved
		);
		assert_eq!(
			Recovery::attempts(ALICE),
			vec![(
				fg([BOB, CHARLIE, DAVE]),
				AttemptOf::<T> {
					friend_group_index: 0,
					init_block: 1,
					last_approval_block: 3,
					approvals: ApprovalBitfield::default().with_bits([0, 2]), /* Bob is index 0,
					                                                           * Dave is 2 */
				}
			)]
		);

		// Non-friend cannot vote
		assert_noop!(Recovery::approve_attempt(signed(EVE), ALICE, 0), Error::<T>::NotFriend);

		// Charlie voting will fail since it is already approved
		frame_system::Pallet::<T>::set_block_number(4);
		assert_noop!(
			Recovery::approve_attempt(signed(CHARLIE), ALICE, 0),
			Error::<T>::AlreadyApproved
		);

		// Charlie cannot finish before the inheritance delay
		assert_noop!(
			Recovery::finish_attempt(signed(CHARLIE), ALICE, 0),
			Error::<T>::NotYetInheritable
		);

		// .. but can exactly at the unlock block
		hypothetically!({
			frame_system::Pallet::<T>::set_block_number(11);
			assert_ok!(Recovery::finish_attempt(signed(CHARLIE), ALICE, 0));
			assert_eq!(Recovery::inheritor(ALICE), Some(FERDIE));
			assert_eq!(Recovery::inheritance(FERDIE), vec![ALICE]);
		});
		// .. or later
		frame_system::Pallet::<T>::set_block_number(25);
		assert_ok!(Recovery::finish_attempt(signed(CHARLIE), ALICE, 0));
		assert_eq!(Recovery::inheritor(ALICE), Some(FERDIE));
		assert_eq!(Recovery::inheritance(FERDIE), vec![ALICE]);
	});
}

/// Can inherit multiple accounts.
#[test]
fn inherit_multiple_accounts_works() {
	new_test_ext().execute_with(|| {
		setup_alice_fgs([[BOB, CHARLIE, DAVE]]);

		// setup bob friend groups
		let fgs = vec![fg([ALICE, CHARLIE, DAVE])];
		assert_ok!(Recovery::set_friend_groups(signed(BOB), fgs));

		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		assert_ok!(Recovery::approve_attempt(signed(BOB), ALICE, 0));
		assert_ok!(Recovery::approve_attempt(signed(CHARLIE), ALICE, 0));
		frame_system::Pallet::<T>::set_block_number(11);
		assert_ok!(Recovery::finish_attempt(signed(BOB), ALICE, 0));

		assert_ok!(Recovery::initiate_attempt(signed(ALICE), BOB, 0));
		assert_ok!(Recovery::approve_attempt(signed(ALICE), BOB, 0));
		assert_ok!(Recovery::approve_attempt(signed(CHARLIE), BOB, 0));
		frame_system::Pallet::<T>::set_block_number(21);
		assert_ok!(Recovery::finish_attempt(signed(ALICE), BOB, 0));

		// Ferdie inherits both
		assert_eq!(Recovery::inheritor(BOB), Some(FERDIE));
		assert_eq!(Recovery::inheritor(ALICE), Some(FERDIE));
		assert_eq!(Recovery::inheritance(FERDIE), vec![ALICE, BOB]);
	});
}

/// Finish attempt works
#[test]
fn finish_attempt_works() {
	new_test_ext().execute_with(|| {
		assert_err!(Recovery::finish_attempt(signed(BOB), ALICE, 0), Error::<T>::NotAttempt);
		setup_alice_fgs([[BOB, CHARLIE, DAVE]]);
		assert_err!(Recovery::finish_attempt(signed(BOB), ALICE, 0), Error::<T>::NotAttempt);

		assert_ok!(Recovery::initiate_attempt(signed(BOB), ALICE, 0));
		assert_err!(Recovery::finish_attempt(signed(BOB), ALICE, 0), Error::<T>::NotApproved);

		assert_ok!(Recovery::approve_attempt(signed(BOB), ALICE, 0));
		assert_err!(Recovery::finish_attempt(signed(BOB), ALICE, 0), Error::<T>::NotApproved);

		assert_ok!(Recovery::approve_attempt(signed(CHARLIE), ALICE, 0));
		assert_err!(Recovery::finish_attempt(signed(BOB), ALICE, 0), Error::<T>::NotYetInheritable);

		frame_system::Pallet::<T>::set_block_number(11);
		assert_ok!(Recovery::finish_attempt(signed(BOB), ALICE, 0));
		assert_err!(Recovery::finish_attempt(signed(BOB), ALICE, 0), Error::<T>::NotAttempt);

		assert_eq!(Recovery::inheritor(ALICE), Some(FERDIE));
		assert_eq!(Recovery::inheritance(FERDIE), vec![ALICE]);
	});
}
