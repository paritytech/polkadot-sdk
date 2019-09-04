// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for the module.

use super::*;
use mock::*;

use support::{assert_ok, assert_noop};
use runtime_io::with_externalities;
use sr_primitives::traits::OnInitialize;

type ScoredPool = Module<Test>;
type System = system::Module<Test>;
type Balances = balances::Module<Test>;

const OOB_ERR: &str = "index out of bounds";
const INDEX_ERR: &str = "index does not match requested account";

#[test]
fn query_membership_works() {
	with_externalities(&mut new_test_ext(), || {
		assert_eq!(ScoredPool::members(), vec![20, 40]);
		assert_eq!(Balances::reserved_balance(&31), CandidateDeposit::get());
		assert_eq!(Balances::reserved_balance(&40), CandidateDeposit::get());
		assert_eq!(MEMBERS.with(|m| m.borrow().clone()), vec![20, 40]);
	});
}

#[test]
fn submit_candidacy_must_not_work() {
	with_externalities(&mut new_test_ext(), || {
		assert_noop!(
			ScoredPool::submit_candidacy(Origin::signed(99)),
			"balance too low to submit candidacy"
		);
		assert_noop!(
			ScoredPool::submit_candidacy(Origin::signed(40)),
			"already a member"
		);
	});
}

#[test]
fn submit_candidacy_works() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 15;

		// when
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(who)));
		assert_eq!(fetch_from_pool(15), Some((who, None)));

		// then
		assert_eq!(Balances::reserved_balance(&who), CandidateDeposit::get());
	});
}

#[test]
fn scoring_works() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 15;
		let score = 99;
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(who)));

		// when
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::score(Origin::signed(ScoreOrigin::get()), who, index, score));

		// then
		assert_eq!(fetch_from_pool(who), Some((who, Some(score))));
		assert_eq!(find_in_pool(who), Some(0)); // must be first element, since highest scored
	});
}

#[test]
fn scoring_same_element_with_same_score_works() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 31;
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		let score = 2;

		// when
		assert_ok!(ScoredPool::score(Origin::signed(ScoreOrigin::get()), who, index, score));

		// then
		assert_eq!(fetch_from_pool(who), Some((who, Some(score))));

		// must have been inserted right before the `20` element which is
		// of the same score as `31`. so sort order is maintained.
		assert_eq!(find_in_pool(who), Some(1));
	});
}

#[test]
fn kicking_works_only_for_authorized() {
	with_externalities(&mut new_test_ext(), || {
		let who = 40;
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_noop!(ScoredPool::kick(Origin::signed(99), who, index), "bad origin");
	});
}

#[test]
fn kicking_works() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 40;
		assert_eq!(Balances::reserved_balance(&who), CandidateDeposit::get());
		assert_eq!(find_in_pool(who), Some(0));

		// when
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::kick(Origin::signed(KickOrigin::get()), who, index));

		// then
		assert_eq!(find_in_pool(who), None);
		assert_eq!(ScoredPool::members(), vec![20, 31]);
		assert_eq!(MEMBERS.with(|m| m.borrow().clone()), ScoredPool::members());
		assert_eq!(Balances::reserved_balance(&who), 0); // deposit must have been returned
	});
}

#[test]
fn unscored_entities_must_not_be_used_for_filling_members() {
	with_externalities(&mut new_test_ext(), || {
		// given
		// we submit a candidacy, score will be `None`
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(15)));

		// when
		// we remove every scored member
		ScoredPool::pool()
			.into_iter()
			.for_each(|(who, score)| {
				if let Some(_) = score {
					let index = find_in_pool(who).expect("entity must be in pool") as u32;
					assert_ok!(ScoredPool::kick(Origin::signed(KickOrigin::get()), who, index));
				}
			});

		// then
		// the `None` candidates should not have been filled in
		assert_eq!(ScoredPool::members(), vec![]);
		assert_eq!(MEMBERS.with(|m| m.borrow().clone()), ScoredPool::members());
	});
}

