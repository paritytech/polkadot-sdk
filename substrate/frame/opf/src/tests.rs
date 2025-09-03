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

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, traits::Currency};
use pallet_conviction_voting::{AccountVote, Conviction, Vote};
use sp_core::Get;
use sp_runtime::{traits::AccountIdConversion, DispatchError};

/// Registry-related unit-tests for the OPF pallet.
mod registry {
	use super::*;

	/// Root (AdminOrigin) can register.
	#[test]
	fn register_project_by_admin_works() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(pallet::Pallet::<Test>::register_project(
				frame_system::RawOrigin::Root.into(),
				project(1, 2),
			));

			// Storage reflects change.
			assert!(pallet::Projects::<Test>::contains_key(0));
			assert_eq!(pallet::NextProjectIndex::<Test>::get(), 1);
		});
	}

	/// Non‑admin attempt fails with `BadOrigin`.
	#[test]
	fn register_project_non_admin_fails() {
		ExtBuilder::build().execute_with(|| {
			assert_noop!(
				pallet::Pallet::<Test>::register_project(
					frame_system::RawOrigin::Signed(1).into(),
					project(1, 2),
				),
				DispatchError::BadOrigin
			);

			// Nothing was written.
			assert!(!pallet::Projects::<Test>::contains_key(0));
			assert_eq!(pallet::NextProjectIndex::<Test>::get(), 0);
		});
	}

	/// Sequential registrations get consecutive indices.
	#[test]
	fn sequential_registration_indices() {
		ExtBuilder::build().execute_with(|| {
			for i in 0..2 {
				assert_ok!(pallet::Pallet::<Test>::register_project(
					frame_system::RawOrigin::Root.into(),
					project(i + 1, i + 10),
				));
			}

			assert_eq!(pallet::NextProjectIndex::<Test>::get(), 2);
			assert!(pallet::Projects::<Test>::contains_key(0));
			assert!(pallet::Projects::<Test>::contains_key(1));
		});
	}

	/// Owner can update project info.
	#[test]
	fn owner_manages_project_info() {
		ExtBuilder::build().execute_with(|| {
			// Admin registers project 0 with owner = 1.
			assert_ok!(pallet::Pallet::<Test>::register_project(
				frame_system::RawOrigin::Root.into(),
				project(1, 2),
			));

			// Owner modifies it.
			let new_info = project(1, 3);
			assert_ok!(pallet::Pallet::<Test>::manage_project_info(
				frame_system::RawOrigin::Signed(1).into(),
				0,
				new_info.clone(),
			));

			assert_eq!(pallet::Projects::<Test>::get(0), Some(new_info));
		});
	}

	/// Non‑owner update attempt fails.
	#[test]
	fn non_owner_manage_project_info_fails() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(pallet::Pallet::<Test>::register_project(
				frame_system::RawOrigin::Root.into(),
				project(1, 2),
			));

			assert_noop!(
				pallet::Pallet::<Test>::manage_project_info(
					frame_system::RawOrigin::Signed(99).into(), // not the owner
					0,
					project(1, 3),
				),
				crate::pallet::Error::<Test>::AccountIsNotProjectOwner
			);

			// Storage not modified.
			assert_eq!(pallet::Projects::<Test>::get(0).unwrap().fund_dest, 2);
		});
	}

	/// Admin can unregister an existing project.
	#[test]
	fn admin_unregister_project() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(pallet::Pallet::<Test>::register_project(
				frame_system::RawOrigin::Root.into(),
				project(1, 2),
			));
			assert_ok!(pallet::Pallet::<Test>::unregister_project(
				frame_system::RawOrigin::Root.into(),
				0,
			));

			assert!(!pallet::Projects::<Test>::contains_key(0));
		});
	}

	/// Unregistering a non‑existent project errors cleanly.
	#[test]
	fn unregister_nonexistent_project_fails() {
		ExtBuilder::build().execute_with(|| {
			assert_noop!(
				pallet::Pallet::<Test>::unregister_project(
					frame_system::RawOrigin::Root.into(),
					42, // nothing here
				),
				crate::pallet::Error::<Test>::NoProjectAtIndex
			);
		});
	}

	/// MaxProjects is respected: add up to the limit, remove, and add again.
	#[test]
	fn max_projects_limit_is_respected() {
		ExtBuilder::build().execute_with(|| {
			let max = <Test as pallet::Config>::MaxProjects::get();
			// Add up to the limit
			for i in 0..max {
				assert_ok!(pallet::Pallet::<Test>::register_project(
					frame_system::RawOrigin::Root.into(),
					project(i + 1, i + 10),
				));
			}
			// Adding one more should fail
			assert_noop!(
				pallet::Pallet::<Test>::register_project(
					frame_system::RawOrigin::Root.into(),
					project(99, 99),
				),
				crate::pallet::Error::<Test>::MaxProjectsReached
			);
			// Remove a project
			assert_ok!(pallet::Pallet::<Test>::unregister_project(
				frame_system::RawOrigin::Root.into(),
				0,
			));
			// Now we can add one more
			assert_ok!(pallet::Pallet::<Test>::register_project(
				frame_system::RawOrigin::Root.into(),
				project(100, 100),
			));
			// But still can't exceed the limit
			assert_noop!(
				pallet::Pallet::<Test>::register_project(
					frame_system::RawOrigin::Root.into(),
					project(101, 101),
				),
				crate::pallet::Error::<Test>::MaxProjectsReached
			);
		});
	}
}

