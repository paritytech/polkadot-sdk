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

use crate::{
	mock::*, weights::WeightInfo, CreditedUnits, Error, Event, PointsPerService, UnitsPerEraLimit,
};
use frame_support::{assert_noop, assert_ok, weights::Weight};
use sp_runtime::traits::BadOrigin;
use sp_staking::{
	RewardPoints,
	ServiceKind::{self, Collation, RelayConsensus},
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
		// and source 3 is refunded the weight of crediting and uses none of its limit
		assert_eq!(Rewarded::get(), vec![(Collation, vec![(10, 15)])]);
		assert!(!CreditedUnits::<Test>::contains_key(3, Collation));
		assert_eq!(
			events(),
			vec![
				Event::PointsPerServiceSet { source: 2, service: Collation, points: 5 },
				Event::PointsPerServiceSet { source: 3, service: Collation, points: 0 },
				Event::BlocksCredited { source: 2, blocks: 3 },
				Event::BlocksIgnored { source: 3, blocks: 4 },
			]
		);
		let own = <() as WeightInfo>::report_authored_blocks(1);
		assert_eq!(credited, Some(own + MockRewardPoints::weight(1)));
		assert_eq!(ignored, Some(own));

		// WHEN source 2 holds points per service above `MaxPointsPerService` of 10 and reports
		PointsPerService::<Test>::insert(2, Collation, 11);
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
		assert_eq!(empty, Some(<() as WeightInfo>::report_authored_blocks(0)));

		// WHEN no era is active and source 2 reports
		ActiveEra::set(None);
		let no_era = report(2, vec![(10, 3)]);
		// THEN its blocks are ignored without using its limit, and the weight of crediting is
		// refunded
		assert_eq!(Rewarded::get().len(), 2);
		assert_eq!(events(), vec![Event::BlocksIgnored { source: 2, blocks: 3 }]);
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (1, 6));
		assert_eq!(no_era, Some(own));
	});
}

#[test]
fn calls_reject_wrong_origin_and_points_over_the_cap() {
	new_test_ext().execute_with(|| {
		let set = |origin: RuntimeOrigin, source: u64, service: ServiceKind, points: u32| {
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

fn set_points(source: u64, points: u32) {
	assert_ok!(StakingPointsConverter::set_points_per_service(
		RuntimeOrigin::root(),
		source,
		Collation,
		points
	));
}

#[test]
fn era_limit_bounds_the_blocks_of_a_reporting_chain() {
	new_test_ext().execute_with(|| {
		// GIVEN chains 2 and 3 earn 5 points per block, era 1 is active and the limit is the
		// default of 200 blocks
		set_points(2, 5);
		set_points(3, 5);
		System::reset_events();

		// WHEN chain 2 reports two entries that fit
		report(2, vec![(10, 60), (11, 60)]);
		// THEN both are credited in full
		assert_eq!(Rewarded::get(), vec![(Collation, vec![(10, 300), (11, 300)])]);
		assert_eq!(events(), vec![Event::BlocksCredited { source: 2, blocks: 120 }]);
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (1, 120));

		// WHEN chain 2 reports 150 blocks with 80 left, account 10 listed twice
		System::reset_events();
		report(2, vec![(10, 50), (11, 50), (10, 50)]);
		// THEN every entry is cut by the same ratio, rounded down
		assert_eq!(
			Rewarded::get().last(),
			Some(&(Collation, vec![(10, 130), (11, 130), (10, 130)]))
		);
		assert_eq!(
			events(),
			vec![
				Event::BlocksCredited { source: 2, blocks: 78 },
				Event::BlocksOverCap { source: 2, era: 1, blocks: 72 },
			]
		);
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (1, 198));

		// WHEN chain 2 reports 3 blocks of 10 and none of 11 with 2 left
		System::reset_events();
		let weight = report(2, vec![(10, 3), (11, 0)]);
		// THEN 2 are credited and 1 is over the limit
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 10)])));
		assert_eq!(
			events(),
			vec![
				Event::BlocksCredited { source: 2, blocks: 2 },
				Event::BlocksOverCap { source: 2, era: 1, blocks: 1 },
			]
		);
		// AND the weight covers both entries but staking only for the credited one
		assert_eq!(
			weight,
			Some(<() as WeightInfo>::report_authored_blocks(2) + MockRewardPoints::weight(1))
		);

		// WHEN chain 2 reports with nothing left
		System::reset_events();
		let rewarded = Rewarded::get().len();
		let weight = report(2, vec![(10, 10)]);
		// THEN nothing reaches `RewardPoints` and the weight of crediting is refunded
		assert_eq!(Rewarded::get().len(), rewarded);
		assert_eq!(events(), vec![Event::BlocksOverCap { source: 2, era: 1, blocks: 10 }]);
		assert_eq!(weight, Some(<() as WeightInfo>::report_authored_blocks(1)));
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (1, 200));

		// WHEN chain 3 reports in the same era
		System::reset_events();
		report(3, vec![(10, 60)]);
		// THEN it is credited in full, since each chain has its own limit
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 300)])));
		assert_eq!(events(), vec![Event::BlocksCredited { source: 3, blocks: 60 }]);

		// WHEN era 2 becomes active and chain 2 reports
		ActiveEra::set(Some(2));
		System::reset_events();
		report(2, vec![(10, 30)]);
		// THEN its counter starts again from zero in era 2
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 150)])));
		assert_eq!(events(), vec![Event::BlocksCredited { source: 2, blocks: 30 }]);
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (2, 30));
	});
}

