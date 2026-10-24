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

//! Tests for era points earned for services other than relay chain consensus.

use super::*;

fn points_of(era: EraIndex) -> (RewardPoint, Vec<(AccountId, RewardPoint)>) {
	let points = ErasRewardPoints::<Test>::get(era);
	(points.total, points.individual.into_iter().collect())
}

fn points_of_service(
	era: EraIndex,
	service: ServiceKind,
) -> (RewardPoint, Vec<(AccountId, RewardPoint)>) {
	let points = ErasRewardPointsByService::<Test>::get(era, service);
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
			Staking::reward_service_points(ServiceKind::Collation, vec![(11, 10)]);
			assert_eq!(points_of(1), before);
			assert!(!ErasRewardPointsByService::<Test>::contains_key(1, ServiceKind::Collation));
			assert!(System::events().is_empty());
		});

		// WHEN era 1 has no points yet and points are credited for the unexposed 999 only
		// THEN no era points are written and the drop is reported
		hypothetically!({
			ErasRewardPoints::<Test>::remove(1);
			System::reset_events();
			Staking::reward_service_points(ServiceKind::Collation, vec![(999, 7)]);
			assert!(!ErasRewardPoints::<Test>::contains_key(1));
			assert!(!ErasRewardPointsByService::<Test>::contains_key(1, ServiceKind::Collation));
			System::assert_last_event(
				Event::ServicePointsCredited {
					service: ServiceKind::Collation,
					era: 1,
					credited: 0,
					dropped: 1,
				}
				.into(),
			);
		});

		// WHEN points are credited for 11 twice, 21 and the unexposed 999
		Staking::reward_service_points(
			ServiceKind::Collation,
			vec![(11, 10), (21, 5), (999, 7), (11, 1)],
		);

		// THEN the exposed entries merge with the relay chain points, 999 counts in neither its
		// own entry nor the total, so it dilutes no payout of the era
		assert_eq!(points_of(1), (36, vec![(11, 31), (21, 5)]));
		// AND each service holds exactly its own points, so the split adds up to the tally
		assert_eq!(points_of_service(1, ServiceKind::RelayConsensus), (20, vec![(11, 20)]));
		assert_eq!(points_of_service(1, ServiceKind::Collation), (16, vec![(11, 11), (21, 5)]));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::ServicePointsCredited {
				service: ServiceKind::Collation,
				era: 1,
				credited: 3,
				dropped: 1
			}]
		);
	});
}

#[test]
fn relay_session_report_points_are_recorded_as_relay_consensus() {
	use crate::tests::session_mock::CurrentIndex;
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN era 1 is active, and 21 earned collation points
		Session::roll_until_active_era(1);
		Staking::reward_service_points(ServiceKind::Collation, vec![(21, 3)]);

		// WHEN the relay chain reports the end of a session with points for 11 and 21
		let ending = CurrentIndex::get();
		<Staking as pallet_staking_async_rc_client::AHStakingInterface>::on_relay_session_report(
			pallet_staking_async_rc_client::SessionReport::new_terminal(
				ending,
				vec![(11, 20), (21, 10)],
				None,
			),
		);
		CurrentIndex::set(ending + 1);

		// THEN the reported points are recorded as relay consensus points, next to the collation
		// points, and the tally of era 1 is their sum
		assert_eq!(
			points_of_service(1, ServiceKind::RelayConsensus),
			(30, vec![(11, 20), (21, 10)])
		);
		assert_eq!(points_of_service(1, ServiceKind::Collation), (3, vec![(21, 3)]));
		assert_eq!(points_of(1), (33, vec![(11, 20), (21, 13)]));
	});
}

#[test]
fn closed_era_split_never_changes() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN era 1 with points for 11 and 21 has ended and era 2 is active
		Session::roll_until_active_era(1);
		Staking::reward_by_ids(vec![(11, 20), (21, 10)]);
		Staking::reward_service_points(ServiceKind::Collation, vec![(21, 3)]);
		Session::roll_until_active_era(2);
		assert!(ErasStakersOverview::<Test>::contains_key(2, 11));
		let closed = points_of(1);
		let closed_by_service = [ServiceKind::RelayConsensus, ServiceKind::Collation]
			.map(|service| points_of_service(1, service));
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
		Staking::reward_service_points(ServiceKind::Collation, vec![(11, 7)]);

		// THEN era 2 holds them, era 1 and its split are untouched, and so is every payout of
		// era 1
		assert_eq!(points_of(1), closed);
		assert_eq!(
			[ServiceKind::RelayConsensus, ServiceKind::Collation]
				.map(|service| points_of_service(1, service)),
			closed_by_service
		);
		assert_eq!(points_of(2), (7, vec![(11, 7)]));
		assert_eq!(points_of_service(2, ServiceKind::Collation), (7, vec![(11, 7)]));
		assert_eq!(rewards_of_11_in_era_1(), rewards_before);
	});
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "Defensive failure has been triggered!"))]
fn validation_points_are_refused() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN era 1 is active with 11 exposed and relay chain points for 11
		Session::roll_until_active_era(1);
		Staking::reward_by_ids(vec![(11, 20)]);
		let before = (points_of(1), points_of_service(1, ServiceKind::RelayConsensus));
		let _ = staking_events_since_last_call();

		// WHEN relay consensus points are credited outside the relay chain session report
		Staking::reward_service_points(ServiceKind::RelayConsensus, vec![(11, 10)]);

		// THEN nothing is written and no event is emitted, since only the relay chain session
		// report credits relay consensus points
		assert_eq!((points_of(1), points_of_service(1, ServiceKind::RelayConsensus)), before);
		assert!(staking_events_since_last_call().is_empty());
	});
}