/// Round‑life‑cycle (Hooks::on_poll) tests for pallet‑opf.
mod rounds {
	use super::*;

	const OWNER: u64 = 1;
	const FUND_DEST: u64 = 2;
	const VOTER1: u64 = 10;
	const VOTER2: u64 = 11;

	/// Helper to give an account balance and create a vote.
	fn create_vote(voter: u64, project_id: u32, aye: bool, balance: u128, conviction: Conviction) {
		// Give the voter some balance
		let _ = Balances::deposit_creating(&voter, balance * 2);

		// Create the vote
		let vote = AccountVote::Standard { vote: Vote { aye, conviction }, balance };

		// Submit the vote
		assert_ok!(ConvictionVoting::vote(
			frame_system::RawOrigin::Signed(voter).into(),
			PollIndex::new(NextRoundIndex::<Test>::get() - 1, project_id),
			vote,
		));
	}

	// First block starts round 0
	#[test]
	fn first_call_starts_round_zero() {
		ExtBuilder::build().execute_with(|| {
			// Register a project so that Polls will be populated.
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));

			run_to_block(1);

			assert!(Round::<Test>::get().is_some(), "Round should exist");
			assert_eq!(NextRoundIndex::<Test>::get(), 1);
			assert!(Polls::<Test>::contains_key(0u32, 0u32));
		});
	}

	// Round expires after RoundDuration; new round created
	#[test]
	fn round_expires_and_new_round_starts() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));

			run_to_block(1); // start round 0

			let round0_start = 1;
			assert_eq!(Round::<Test>::get().unwrap().starting_block, round0_start);

			// Advance exactly RoundDuration (=5) blocks.
			run_to_block(round0_start + 5);

			// Round 0 should be completed, Round 1 started.
			let round_info = Round::<Test>::get().unwrap();
			assert_eq!(round_info.starting_block, 6);
			assert_eq!(NextRoundIndex::<Test>::get(), 2);

			// Status of project 0 in previous round must be Completed.
			match Polls::<Test>::get(0, 0).unwrap() {
				pallet::PollInfo::Completed(_, true) => {},
				_ => panic!("poll not completed"),
			}

			// And it must exist as ongoing in round 1.
			assert!(matches!(Polls::<Test>::get(1, 0).unwrap(), pallet::PollInfo::Ongoing(_, _)));
		});
	}

	// Simple reward
	#[test]
	fn simple_reward_calculation_matches_expectation() {
		ExtBuilder::build().execute_with(|| {
			// Pot gets 1 000 units.
			let pot = crate::mock::PotPalletId::get().into_account_truncating();
			let _ = Balances::deposit_creating(&pot, 1_000);

			// Register project 0 with FUND_DEST account.
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));

			run_to_block(1); // round 0 started

			// Create real votes instead of manually setting tally
			create_vote(VOTER1, 0, true, 300, Conviction::Locked1x);
			create_vote(VOTER2, 0, false, 10, Conviction::Locked1x);

			// Advance to block 6 → round ends.
			run_to_block(6);

			assert_eq!(Balances::free_balance(&pot), 0);
			assert_eq!(Balances::free_balance(&FUND_DEST), 1000);
		});
	}

	// Zero‑pot balance is handled gracefully
	#[test]
	fn zero_pot_balance_does_not_transfer() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));

			run_to_block(1); // round 0

			// Create some votes but pot balance is zero
			create_vote(VOTER1, 0, true, 100, Conviction::None);

			run_to_block(6); // end round

			assert_eq!(Balances::free_balance(&FUND_DEST), 0);
		});
	}

	// Project removed during a round
	#[test]
	fn unregister_project_mid_round_is_safe() {
		ExtBuilder::build().execute_with(|| {
			// Pot gets 1 000 units.
			let pot = crate::mock::PotPalletId::get().into_account_truncating();
			let _ = Balances::deposit_creating(&pot, 1_000);

			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));

			run_to_block(1); // round 0 active

			// Create some votes
			create_vote(VOTER1, 0, true, 100, Conviction::Locked1x);

			// Admin removes the project while poll is ongoing.
			assert_ok!(OPF::unregister_project(frame_system::RawOrigin::Root.into(), 0));

			// No panic when round ends.
			run_to_block(6);

			// Poll marked completed even though project was removed.
			assert!(matches!(Polls::<Test>::get(0, 0).unwrap(), pallet::PollInfo::Completed(_, _)));

			// No reward transferred (project was unregsitered).
			assert_eq!(Balances::free_balance(&FUND_DEST), 0);
		});
	}

	// Project added mid‑round appears only in next round
	#[test]
	fn project_added_mid_round_only_joins_next_round() {
		ExtBuilder::build().execute_with(|| {
			// Register project 0 and start round 0.
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));
			run_to_block(1);

			// Register a **second** project after round started.
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(42, 43)
			));

			// Should NOT be part of current round.
			assert!(!Polls::<Test>::contains_key(0, 1));

			// After round ends, it should be enrolled.
			run_to_block(6);
			assert!(Polls::<Test>::contains_key(1, 1));
		});
	}

	// Works when no projects are registered
	#[test]
	fn on_poll_handles_empty_project_set() {
		ExtBuilder::build().execute_with(|| {
			run_to_block(1); // start first round with zero projects

			assert!(Round::<Test>::get().is_some());
			assert!(Polls::<Test>::iter_prefix(0u32).next().is_none());
		});
	}
}

