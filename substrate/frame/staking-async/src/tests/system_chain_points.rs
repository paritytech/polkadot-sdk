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

//! Tests for era points earned outside the relay chain.

use super::*;

fn points_of(era: EraIndex) -> (RewardPoint, Vec<(AccountId, RewardPoint)>) {
	let points = ErasRewardPoints::<Test>::get(era);
	(points.total, points.individual.into_iter().collect())
}

#[test]
fn points_credit_exposed_validators_of_the_active_era_only() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN era 1 is active with 11 and 21 exposed, and 11 earned relay chain points
		Session::roll_until_active_era(1);
		assert!(ErasStakersOverview::<Test>::contains_key(1, 11));
		assert!(ErasStakersOverview::<Test>::contains_key(1, 21));
		assert!(!ErasStakersOverview::<Test>::contains_key(1, 999));
		Staking::reward_by_ids(vec![(11, 20)]);
		let _ = staking_events_since_last_call();

		// WHEN points are credited while no era is active
		// THEN nothing is written and no credit is reported
		hypothetically!({
			let before = points_of(1);
			ActiveEra::<Test>::kill();
			System::reset_events();
			Staking::reward_system_chain_points(vec![(11, 10)]);
			assert_eq!(points_of(1), before);
			assert!(System::events().is_empty());
		});

		// WHEN era 1 has no points yet and points are credited for the unexposed 999 only
		// THEN no era points are written and the drop is reported
		hypothetically!({
			ErasRewardPoints::<Test>::remove(1);
			System::reset_events();
			Staking::reward_system_chain_points(vec![(999, 7)]);
			assert!(!ErasRewardPoints::<Test>::contains_key(1));
			System::assert_last_event(
				Event::SystemChainPointsCredited { era: 1, credited: 0, dropped: 1 }.into(),
			);
		});

		// WHEN points are credited for 11 twice, 21 and the unexposed 999
		Staking::reward_system_chain_points(vec![(11, 10), (21, 5), (999, 7), (11, 1)]);

		// THEN the exposed entries merge with the relay chain points, 999 counts in neither its
		// own entry nor the total, so it dilutes no payout of the era
		assert_eq!(points_of(1), (36, vec![(11, 31), (21, 5)]));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SystemChainPointsCredited { era: 1, credited: 3, dropped: 1 }]
		);
	});
}

#[test]
fn closed_era_split_never_changes() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN era 1 with points for 11 and 21 has ended and era 2 is active
		Session::roll_until_active_era(1);
		Staking::reward_by_ids(vec![(11, 20), (21, 10)]);
		Session::roll_until_active_era(2);
		assert!(ErasStakersOverview::<Test>::contains_key(2, 11));
		let closed = points_of(1);
		let rewards_of_11_in_era_1 = || {
			hypothetically!({
				System::reset_events();
				assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, 1));
				System::events()
					.into_iter()
					.filter_map(|record| match record.event {
						RuntimeEvent::Staking(Event::Rewarded { stash, amount, .. }) => {
							Some((stash, amount))
						},
						_ => None,
					})
					.collect::<Vec<_>>()
			})
		};
		let rewards_before = rewards_of_11_in_era_1();
		assert!(!rewards_before.is_empty());

		// WHEN points for 11 are credited
		Staking::reward_system_chain_points(vec![(11, 7)]);

		// THEN era 2 holds them, era 1 is untouched, and so is every payout of era 1
		assert_eq!(points_of(1), closed);
		assert_eq!(points_of(2), (7, vec![(11, 7)]));
		assert_eq!(rewards_of_11_in_era_1(), rewards_before);
	});
}
