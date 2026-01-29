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

//! Tests for pallet-price-oracle

use crate::oracle::{self, mock::*, StorageManager, Vote};
use frame_support::{pallet_prelude::*, testing_prelude::*};
use sp_runtime::{DispatchError, FixedU128, Percent};
use substrate_test_utils::assert_eq_uvec;

mod setup {
	use super::*;
	#[test]
	fn basic() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(System::block_number(), 7);
			assert_eq!(StorageManager::<T>::tracked_assets(), vec![1]);
			assert_eq!(
				oracle::Authorities::<T>::get(),
				vec![
					(1, Percent::from_percent(100)),
					(2, Percent::from_percent(100)),
					(3, Percent::from_percent(100)),
					(4, Percent::from_percent(100)),
				]
			);
		})
	}

	#[test]
	fn too_many_authorities() {
		todo!()
	}

	#[test]
	fn too_many_endpoints() {
		todo!()
	}

	#[test]
	fn too_long_endpoint() {
		todo!()
	}

	#[test]
	fn track_asset_with_no_endpoint() {
		ExtBuilder::default().extra_asset(2, vec![]).build_and_execute(|| {
			assert!(matches!(
				&StorageManager::<T>::tracked_assets_with_feeds()[..],
				[(1, x), (2, y)] if !x.is_empty() && y.is_empty()
			))
		});
	}
}

mod vote {
	use super::*;

	#[test]
	fn authorities_can_vote() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			assert!(StorageManager::<T>::block_votes(asset_id, now).is_empty());

			// when first vote comes in
			assert_ok!(PriceOracle::vote(
				RuntimeOrigin::signed(1),
				1,
				FixedU128::from_u32(42),
				now - 1
			));

			// then
			assert_eq_uvec!(
				StorageManager::<T>::block_votes(1, now),
				vec![(1, Vote { price: FixedU128::from_u32(42), produced_in: now - 1 })]
			);

			// when different vote, with different price and block origin comes in
			assert_ok!(PriceOracle::vote(
				RuntimeOrigin::signed(2),
				1,
				FixedU128::from_u32(43),
				now - 2
			));