/// Vote‑forwarding engine tests for pallet‑opf.
mod forwarding {
	use super::*;

	const OWNER: u64 = 1;
	const FUND_DEST: u64 = 2;

	/// Helper to cast a vote as a user for a given round/project.
	fn user_vote(user: u64, round: u32, project: u32, bal: u128) {
		let _ = Balances::deposit_creating(&user, bal * 2);
		let poll_index = crate::PollIndex::new(round, project);
		let vote = AccountVote::Standard {
			vote: Vote { aye: true, conviction: Conviction::None },
			balance: bal,
		};
		assert_ok!(
			pallet_conviction_voting::Pallet::<Test, frame_support::instances::Instance1>::vote(
				frame_system::RawOrigin::Signed(user).into(),
				poll_index,
				vote,
			)
		);
	}

	// forwarder use multiple blocks and votes are forwarded
	#[test]
	fn forwarder_use_multiple_blocks() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));
			run_to_block(1); // round 0
			assert_eq!(NextRoundIndex::<Test>::get(), 1);

			// Simulate 10,001 users voting in round 0, project 0.
			for i in 0..10_001 {
				user_vote(10_000 + i as u64, 0, 0, 1);
			}
			assert_eq!(VotesToForward::<Test>::iter().count(), 10_001);

			// Advance to next round (block 6, round 1)
			run_to_block(6);
			assert_eq!(NextRoundIndex::<Test>::get(), 2);
			assert!(matches!(
				VotesForwardingState::<Test>::get().forwarding,
				crate::pallet::ForwardingProcess::LastProcessed(_, _),
			));

			// One extra block: forwarder should process the last entry.
			run_to_block(7);
			assert!(matches!(
				VotesForwardingState::<Test>::get().forwarding,
				crate::pallet::ForwardingProcess::Finished,
			));

			// Assert that the poll for round 1, project 0 has the correct support (10,001)
			let poll = Polls::<Test>::get(1, 0).expect("poll for round 1, project 0 should exist");
			let support = match poll {
				pallet::PollInfo::Ongoing(ref tally, _) => tally.support,
				_ => panic!("poll for round 1, project 0 should be ongoing"),
			};
			assert_eq!(support, 10_001);
		});
	}

	// on_before_vote inserts a record in `VotesToForward`
	#[test]
	fn on_before_vote_creates_forward_record() {
		ExtBuilder::build().execute_with(|| {
			let _ = Balances::deposit_creating(&3, 1_000);

			// Register project 0 and start round 0
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));
			run_to_block(1);

			// Cast a real vote via ConvictionVoting pallet.
			let poll_index = crate::PollIndex::new(0, 0);
			assert_ok!(
				pallet_conviction_voting::Pallet::<Test, frame_support::instances::Instance1>::vote(
					frame_system::RawOrigin::Signed(3).into(),
					poll_index,
					AccountVote::Standard {
						vote: Vote { aye: true, conviction: Conviction::None },
						balance: 100
					},
				)
			);

			// Forward record must exist and round must match current round.
			let rec = VotesToForward::<Test>::get(0, 3).expect("record missing");
			assert_eq!(rec.round, NextRoundIndex::<Test>::get() - 1);
		});
	}

	// `remove_automatic_forwarding` extrinsic deletes mapping
	#[test]
	fn remove_auto_forwarding_deletes_mapping() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));
			run_to_block(1);

			// User votes in round 0, project 0
			user_vote(7, 0, 0, 5);
			assert!(VotesToForward::<Test>::contains_key(0, 7));

			// Call extrinsic.
			assert_ok!(OPF::remove_automatic_forwarding(
				frame_system::RawOrigin::Signed(7).into(),
				0,
			));
			assert!(!VotesToForward::<Test>::contains_key(0, 7));
		});
	}

	// removing non‑existent entry returns Ok and does nothing
	#[test]
	fn remove_nonexistent_entry_is_noop() {
		ExtBuilder::build().execute_with(|| {
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));
			run_to_block(1);

			// Ensure mapping does not exist, then call extrinsic.
			assert_ok!(OPF::remove_automatic_forwarding(
				frame_system::RawOrigin::Signed(99).into(),
				0,
			));
		});
	}

	// project removed before forwarding => mapping purged
	#[test]
	fn votes_for_removed_project_are_purged() {
		ExtBuilder::build().execute_with(|| {
			// Create project 0, start round 0.
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER, FUND_DEST)
			));
			run_to_block(1);

			// User votes in round 0, project 0
			user_vote(13, 0, 0, 1);
			assert!(VotesToForward::<Test>::contains_key(0, 13));

			// Unregister the project during the round.
			assert_ok!(OPF::unregister_project(frame_system::RawOrigin::Root.into(), 0));

			// Let the round finish (block 6) – new round starts at 6.
			run_to_block(6);

			// Mapping must have been deleted because poll for (round 1, project 0) does not exist.
			assert!(!VotesToForward::<Test>::contains_key(0, 13));
		});
	}
}