#[test]
fn refreshing_works() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 15;
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(who)));
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::score(Origin::signed(ScoreOrigin::get()), who, index, 99));

		// when
		ScoredPool::refresh_members(ScoredPool::pool(), ChangeReceiver::MembershipChanged);

		// then
		assert_eq!(ScoredPool::members(), vec![15, 40]);
		assert_eq!(MEMBERS.with(|m| m.borrow().clone()), ScoredPool::members());
	});
}

#[test]
fn refreshing_happens_every_period() {
	with_externalities(&mut new_test_ext(), || {
		// given
		System::set_block_number(1);
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(15)));
		let index = find_in_pool(15).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::score(Origin::signed(ScoreOrigin::get()), 15, index, 99));
		assert_eq!(ScoredPool::members(), vec![20, 40]);

		// when
		System::set_block_number(4);
		ScoredPool::on_initialize(4);

		// then
		assert_eq!(ScoredPool::members(), vec![15, 40]);
		assert_eq!(MEMBERS.with(|m| m.borrow().clone()), ScoredPool::members());
	});
}

#[test]
fn withdraw_candidacy_must_only_work_for_members() {
	with_externalities(&mut new_test_ext(), || {
		let who = 77;
		let index = 0;
		assert_noop!( ScoredPool::withdraw_candidacy(Origin::signed(who), index), INDEX_ERR);
	});
}

#[test]
fn oob_index_should_abort() {
	with_externalities(&mut new_test_ext(), || {
		let who = 40;
		let oob_index = ScoredPool::pool().len() as u32;
		assert_noop!(ScoredPool::withdraw_candidacy(Origin::signed(who), oob_index), OOB_ERR);
		assert_noop!(ScoredPool::score(Origin::signed(ScoreOrigin::get()), who, oob_index, 99), OOB_ERR);
		assert_noop!(ScoredPool::kick(Origin::signed(KickOrigin::get()), who, oob_index), OOB_ERR);
	});
}

#[test]
fn index_mismatches_should_abort() {
	with_externalities(&mut new_test_ext(), || {
		let who = 40;
		let index = 3;
		assert_noop!(ScoredPool::withdraw_candidacy(Origin::signed(who), index), INDEX_ERR);
		assert_noop!(ScoredPool::score(Origin::signed(ScoreOrigin::get()), who, index, 99), INDEX_ERR);
		assert_noop!(ScoredPool::kick(Origin::signed(KickOrigin::get()), who, index), INDEX_ERR);
	});
}

#[test]
fn withdraw_unscored_candidacy_must_work() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 5;

		// when
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::withdraw_candidacy(Origin::signed(who), index));

		// then
		assert_eq!(fetch_from_pool(5), None);
	});
}

#[test]
fn withdraw_scored_candidacy_must_work() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 40;
		assert_eq!(Balances::reserved_balance(&who), CandidateDeposit::get());

		// when
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::withdraw_candidacy(Origin::signed(who), index));

		// then
		assert_eq!(fetch_from_pool(who), None);
		assert_eq!(ScoredPool::members(), vec![20, 31]);
		assert_eq!(Balances::reserved_balance(&who), 0);
	});
}

#[test]
fn candidacy_resubmitting_works() {
	with_externalities(&mut new_test_ext(), || {
		// given
		let who = 15;

		// when
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(who)));
		assert_eq!(ScoredPool::candidate_exists(who), true);
		let index = find_in_pool(who).expect("entity must be in pool") as u32;
		assert_ok!(ScoredPool::withdraw_candidacy(Origin::signed(who), index));
		assert_eq!(ScoredPool::candidate_exists(who), false);
		assert_ok!(ScoredPool::submit_candidacy(Origin::signed(who)));

		// then
		assert_eq!(ScoredPool::candidate_exists(who), true);
	});
}
