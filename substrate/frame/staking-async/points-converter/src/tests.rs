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

use crate::{mock::*, weights::WeightInfo, Error, Event, PointsPerService};
use frame_support::{assert_noop, assert_ok, weights::Weight};
use sp_runtime::traits::BadOrigin;
use sp_staking::{
	RewardPoints,
	ValidatorServiceKind::{self, Collation, RelayConsensus},
};

fn events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::StakingPointsConverter(event) => Some(event),
			_ => None,
		})
		.collect()
}

fn report(source: u64, blocks: Vec<(u64, u32)>) -> Option<Weight> {
	let result =
		StakingPointsConverter::report_authored_blocks(RuntimeOrigin::signed(source), blocks);
	assert_ok!(result);
	result.unwrap().actual_weight
}

#[test]
fn blocks_are_credited_with_the_weight_of_their_source() {
	new_test_ext().execute_with(|| {
		// GIVEN source 2 earns 5 points per service and source 3 is set to zero, which removes
		// its entry
		assert_ok!(StakingPointsConverter::set_points_per_service(
			RuntimeOrigin::root(),
			2,
			Collation,
			5
		));
		System::assert_last_event(
			Event::PointsPerServiceSet { source: 2, service: Collation, points: 5 }.into(),
		);
		assert_ok!(StakingPointsConverter::set_points_per_service(
			RuntimeOrigin::root(),
			3,
			Collation,
			0
		));
		assert!(!PointsPerService::<Test>::contains_key(3, Collation));

		// WHEN source 2 reports 3 blocks of 10 and source 3 reports 4 blocks of 10
		let credited = report(2, vec![(10, 3)]);
		let ignored = report(3, vec![(10, 4)]);

		// THEN only source 2 is credited, with its own points per service, as collation points,
		// and source 3 is refunded the weight of crediting
		assert_eq!(Rewarded::get(), vec![(Collation, vec![(10, 15)])]);
		assert_eq!(
			events(),
			vec![
				Event::PointsPerServiceSet { source: 2, service: Collation, points: 5 },
				Event::PointsPerServiceSet { source: 3, service: Collation, points: 0 },
				Event::BlocksCredited { source: 2, entries: 1 },
				Event::BlocksIgnored { source: 3, entries: 1 },
			]
		);
		let own = <() as WeightInfo>::report_authored_blocks();
		assert_eq!(credited, Some(own + MockRewardPoints::weight(1)));
		assert_eq!(ignored, Some(own));

		// WHEN source 2 holds points per service above `MaxPointsPerService` of 10 and reports
		PointsPerService::<Test>::insert(2, Collation, 11);
		report(2, vec![(10, 3)]);
		// THEN its blocks are credited at the cap
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 30)])));

		// WHEN source 2 reports a count above `MaxBlocksPerEntry` of 100
		report(2, vec![(10, 101), (11, 100)]);
		// THEN that count is credited as 100 blocks, and the count at the cap is credited in full
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 1_000), (11, 1_000)])));

		// WHEN source 2 reports no entries
		System::reset_events();
		let empty = report(2, vec![]);
		// THEN nothing is handed to `RewardPoints`, no event is deposited and the weight of
		// crediting is refunded
		assert_eq!(Rewarded::get().len(), 3);
		assert!(events().is_empty());
		assert_eq!(empty, Some(own));
	});
}

#[test]
fn calls_reject_wrong_origin_and_points_over_the_cap() {
	new_test_ext().execute_with(|| {
		let set =
			|origin: RuntimeOrigin, source: u64, service: ValidatorServiceKind, points: u32| {
				StakingPointsConverter::set_points_per_service(origin, source, service, points)
			};
		// GIVEN account 4, which is neither root nor a reporting chain
		let other = RuntimeOrigin::signed(4);
		// WHEN it sets points per service or reports blocks
		// THEN both calls fail with BadOrigin
		assert_noop!(set(other.clone(), 4, Collation, 5), BadOrigin);
		assert_noop!(
			StakingPointsConverter::report_authored_blocks(other, vec![(10, 1)])
				.map_err(|e| e.error),
			BadOrigin
		);
		// AND a reporting chain cannot set its own points per service
		assert_noop!(set(RuntimeOrigin::signed(2), 2, Collation, 5), BadOrigin);

		// WHEN root sets points per service above `MaxPointsPerService` of 10
		// THEN it fails, and the cap itself is accepted
		assert_noop!(
			set(RuntimeOrigin::root(), 2, Collation, 11),
			Error::<Test>::PointsPerServiceTooHigh
		);
		assert_ok!(set(RuntimeOrigin::root(), 2, Collation, 10));
		assert_eq!(PointsPerService::<Test>::get(2, Collation), 10);

		// WHEN root weights relay consensus points
		// THEN it fails, since relay chain points are weighted by the relay chain
		assert_noop!(
			set(RuntimeOrigin::root(), 2, RelayConsensus, 1),
			Error::<Test>::RelayConsensusNotWeighted
		);
	});
}