/// Complex reward‑distribution integration test.
///
/// * Four projects.
/// * Mixed ayes/nays so that:
///   * idx 0: positive‑net 400
///   * idx 1: positive‑net 150
///   * idx 2: negative (net 0)
///   * idx 3: positive‑net 300 BUT project is removed mid‑round.
/// * Pot starts with 1 000; expected payouts
///   * idx 0 → 470
///   * idx 1 → 170
///   * idx 2 →   0
///   * idx 3 →   0 (removed)
///   * pot remainder → 360
mod full_scenario {
	use super::*;

	const OWNER0: u64 = 100;
	const DEST0: u64 = 10;
	const OWNER1: u64 = 101;
	const DEST1: u64 = 20;
	const OWNER2: u64 = 102;
	const DEST2: u64 = 30;
	const OWNER3: u64 = 103;
	const DEST3: u64 = 40;

	// voters
	const V1: u64 = 1;
	const V2: u64 = 2;
	const V3: u64 = 3;
	const V4: u64 = 4;
	const V5: u64 = 5;
	const V6: u64 = 6;
	const V7: u64 = 7;

	/// helper: cast a conviction‑vote
	fn cast_vote(voter: u64, proj_idx: u32, aye: bool, bal: u128) {
		let _ = Balances::deposit_creating(&voter, bal);
		let poll = PollIndex::new(0, proj_idx);
		let vote = AccountVote::Standard {
			vote: Vote { aye, conviction: Conviction::Locked1x },
			balance: bal,
		};
		assert_ok!(
			pallet_conviction_voting::Pallet::<Test, frame_support::instances::Instance1>::vote(
				frame_system::RawOrigin::Signed(voter).into(),
				poll,
				vote,
			)
		);
	}