#[test]
fn entries_are_clamped_before_the_era_limit_applies() {
	new_test_ext().execute_with(|| {
		// GIVEN chain 2 earns 5 points per block in era 1, with a limit of 200 blocks and
		// `MaxBlocksPerEntry` of 100
		set_points(2, 5);
		System::reset_events();

		// WHEN it reports one entry far above `MaxBlocksPerEntry`
		report(2, vec![(10, 1_000_000), (11, 50)]);
		// THEN that entry is credited 100 blocks and the other entry keeps its full count
		assert_eq!(Rewarded::get(), vec![(Collation, vec![(10, 500), (11, 250)])]);
		assert_eq!(
			events(),
			vec![
				Event::BlocksCredited { source: 2, blocks: 150 },
				Event::BlocksOverCap { source: 2, era: 1, blocks: 999_900 },
			]
		);

		// WHEN it reports two maximal counts with 50 left
		System::reset_events();
		report(2, vec![(10, u32::MAX), (11, u32::MAX)]);
		// THEN both are clamped, then cut to 25 blocks each, and the blocks over the limit
		// saturate
		assert_eq!(Rewarded::get().last(), Some(&(Collation, vec![(10, 125), (11, 125)])));
		assert_eq!(
			events(),
			vec![
				Event::BlocksCredited { source: 2, blocks: 50 },
				Event::BlocksOverCap { source: 2, era: 1, blocks: u32::MAX },
			]
		);
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (1, 200));
	});
}

#[test]
fn governance_sets_the_era_limit_per_chain_and_service() {
	new_test_ext().execute_with(|| {
		let set = |origin: RuntimeOrigin, source: u64, service: ServiceKind, limit: u32| {
			StakingPointsConverter::set_units_per_era_limit(origin, source, service, limit)
		};
		// GIVEN chains 2 and 3 earn 5 points per block in era 1
		set_points(2, 5);
		set_points(3, 5);

		// WHEN account 4 sets a limit
		// THEN it fails with BadOrigin
		assert_noop!(set(RuntimeOrigin::signed(4), 2, Collation, 30), BadOrigin);

		// WHEN chain 2 sets its own limit
		// THEN it fails with BadOrigin
		assert_noop!(set(RuntimeOrigin::signed(2), 2, Collation, 30), BadOrigin);

		// WHEN root sets a limit for relay consensus, whose points the converter does not credit
		// THEN it fails with RelayConsensusNotWeighted
		assert_noop!(
			set(RuntimeOrigin::root(), 2, RelayConsensus, 30),
			Error::<Test>::RelayConsensusNotWeighted
		);

		// WHEN root sets a limit of 1001, above `MaxUnitsPerEraLimit` of 1000
		// THEN it fails with UnitsPerEraLimitTooHigh
		assert_noop!(
			set(RuntimeOrigin::root(), 2, Collation, 1_001),
			Error::<Test>::UnitsPerEraLimitTooHigh
		);

		// WHEN root sets a limit of 1000
		// THEN it is stored
		assert_ok!(set(RuntimeOrigin::root(), 2, Collation, 1_000));
		assert_eq!(UnitsPerEraLimit::<Test>::get(2, Collation), 1_000);

		// WHEN root sets 30 for chain 2 and both chains report 50 blocks
		System::reset_events();
		assert_ok!(set(RuntimeOrigin::root(), 2, Collation, 30));
		report(2, vec![(10, 50)]);
		report(3, vec![(10, 50)]);
		// THEN only chain 2 is cut to its limit
		assert_eq!(
			events(),
			vec![
				Event::UnitsPerEraLimitSet { source: 2, service: Collation, limit: 30 },
				Event::BlocksCredited { source: 2, blocks: 30 },
				Event::BlocksOverCap { source: 2, era: 1, blocks: 20 },
				Event::BlocksCredited { source: 3, blocks: 50 },
			]
		);

		// WHEN root lowers the limit of chain 2 below what it was credited in the era and chain 2
		// reports
		System::reset_events();
		let rewarded = Rewarded::get().len();
		assert_ok!(set(RuntimeOrigin::root(), 2, Collation, 10));
		report(2, vec![(10, 5)]);
		// THEN nothing more is credited and the credited units stay
		assert_eq!(Rewarded::get().len(), rewarded);
		assert_eq!(
			events(),
			vec![
				Event::UnitsPerEraLimitSet { source: 2, service: Collation, limit: 10 },
				Event::BlocksOverCap { source: 2, era: 1, blocks: 5 },
			]
		);
		assert_eq!(CreditedUnits::<Test>::get(2, Collation), (1, 30));

		// WHEN root sets 0 for chain 3 and it reports
		System::reset_events();
		assert_ok!(set(RuntimeOrigin::root(), 3, Collation, 0));
		report(3, vec![(10, 5)]);
		// THEN chain 3 is credited nothing, its limit of zero is stored and its points per
		// service are kept
		assert_eq!(Rewarded::get().len(), rewarded);
		assert_eq!(
			events(),
			vec![
				Event::UnitsPerEraLimitSet { source: 3, service: Collation, limit: 0 },
				Event::BlocksOverCap { source: 3, era: 1, blocks: 5 },
			]
		);
		assert!(UnitsPerEraLimit::<Test>::contains_key(3, Collation));
		assert_eq!(PointsPerService::<Test>::get(3, Collation), 5);

		// WHEN chain 2 holds a limit above `MaxUnitsPerEraLimit` of 1000 and reports 2000 blocks
		// in era 2
		UnitsPerEraLimit::<Test>::insert(2, Collation, 5_000);
		ActiveEra::set(Some(2));
		System::reset_events();
		report(2, (0..20).map(|who| (who, 100)).collect());
		// THEN it is credited up to the bound
		assert_eq!(
			events(),
			vec![
				Event::BlocksCredited { source: 2, blocks: 1_000 },
				Event::BlocksOverCap { source: 2, era: 2, blocks: 1_000 },
			]
		);
	});
}