			// then
			assert_eq_uvec!(
				StorageManager::<T>::block_votes(1, now),
				vec![
					(1, Vote { price: FixedU128::from_u32(42), produced_in: now - 1 }),
					(2, Vote { price: FixedU128::from_u32(43), produced_in: now - 2 })
				]
			);
		});
	}

	#[test]
	fn respects_max_votes_per_block() {
		ExtBuilder::default()
			.extra_asset(2, Default::default())
			.max_votes_per_block(1)
			.build_and_execute(|| {
				// given
				let now = System::block_number();
				let first_asset_id = 1;

				// when
				assert_ok!(PriceOracle::vote(
					RuntimeOrigin::signed(1),
					first_asset_id,
					FixedU128::from_u32(42),
					now - 1
				));

				// then
				assert_eq_uvec!(
					StorageManager::<T>::block_votes(1, now),
					vec![(1, Vote { price: FixedU128::from_u32(42), produced_in: now - 1 })]
				);

				// when 2 tries to vote for `first_asset_id` again
				assert_noop!(
					PriceOracle::vote(
						RuntimeOrigin::signed(2),
						first_asset_id,
						FixedU128::from_u32(43),
						now - 1
					),
					oracle::Error::<T>::TooManyVotes
				);

				// when 2 tries to vote for `second_asset_id`
				let second_asset_id = 2;
				assert_ok!(PriceOracle::vote(
					RuntimeOrigin::signed(2),
					second_asset_id,
					FixedU128::from_u32(43),
					now - 1
				));

				// then
				assert_eq_uvec!(
					StorageManager::<T>::block_votes(2, now),
					vec![(2, Vote { price: FixedU128::from_u32(43), produced_in: now - 1 })]
				);

				// when 1 tries to update their vote (re-tests `duplicate_vote_replaces_old_vote`)
				assert_ok!(PriceOracle::vote(
					RuntimeOrigin::signed(1),
					first_asset_id,
					FixedU128::from_u32(77),
					now - 1
				));

				// then
				assert_eq_uvec!(
					StorageManager::<T>::block_votes(1, now),
					vec![(1, Vote { price: FixedU128::from_u32(77), produced_in: now - 1 })]
				);
			})
	}

	#[test]
	fn rejects_non_authority_vote() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;

			// when
			assert_noop!(
				PriceOracle::vote(
					RuntimeOrigin::signed(5),
					asset_id,
					FixedU128::from_u32(42),
					now - 1
				),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn rejects_non_tracked_asset_vote() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			assert_eq!(StorageManager::<T>::tracked_assets(), vec![1]);
			let bad_asset_id = 2;

			// when
			assert_noop!(
				PriceOracle::vote(
					RuntimeOrigin::signed(1),
					bad_asset_id,
					FixedU128::from_u32(42),
					now - 1
				),
				oracle::Error::<T>::AssetNotTracked
			);
		});
	}

	#[test]
	fn duplicate_vote_replaces_old_vote() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			assert!(StorageManager::<T>::block_votes(asset_id, now).is_empty());

			// when
			assert_ok!(PriceOracle::vote(
				RuntimeOrigin::signed(1),
				asset_id,
				FixedU128::from_u32(42),
				now - 1
			));

			// then
			assert_eq_uvec!(
				StorageManager::<T>::block_votes(asset_id, now),
				vec![(1, Vote { price: FixedU128::from_u32(42), produced_in: now - 1 })]
			);

			// when 1 submits vote in the same block
			assert_ok!(PriceOracle::vote(
				RuntimeOrigin::signed(1),
				asset_id,
				FixedU128::from_u32(43),
				now - 1
			));

			// then it is replacing the old one.
			assert_eq_uvec!(
				StorageManager::<T>::block_votes(asset_id, now),
				vec![(1, Vote { price: FixedU128::from_u32(43), produced_in: now - 1 })]
			);
		})
	}

	#[test]
	fn rejects_old_vote() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			let max_age = MaxVoteAge::get();

			// when vote is at the boundary of age
			assert_ok!(PriceOracle::vote(
				RuntimeOrigin::signed(1),
				asset_id,
				FixedU128::from_u32(42),
				now - max_age
			));

			// then
			assert_eq_uvec!(
				StorageManager::<T>::block_votes(asset_id, now),
				vec![(1, Vote { price: FixedU128::from_u32(42), produced_in: now - max_age })]
			);

			// when vote is too old
			assert_noop!(
				PriceOracle::vote(
					RuntimeOrigin::signed(2),
					asset_id,
					FixedU128::from_u32(43),
					now - max_age - 1
				),
				oracle::Error::<T>::OldVote
			);
		});
	}
}

mod tally_on_finalize {
	use frame_support::pallet_prelude::StorageMap;
	use sp_runtime::Perbill;

	use crate::oracle::{PriceData, TallyOuterError, TimePoint};

	use super::*;

	fn all_vote(asset_id: AssetId, now: BlockNumber, vote: u32) {
		for (who, _) in oracle::Authorities::<T>::get() {
			assert_ok!(PriceOracle::vote(
				RuntimeOrigin::signed(who),
				asset_id,
				FixedU128::from_u32(vote),
				now
			));
		}
	}

	#[test]
	fn successful_tally_reports_price() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			assert!(StorageManager::<T>::block_votes(asset_id, now).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());

