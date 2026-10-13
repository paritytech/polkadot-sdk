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

use crate::{mock::*, weights::WeightInfo, Error, Event, PointsPerUnit, RewardPoints};
use frame_support::{assert_noop, assert_ok, weights::Weight};
use sp_runtime::traits::BadOrigin;
use sp_staking::PointsKind::{self, Collation, Validation};

fn events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::StakingSystemChainClient(event) => Some(event),
			_ => None,
		})
		.collect()
}

fn report(source: u64, blocks: Vec<(u64, u32)>) -> Option<Weight> {
	let result =
		StakingSystemChainClient::report_authored_blocks(RuntimeOrigin::signed(source), blocks);
	assert_ok!(result);
	result.unwrap().actual_weight
}

#[test]
fn blocks_are_credited_with_the_weight_of_their_source() {
	new_test_ext().execute_with(|| {
		// GIVEN source 2 earns 5 points per unit and source 3 is set to zero, which removes
		// its entry
		assert_ok!(StakingSystemChainClient::set_points_per_unit(
			RuntimeOrigin::root(),
			2,
			Collation,
			5
		));
		System::assert_last_event(
			Event::PointsPerUnitSet { source: 2, kind: Collation, points: 5 }.into(),
		);
		assert_ok!(StakingSystemChainClient::set_points_per_unit(
			RuntimeOrigin::root(),
			3,
			Collation,
			0
		));
		assert!(!PointsPerUnit::<Test>::contains_key(3, Collation));

		// WHEN source 2 reports 3 blocks of 10 and source 3 reports 4 blocks of 10
		let credited = report(2, vec![(10, 3)]);
		let ignored = report(3, vec![(10, 4)]);

		// THEN only source 2 is credited, with its own points per unit, as collation points,
		// and source 3 is refunded the weight of crediting
		assert_eq!(Rewarded::get(), vec![(Collation, vec![(10, 15)])]);
		assert_eq!(
			events(),
			vec![
				Event::PointsPerUnitSet { source: 2, kind: Collation, points: 5 },
				Event::PointsPerUnitSet { source: 3, kind: Collation, points: 0 },
				Event::BlocksCredited { source: 2, entries: 1 },
				Event::BlocksIgnored { source: 3, entries: 1 },
			]
		);
		let own = <() as WeightInfo>::report_authored_blocks();
		assert_eq!(credited, Some(own + MockRewardPoints::weight(1)));
		assert_eq!(ignored, Some(own));

		// WHEN source 2 holds points per unit above `MaxPointsPerUnit` of 10 and reports
		PointsPerUnit::<Test>::insert(2, Collation, 11);
		report(2, vec![(10, 3)]);
		// THEN its blocks are credited at the cap
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 30)])));

		// WHEN source 2 reports no entries
		System::reset_events();
		let empty = report(2, vec![]);
		// THEN nothing is handed to `RewardPoints`, no event is deposited and the weight of
		// crediting is refunded
		assert_eq!(Rewarded::get().len(), 2);
		assert!(events().is_empty());
		assert_eq!(empty, Some(own));
	});
}

#[test]
fn calls_reject_wrong_origin_and_points_over_the_cap() {
	new_test_ext().execute_with(|| {
		let set = |origin: RuntimeOrigin, source: u64, kind: PointsKind, points: u32| {
			StakingSystemChainClient::set_points_per_unit(origin, source, kind, points)
		};
		// GIVEN account 4, which is neither root nor a reporting chain
		let other = RuntimeOrigin::signed(4);
		// WHEN it sets points per unit or reports blocks
		// THEN both calls fail with BadOrigin
		assert_noop!(set(other.clone(), 4, Collation, 5), BadOrigin);
		assert_noop!(
			StakingSystemChainClient::report_authored_blocks(other, vec![(10, 1)])
				.map_err(|e| e.error),
			BadOrigin
		);
		// AND a reporting chain cannot set its own points per unit
		assert_noop!(set(RuntimeOrigin::signed(2), 2, Collation, 5), BadOrigin);

		// WHEN root sets points per unit above `MaxPointsPerUnit` of 10
		// THEN it fails, and the cap itself is accepted
		assert_noop!(
			set(RuntimeOrigin::root(), 2, Collation, 11),
			Error::<Test>::PointsPerUnitTooHigh
		);
		assert_ok!(set(RuntimeOrigin::root(), 2, Collation, 10));
		assert_eq!(PointsPerUnit::<Test>::get(2, Collation), 10);

		// WHEN root weights validation points
		// THEN it fails, since relay chain points are weighted by the relay chain
		assert_noop!(
			set(RuntimeOrigin::root(), 2, Validation, 1),
			Error::<Test>::ValidationNotWeighted
		);
	});
}
