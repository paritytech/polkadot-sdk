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

use crate::{mock::*, weights::WeightInfo, Error, Event, OutgoingAnnouncements};
use frame_support::{assert_err, assert_ok};
use pallet_validator_collators::{
	Error as ReceiverError, PendingRotation, RotationState, ValidatorSet,
};
use sp_runtime::DispatchResult;
use sp_staking::EraIndex;

fn announce(era: EraIndex, validators: Vec<u64>) -> DispatchResult {
	ValidatorSetAnnouncer::announce(era, &validators)
}

fn announcement_events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::ValidatorSetAnnouncer(
				event @ (Event::AnnouncementSent { .. } |
				Event::AnnouncementFailed { .. } |
				Event::AnnouncementRejected { .. }),
			) => Some(event),
			_ => None,
		})
		.collect()
}

fn outgoing() -> Vec<u32> {
	OutgoingAnnouncements::<Test>::get().into_inner()
}

#[test]
fn announce_stores_the_set_sends_it_next_block_and_rejects_a_repeat() {
	new_test_ext().execute_with(|| {
		// GIVEN two destinations that accept the set
		initialize_to_block(1);
		// WHEN a set is announced
		assert_ok!(announce(1, vec![10, 11]));
		// THEN it is stored locally and queued, and the next block sends it to both destinations
		assert_eq!(ValidatorSet::<Test>::get().map(|set| set.era), Some(1));
		assert_eq!(PendingRotation::<Test>::get(), RotationState::ToPlan);
		assert_eq!(outgoing(), vec![1, 2]);
		assert!(Sent::get().is_empty());
		initialize_to_block(2);
		assert_eq!(Sent::get(), vec![(1, 1, vec![10, 11]), (2, 1, vec![10, 11])]);
		assert_eq!(outgoing(), Vec::<u32>::new());
		assert_eq!(
			announcement_events(),
			vec![
				Event::AnnouncementSent { destination: 1, era: 1 },
				Event::AnnouncementSent { destination: 2, era: 1 },
			]
		);
		// WHEN the same era is announced again
		// THEN it fails with StaleEra, is reported and nothing is queued or sent
		assert_err!(announce(1, vec![11]), ReceiverError::<Test>::StaleEra);
		System::assert_last_event(
			Event::AnnouncementRejected { era: 1, error: ReceiverError::<Test>::StaleEra.into() }
				.into(),
		);
		initialize_to_block(3);
		assert_eq!(Sent::get().len(), 2);
		assert_eq!(outgoing(), Vec::<u32>::new());
		assert_eq!(
			ValidatorSet::<Test>::get().map(|set| set.validators.to_vec()),
			Some(vec![10, 11])
		);
		assert_ok!(ValidatorSetAnnouncer::do_try_state());
	});
}

#[test]
fn failed_send_is_retried_every_block_until_sent_or_replaced() {
	new_test_ext().execute_with(|| {
		// GIVEN destination 2 rejects every send
		initialize_to_block(1);
		FailingDestinations::set(vec![2]);
		// WHEN the era 1 set is announced and three blocks pass
		assert_ok!(announce(1, vec![10]));
		initialize_to_block(4);
		// THEN destination 1 got it once and destination 2 is still queued after three failures
		assert_eq!(Sent::get(), vec![(1, 1, vec![10])]);
		assert_eq!(outgoing(), vec![2]);
		assert_eq!(
			announcement_events(),
			vec![
				Event::AnnouncementSent { destination: 1, era: 1 },
				Event::AnnouncementFailed { destination: 2, era: 1 },
				Event::AnnouncementFailed { destination: 2, era: 1 },
				Event::AnnouncementFailed { destination: 2, era: 1 },
			]
		);
		// WHEN the single pending destination is retried
		// THEN the retry is charged one send
		let one_send = <() as WeightInfo>::send_announcement(1);
		assert_eq!(ValidatorSetAnnouncer::send_announcements(), one_send);
		// WHEN the era 2 set is announced, destination 2 accepts again and the queue is drained
		assert_ok!(announce(2, vec![11]));
		FailingDestinations::set(vec![]);
		// THEN the drain is charged one send per pending destination, both destinations receive
		// only the era 2 set and the queue is empty
		assert_eq!(ValidatorSetAnnouncer::send_announcements(), one_send.saturating_mul(2));
		assert_eq!(Sent::get(), vec![(1, 1, vec![10]), (1, 2, vec![11]), (2, 2, vec![11])]);
		assert_eq!(outgoing(), Vec::<u32>::new());
		assert_ok!(ValidatorSetAnnouncer::do_try_state());
	});
}

#[test]
fn announcement_larger_than_max_validators_is_rejected_and_reported() {
	new_test_ext().execute_with(|| {
		// GIVEN a set of one account more than MaxValidators
		initialize_to_block(1);
		let too_many = (100..151).collect::<Vec<u64>>();
		// WHEN it is announced
		// THEN it fails with TooManyValidators, is reported and nothing is stored or queued
		assert_err!(announce(1, too_many), Error::<Test>::TooManyValidators);
		System::assert_last_event(
			Event::AnnouncementRejected { era: 1, error: Error::<Test>::TooManyValidators.into() }
				.into(),
		);
		assert_eq!(ValidatorSet::<Test>::get(), None);
		assert_eq!(outgoing(), Vec::<u32>::new());
		assert_ok!(ValidatorSetAnnouncer::do_try_state());
	});
}

#[test]
fn try_state_rejects_an_announcement_queued_for_an_unknown_destination() {
	new_test_ext().execute_with(|| {
		// GIVEN a stored set
		assert_ok!(announce(1, vec![10]));
		// WHEN an unknown destination is queued
		OutgoingAnnouncements::<Test>::mutate(|outgoing| outgoing[0] = 3);
		// THEN try_state fails
		assert!(ValidatorSetAnnouncer::do_try_state().is_err());
	});
}