			// when
			all_vote(asset_id, now, 42);
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 4);
			PriceOracle::on_finalize(now);

			let expected_price = PriceData {
				price: FixedU128::from_u32(42),
				confidence: Percent::from_percent(100),
				updated_in: TimePoint { local: 7, relay: 7, timestamp: 7000 },
			};

			// then price is recorded onchain
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap(), expected_price);
			// and is reported externally
			assert_eq!(PriceUpdates::get(), vec![(asset_id, expected_price)]);
			// voting record is still in place
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 4);
		})
	}

	#[test]
	fn unsuccessful_tally_moves_votes_to_next_block() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			assert!(StorageManager::<T>::block_votes(asset_id, now).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());

			// when
			all_vote(asset_id, now, 42);
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 4);
			NextTallyFails::set(Some(TallyOuterError::KeepVotes(())));
			PriceOracle::on_finalize(now);

			// then price is not updated.
			assert!(StorageManager::<T>::current_price(asset_id).is_none());
			// and votes are moved to the next block.
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now + 1).len(), 4);
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 0);
		})
	}

	#[test]
	fn unsuccessful_tally_moves_only_valid_votes_to_next_block() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			assert!(StorageManager::<T>::block_votes(asset_id, now).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());
			assert_eq!(MaxVoteAge::get(), 4);

			// when all vote, with produced_in decreasing by 1, all votes are valid, but only 3 will
			// be valid if we move these votes to the next block.
			for (idx, who) in
				oracle::Authorities::<T>::get().into_iter().map(|(who, _)| who).enumerate()
			{
				assert_ok!(PriceOracle::vote(
					RuntimeOrigin::signed(who),
					asset_id,
					FixedU128::from_u32(42),
					now - (idx as u64) - 1
				));
			}
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 4);
			NextTallyFails::set(Some(TallyOuterError::KeepVotes(())));
			PriceOracle::on_finalize(now);

			// then price is not updated.
			assert!(StorageManager::<T>::current_price(asset_id).is_none());
			// and votes are moved to the next block, but 1 is still discarded
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now + 1).len(), 3);
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 0);
		})
	}

	#[test]
	fn unsuccessful_tally_yanks_votes() {
		ExtBuilder::default().build_and_execute(|| {
			// given
			let now = System::block_number();
			let asset_id = 1;
			assert!(StorageManager::<T>::block_votes(asset_id, now).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());

			// when
			all_vote(asset_id, now, 42);
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 4);
			NextTallyFails::set(Some(TallyOuterError::YankVotes(())));
			PriceOracle::on_finalize(now);

			// then price is not updated.
			assert!(StorageManager::<T>::current_price(asset_id).is_none());
			// and no votes remain
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now + 1).len(), 0);
			assert_eq!(StorageManager::<T>::block_votes(asset_id, now).len(), 0);
		})
	}

	#[test]
	fn tracks_history_up_to_history_depth() {
		ExtBuilder::default().history_depth(2).build_and_execute(|| {
			// given
			let asset_id = 1;
			assert_eq!(System::block_number(), 7);
			assert!(StorageManager::<T>::block_votes(asset_id, System::block_number()).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());

			// when price updated in block 7
			all_vote(asset_id, System::block_number(), 42);
			PriceOracle::on_finalize(System::block_number());

			// then current price is set
			assert_eq!(
				StorageManager::<T>::current_price(asset_id).unwrap().price,
				FixedU128::from_u32(42)
			);
			// then no history is set yet
			assert_eq!(StorageManager::<T>::price_history(asset_id).len(), 0);
			// then voting data from block 7 is kept
			assert_eq_uvec!(StorageManager::<T>::block_with_votes(asset_id), vec![(7, 4)]);

			// when price updated in block 8
			bump_block_number(8);
			all_vote(asset_id, System::block_number(), 43);
			PriceOracle::on_finalize(System::block_number());

			// then current price is set
			assert_eq!(
				StorageManager::<T>::current_price(asset_id).unwrap(),
				PriceData {
					price: FixedU128::from_u32(43),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 8, relay: 8, timestamp: 8000 }
				}
			);
			// then old price goes to history
			assert_eq!(
				StorageManager::<T>::price_history(asset_id),
				vec![PriceData {
					price: FixedU128::from_u32(42),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 7, relay: 7, timestamp: 7000 }
				}]
			);
			// then voting data from block 8 is added.
			assert_eq_uvec!(StorageManager::<T>::block_with_votes(asset_id), vec![(7, 4), (8, 4)]);

			// when price is at block 9
			bump_block_number(9);
			all_vote(asset_id, System::block_number(), 44);
			PriceOracle::on_finalize(System::block_number());

			// then the current price is updated
			assert_eq!(
				StorageManager::<T>::current_price(asset_id).unwrap(),
				PriceData {
					price: FixedU128::from_u32(44),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 9, relay: 9, timestamp: 9000 }
				}
			);
			// then the old price goes to history
			assert_eq!(
				StorageManager::<T>::price_history(asset_id),
				vec![
					PriceData {
						price: FixedU128::from_u32(42),
						confidence: Percent::from_percent(100),
						updated_in: TimePoint { local: 7, relay: 7, timestamp: 7000 }
					},
					PriceData {
						price: FixedU128::from_u32(43),
						confidence: Percent::from_percent(100),
						updated_in: TimePoint { local: 8, relay: 8, timestamp: 8000 }
					}
				]
			);
			// then the voting data from block 9 is added.
			assert_eq_uvec!(
				StorageManager::<T>::block_with_votes(asset_id),
				vec![(7, 4), (8, 4), (9, 4)]
			);

			// when price is updated at block 10
			bump_block_number(10);
			all_vote(asset_id, System::block_number(), 45);
			PriceOracle::on_finalize(System::block_number());

			// then the current price is updated
			assert_eq!(
				StorageManager::<T>::current_price(asset_id).unwrap(),
				PriceData {
					price: FixedU128::from_u32(45),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 10, relay: 10, timestamp: 10000 }
				}
			);

			// then the old price goes to history, but the oldest record is yanked.
			assert_eq!(
				StorageManager::<T>::price_history(asset_id),
				vec![
					PriceData {
						price: FixedU128::from_u32(43),
						confidence: Percent::from_percent(100),
						updated_in: TimePoint { local: 8, relay: 8, timestamp: 8000 }
					},
					PriceData {
						price: FixedU128::from_u32(44),
						confidence: Percent::from_percent(100),
						updated_in: TimePoint { local: 9, relay: 9, timestamp: 9000 }
					}
				]
			);

			// the the voting data from block 10 is added, but 7 is yanked as too old now.
			assert_eq_uvec!(
				StorageManager::<T>::block_with_votes(asset_id),
				vec![(8, 4), (9, 4), (10, 4)]
			);
		})
	}

	#[test]
	fn tracks_nothing_if_no_history_depth() {
		ExtBuilder::default().history_depth(0).build_and_execute(|| {
			// given: history_depth is zero and at block 7,
			let asset_id = 1;
			assert_eq!(System::block_number(), 7);

			// when price updated at block 7
			all_vote(asset_id, System::block_number(), 42);
			PriceOracle::on_finalize(System::block_number());

			// then: the current price is updated,
			assert_eq!(
				StorageManager::<T>::current_price(asset_id).unwrap(),
				PriceData {
					price: FixedU128::from_u32(42),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 7, relay: 7, timestamp: 7000 }
				}
			);

			// then: no price history is ever kept,
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());

			// then: no voting history is ever kept,
			assert!(StorageManager::<T>::block_with_votes(asset_id).is_empty());

			// when: price updated at block 8
			bump_block_number(8);
			all_vote(asset_id, System::block_number(), 43);
			PriceOracle::on_finalize(System::block_number());

			// then: the current price is updated,
			assert_eq!(
				StorageManager::<T>::current_price(asset_id).unwrap(),
				PriceData {
					price: FixedU128::from_u32(43),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 8, relay: 8, timestamp: 8000 }
				}
			);

			// then: still, no price history is kept,
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());

			// then: and no voting history is kept,
			assert!(StorageManager::<T>::block_with_votes(asset_id).is_empty());
		});
	}

	#[test]
	fn history_tracking_with_failed_tallies() {
		ExtBuilder::default().history_depth(2).build_and_execute(|| {
			// given
			let asset_id = 1;
			assert_eq!(System::block_number(), 7);
			assert!(StorageManager::<T>::block_votes(asset_id, System::block_number()).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());

			// when 1 round of successful tally
			all_vote(asset_id, System::block_number(), 42);
			PriceOracle::on_finalize(System::block_number());

			// then current price is set
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 7);
			// then no price history is set yet
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());
			// then voting data from block 7 is kept
			assert_eq_uvec!(StorageManager::<T>::block_with_votes(asset_id), vec![(7, 4)]);

			// when tally at block 8 fails with `YankVotes`
			bump_block_number(8);
			all_vote(asset_id, System::block_number(), 43);
			NextTallyFails::set(Some(TallyOuterError::YankVotes(())));
			PriceOracle::on_finalize(System::block_number());

			// then current price is not updated
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 7);
			// then price history is still the same as before
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());
			// then the voting record is the same as above
			assert_eq_uvec!(StorageManager::<T>::block_with_votes(asset_id), vec![(7, 4)]);

			// when tally at block 9 succeeds
			bump_block_number(9);
			all_vote(asset_id, System::block_number(), 44);
			PriceOracle::on_finalize(System::block_number());

			// then current price is updated
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 9);
			// Price history now has one old record
			assert_eq!(
				StorageManager::<T>::price_history(asset_id),
				vec![PriceData {
					price: FixedU128::from_u32(42),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 7, relay: 7, timestamp: 7000 }
				}]
			);
			// then the voting record is consistent.
			assert_eq_uvec!(StorageManager::<T>::block_with_votes(asset_id), vec![(7, 4), (9, 4)]);

			// when tally at block 10 fails with `KeepVotes`
			bump_block_number(10);
			all_vote(asset_id, System::block_number(), 45);
			NextTallyFails::set(Some(TallyOuterError::KeepVotes(())));
			PriceOracle::on_finalize(System::block_number());

			// then current price is not updated
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 9);
			// then price history is still the same as before
			assert_eq!(
				StorageManager::<T>::price_history(asset_id),
				vec![PriceData {
					price: FixedU128::from_u32(42),
					confidence: Percent::from_percent(100),
					updated_in: TimePoint { local: 7, relay: 7, timestamp: 7000 }
				}]
			);
			// then the voting record has one extra record for the future block (11).
			assert_eq_uvec!(
				StorageManager::<T>::block_with_votes(asset_id),
				vec![(7, 4), (9, 4), (11, 4)]
			);
		});
	}

	#[test]
	fn no_history_with_failed_tallies() {
		ExtBuilder::default().history_depth(0).build_and_execute(|| {
			// given
			let asset_id = 1;
			assert_eq!(System::block_number(), 7);
			assert!(StorageManager::<T>::block_votes(asset_id, System::block_number()).is_empty());
			assert!(StorageManager::<T>::current_price(asset_id).is_none());

			// when a successful tally happens at block 7
			all_vote(asset_id, System::block_number(), 42);
			PriceOracle::on_finalize(System::block_number());

			// then the current price is updated
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 7);
			// then no price history is set yet
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());
			// then no voting history is set yet
			assert!(StorageManager::<T>::block_with_votes(asset_id).is_empty());

			// when a failed tally happens at block 8 with `KeepVotes`
			bump_block_number(8);
			all_vote(asset_id, System::block_number(), 43);
			NextTallyFails::set(Some(TallyOuterError::KeepVotes(())));
			PriceOracle::on_finalize(System::block_number());

			// then the price is the same as before.
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 7);
			// then no price history is set
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());
			// then the voting record is moved forward to block 9 (this is not counting towards
			// history --they are speculative future votes).
			assert_eq_uvec!(StorageManager::<T>::block_with_votes(asset_id), vec![(9, 4)]);
			// then importantly our `try_state` calls, automatically called after
			// `PriceOracle::on_finalize` have passed.

			// when a successful tally happens at block 9 with our old voting record that is kept
			bump_block_number(9);
			all_vote(asset_id, System::block_number(), 44);
			PriceOracle::on_finalize(System::block_number());

			// then the price is updated
			assert_eq!(StorageManager::<T>::current_price(asset_id).unwrap().updated_in.local, 9);
			// then no price history is set
			assert!(StorageManager::<T>::price_history(asset_id).is_empty());
			// then no voting record exists
			assert!(StorageManager::<T>::block_with_votes(asset_id).is_empty());
		});
	}

	#[test]
	fn registers_weight_on_init() {
		todo!();
	}
}

mod on_session_change {
	use super::*;

	#[test]
	fn respects_max_authorities() {
		todo!()
	}

	#[test]
	fn updates_authorities_on_session_change() {
		todo!()
	}
}