	#[test]
	fn complex_reward_distribution() {
		ExtBuilder::build().execute_with(|| {
			// Setup: Pot & projects
			let pot = crate::mock::PotPalletId::get().into_account_truncating();
			let _ = Balances::deposit_creating(&pot, 1_000);

			// register 4 projects (indices 0-3)
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER0, DEST0)
			));
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER1, DEST1)
			));
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER2, DEST2)
			));
			assert_ok!(OPF::register_project(
				frame_system::RawOrigin::Root.into(),
				project(OWNER3, DEST3)
			));

			// Start round 0
			run_to_block(1);

			// Votes

			// project 0 → +400 (500‑100)
			cast_vote(V1, 0, true, 500);
			cast_vote(V2, 0, false, 100);

			// project 1 → +150 (200‑50)
			cast_vote(V3, 1, true, 200);
			cast_vote(V4, 1, false, 50);

			// project 2 → negative, net saturates to 0 (50‑100)
			cast_vote(V5, 2, true, 50);
			cast_vote(V6, 2, false, 100);

			// project 3 → +300 (only ayes, will be removed)
			cast_vote(V7, 3, true, 300);

			// Remove project 3 mid‑round
			assert_ok!(OPF::unregister_project(frame_system::RawOrigin::Root.into(), 3));

			// End round (block 6)
			run_to_block(6);

			let total_positive_net: u128 = 400 + 150 + 0 + 300;

			// Assertions: balances
			assert_eq!(470, 400 * 1000 / total_positive_net);
			assert_eq!(Balances::free_balance(&DEST0), 470);
			assert_eq!(176, 150 * 1000 / total_positive_net);
			assert_eq!(Balances::free_balance(&DEST1), 176);
			assert_eq!(Balances::free_balance(&DEST2), 0); // negative → 0
			assert_eq!(Balances::free_balance(&DEST3), 0); // removed

			// pot left with the remainder
			assert_eq!(Balances::free_balance(&TREASURY_POT), 354);

			// pot left with 0
			assert_eq!(Balances::free_balance(&pot), 0);
		});
	}
}
