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

use crate::{mock::*, weights::WeightInfo, Event};
use frame_support::{assert_err, assert_ok};
use pallet_validator_collators::{
	Error as ReceiverError, PendingRotation, RotationState, ValidatorSet,
};

fn announcement_events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::ValidatorSetAnnouncer(event) => Some(event),
			_ => None,
		})
		.collect()
}

fn run_block(n: u64) {
	use frame_support::traits::{OnFinalize, OnInitialize};
	AllPalletsWithSystem::on_finalize(System::block_number());
	System::set_block_number(n);
	AllPalletsWithSystem::on_initialize(n);
	AllPalletsWithSystem::on_finalize(n);
}

fn stored() -> Option<(u32, Vec<u64>)> {
	ValidatorSet::<Test>::get().map(|set| (set.era, set.validators.into_iter().collect()))
}

#[test]
fn announce_stores_the_set_and_sends_it_once_to_every_destination() {
	new_test_ext().execute_with(|| {
		// GIVEN destination 2 rejects every send
		FailingDestinations::set(vec![2]);
		// WHEN the era 1 set is announced
		assert_ok!(ValidatorSetAnnouncer::announce(1, &[10, 11]));
		// THEN it is stored locally, destination 1 got it, and each send is reported
		assert_eq!(stored(), Some((1, vec![10, 11])));
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingQueue);
		assert_eq!(Sent::get(), vec![(1, 1, vec![10, 11])]);
		assert_eq!(
			announcement_events(),
			vec![
				Event::AnnouncementSent { destination: 1, era: 1 },
				Event::AnnouncementFailed { destination: 2, era: 1 },
			]
		);
		// WHEN a full block runs and destination 2 accepts again
		FailingDestinations::set(vec![]);
		run_block(2);
		// THEN the failed send is not retried, only the next era's set reaches destination 2
		assert_eq!(Sent::get().len(), 1);
		assert_eq!(announcement_events().len(), 2);
		assert_ok!(ValidatorSetAnnouncer::announce(2, &[12]));
		assert_eq!(Sent::get(), vec![(1, 1, vec![10, 11]), (1, 2, vec![12]), (2, 2, vec![12])]);
	});
}

#[test]
fn rejected_set_is_reported_and_neither_stored_nor_sent() {
	new_test_ext().execute_with(|| {
		// GIVEN the era 1 set is stored and sent
		assert_ok!(ValidatorSetAnnouncer::announce(1, &[10]));
		let sent = Sent::get();
		// WHEN the same era is announced again
		// THEN it fails with StaleEra, is reported, and nothing is stored or sent
		assert_err!(ValidatorSetAnnouncer::announce(1, &[11]), ReceiverError::<Test>::StaleEra);
		System::assert_last_event(
			Event::AnnouncementRejected { era: 1, error: ReceiverError::<Test>::StaleEra.into() }
				.into(),
		);
		// WHEN a later era has one account more than MaxValidators
		// THEN it fails with TooManyValidators, is reported, and nothing is stored or sent
		let too_many = (100..151).collect::<Vec<u64>>();
		assert_err!(
			ValidatorSetAnnouncer::announce(2, &too_many),
			ReceiverError::<Test>::TooManyValidators
		);
		System::assert_last_event(
			Event::AnnouncementRejected {
				era: 2,
				error: ReceiverError::<Test>::TooManyValidators.into(),
			}
			.into(),
		);
		assert_eq!(stored(), Some((1, vec![10])));
		assert_eq!(Sent::get(), sent);
	});
}

#[test]
fn announce_weight_charges_the_local_store_and_one_send_per_destination() {
	new_test_ext().execute_with(|| {
		// GIVEN two destinations
		let store = <() as WeightInfo>::announce(5);
		let send = <() as WeightInfo>::send_announcement(5);
		// WHEN a set of 5 is weighed
		// THEN it costs the store and two sends, and only the store without destinations
		assert_eq!(ValidatorSetAnnouncer::announce_weight(5), store + send.saturating_mul(2));
		Destinations::set(vec![]);
		assert_eq!(ValidatorSetAnnouncer::announce_weight(5), store);
	});
}
