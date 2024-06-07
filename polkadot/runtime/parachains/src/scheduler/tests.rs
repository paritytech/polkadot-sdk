// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;

use frame_support::assert_ok;
use polkadot_primitives::{
	vstaging::SchedulerParams, BlockNumber, SessionIndex, ValidationCode, ValidatorId,
};
use sp_keyring::Sr25519Keyring;
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use crate::{
	configuration::HostConfiguration,
	initializer::SessionChangeNotification,
	mock::{
		new_test_ext, MockAssigner, MockGenesisConfig, Paras, ParasShared, RuntimeOrigin,
		Scheduler, System, Test,
	},
	paras::{ParaGenesisArgs, ParaKind},
	scheduler::{self, common::Assignment, ClaimQueue},
};

fn schedule_blank_para(id: ParaId) {
	let validation_code: ValidationCode = vec![1, 2, 3].into();
	assert_ok!(Paras::schedule_para_initialize(
		id,
		ParaGenesisArgs {
			genesis_head: Vec::new().into(),
			validation_code: validation_code.clone(),
			para_kind: ParaKind::Parathread, // This most closely mimics our test assigner
		}
	));

	assert_ok!(Paras::add_trusted_validation_code(RuntimeOrigin::root(), validation_code));
}

fn run_to_block(
	to: BlockNumber,
	new_session: impl Fn(BlockNumber) -> Option<SessionChangeNotification<BlockNumber>>,
) {
	while System::block_number() < to {
		let b = System::block_number();

		Scheduler::initializer_finalize();
		Paras::initializer_finalize(b);

		if let Some(notification) = new_session(b + 1) {
			let mut notification_with_session_index = notification;
			// We will make every session change trigger an action queue. Normally this may require
			// 2 or more session changes.
			if notification_with_session_index.session_index == SessionIndex::default() {
				notification_with_session_index.session_index = ParasShared::scheduled_session();
			}
			Scheduler::pre_new_session();

			Paras::initializer_on_new_session(&notification_with_session_index);
			Scheduler::initializer_on_new_session(&notification_with_session_index);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Paras::initializer_initialize(b + 1);
		Scheduler::initializer_initialize(b + 1);

		// In the real runtime this is expected to be called by the `InclusionInherent` pallet.
		Scheduler::free_cores_and_fill_claim_queue(BTreeMap::new(), b + 1);
	}
}

fn run_to_end_of_block(
	to: BlockNumber,
	new_session: impl Fn(BlockNumber) -> Option<SessionChangeNotification<BlockNumber>>,
) {
	run_to_block(to, &new_session);

	Scheduler::initializer_finalize();
	Paras::initializer_finalize(to);

	if let Some(notification) = new_session(to + 1) {
		Scheduler::pre_new_session();

		Paras::initializer_on_new_session(&notification);
		Scheduler::initializer_on_new_session(&notification);
	}

	System::on_finalize(to);
}

fn default_config() -> HostConfiguration<BlockNumber> {
	HostConfiguration {
		// This field does not affect anything that scheduler does. However, `HostConfiguration`
		// is still a subject to consistency test. It requires that
		// `minimum_validation_upgrade_delay` is greater than `chain_availability_period` and
		// `thread_availability_period`.
		minimum_validation_upgrade_delay: 6,
		scheduler_params: SchedulerParams {
			group_rotation_frequency: 10,
			paras_availability_period: 3,
			lookahead: 2,
			num_cores: 3,
			max_availability_timeouts: 1,
			..Default::default()
		},
		..Default::default()
	}
}

fn genesis_config(config: &HostConfiguration<BlockNumber>) -> MockGenesisConfig {
	MockGenesisConfig {
		configuration: crate::configuration::GenesisConfig { config: config.clone() },
		..Default::default()
	}
}

fn claimqueue_contains_para_ids<T: Config>(pids: Vec<ParaId>) -> bool {
	let set: BTreeSet<ParaId> = ClaimQueue::<T>::get()
		.into_iter()
		.flat_map(|(_, paras_entries)| paras_entries.into_iter().map(|pe| pe.assignment.para_id()))
		.collect();

	pids.into_iter().all(|pid| set.contains(&pid))
}

fn availability_cores_contains_para_ids<T: Config>(pids: Vec<ParaId>) -> bool {
	let set: BTreeSet<ParaId> = AvailabilityCores::<T>::get()
		.into_iter()
		.filter_map(|core| match core {
			CoreOccupied::Free => None,
			CoreOccupied::Paras(entry) => Some(entry.para_id()),
		})
		.collect();

	pids.into_iter().all(|pid| set.contains(&pid))
}

/// Internal access to entries at the top of the claim queue.
fn scheduled_entries() -> impl Iterator<Item = (CoreIndex, ParasEntry<BlockNumberFor<Test>>)> {
	let claimqueue = ClaimQueue::<Test>::get();
	claimqueue
		.into_iter()
		.filter_map(|(core_idx, v)| v.front().map(|e| (core_idx, e.clone())))
}

#[test]
fn claim_queue_iterator_handles_holes_correctly() {
	let mut queue = BTreeMap::new();
	queue.insert(CoreIndex(1), ["abc"].into_iter().collect());
	queue.insert(CoreIndex(4), ["cde"].into_iter().collect());
	let queue = queue.into_iter().peekable();
	let mut i = ClaimQueueIterator { next_idx: 0, queue };

	let (idx, e) = i.next().unwrap();
	assert_eq!(idx, CoreIndex(0));
	assert!(e.is_empty());

	let (idx, e) = i.next().unwrap();
	assert_eq!(idx, CoreIndex(1));
	assert!(e.len() == 1);

	let (idx, e) = i.next().unwrap();
	assert_eq!(idx, CoreIndex(2));
	assert!(e.is_empty());

	let (idx, e) = i.next().unwrap();
	assert_eq!(idx, CoreIndex(3));
	assert!(e.is_empty());

	let (idx, e) = i.next().unwrap();
	assert_eq!(idx, CoreIndex(4));
	assert!(e.len() == 1);

	assert!(i.next().is_none());
}

#[test]
fn claimqueue_ttl_drop_fn_works() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	let genesis_config = genesis_config(&config);

	let para_id = ParaId::from(100);
	let core_idx = CoreIndex::from(0);
	let mut now = 10;

	new_test_ext(genesis_config).execute_with(|| {
		assert!(config.scheduler_params.ttl == 5);
		// Register and run to a blockheight where the para is in a valid state.
		schedule_blank_para(para_id);
		run_to_block(now, |n| if n == now { Some(Default::default()) } else { None });

		// Add a claim on core 0 with a ttl in the past.
		let paras_entry = ParasEntry::new(Assignment::Bulk(para_id), now - 5 as u32);
		Scheduler::add_to_claim_queue(core_idx, paras_entry.clone());

		// Claim is in queue prior to call.
		assert!(claimqueue_contains_para_ids::<Test>(vec![para_id]));

		// Claim is dropped post call.
		Scheduler::drop_expired_claims_from_claim_queue();
		assert!(!claimqueue_contains_para_ids::<Test>(vec![para_id]));

		// Add a claim on core 0 with a ttl in the future (15).
		let paras_entry = ParasEntry::new(Assignment::Bulk(para_id), now + 5);
		Scheduler::add_to_claim_queue(core_idx, paras_entry.clone());

		// Claim is in queue post call.
		Scheduler::drop_expired_claims_from_claim_queue();
		assert!(claimqueue_contains_para_ids::<Test>(vec![para_id]));

		now = now + 6;
		run_to_block(now, |_| None);

		// Claim is dropped
		Scheduler::drop_expired_claims_from_claim_queue();
		assert!(!claimqueue_contains_para_ids::<Test>(vec![para_id]));

		// Add a claim on core 0 with a ttl == now (16)
		let paras_entry = ParasEntry::new(Assignment::Bulk(para_id), now);
		Scheduler::add_to_claim_queue(core_idx, paras_entry.clone());

		// Claim is in queue post call.
		Scheduler::drop_expired_claims_from_claim_queue();
		assert!(claimqueue_contains_para_ids::<Test>(vec![para_id]));

		now = now + 1;
		run_to_block(now, |_| None);

		// Drop expired claim.
		Scheduler::drop_expired_claims_from_claim_queue();
		assert!(!claimqueue_contains_para_ids::<Test>(vec![para_id]));

		// Add a claim on core 0 with a ttl == now (17)
		let paras_entry_non_expired = ParasEntry::new(Assignment::Bulk(para_id), now);
		let paras_entry_expired = ParasEntry::new(Assignment::Bulk(para_id), now - 2);
		// ttls = [17, 15, 17]
		Scheduler::add_to_claim_queue(core_idx, paras_entry_non_expired.clone());
		Scheduler::add_to_claim_queue(core_idx, paras_entry_expired.clone());
		Scheduler::add_to_claim_queue(core_idx, paras_entry_non_expired.clone());
		let cq = scheduler::ClaimQueue::<Test>::get();
		assert_eq!(cq.get(&core_idx).unwrap().len(), 3);

		// Add a claim to the test assignment provider.
		let assignment = Assignment::Bulk(para_id);

		MockAssigner::add_test_assignment(assignment.clone());

		// Drop expired claim.
		Scheduler::drop_expired_claims_from_claim_queue();

		let cq = scheduler::ClaimQueue::<Test>::get();
		let cqc = cq.get(&core_idx).unwrap();
		// Same number of claims, because a new claim is popped from `MockAssigner` instead of the
		// expired one
		assert_eq!(cqc.len(), 3);

		// The first 2 claims in the queue should have a ttl of 17,
		// being the ones set up prior in this test as claims 1 and 3.
		// The third claim is popped from the assignment provider and
		// has a new ttl set by the scheduler of now +
		// assignment_provider_ttl. ttls = [17, 17, 22]
		assert!(cqc.iter().enumerate().all(|(index, entry)| {
			match index {
				0 | 1 => entry.clone().ttl == 17,
				2 => entry.clone().ttl == 22,
				_ => false,
			}
		}))
	});
}

#[test]
fn session_change_shuffles_validators() {
	let genesis_config = genesis_config(&default_config());

	new_test_ext(genesis_config).execute_with(|| {
		// Need five cores for this test
		MockAssigner::set_core_count(5);
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
					ValidatorId::from(Sr25519Keyring::Dave.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
					ValidatorId::from(Sr25519Keyring::Ferdie.public()),
					ValidatorId::from(Sr25519Keyring::One.public()),
				],
				random_seed: [99; 32],
				..Default::default()
			}),
			_ => None,
		});

		let groups = ValidatorGroups::<Test>::get();
		assert_eq!(groups.len(), 5);

		// first two groups have the overflow.
		for i in 0..2 {
			assert_eq!(groups[i].len(), 2);
		}

		for i in 2..5 {
			assert_eq!(groups[i].len(), 1);
		}
	});
}

#[test]
fn session_change_takes_only_max_per_core() {
	let config = {
		let mut config = default_config();
		config.scheduler_params.max_validators_per_core = Some(1);
		config
	};

	let genesis_config = genesis_config(&config);

	new_test_ext(genesis_config).execute_with(|| {
		// Simulate 2 cores between all usage types
		MockAssigner::set_core_count(2);

		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
					ValidatorId::from(Sr25519Keyring::Dave.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
					ValidatorId::from(Sr25519Keyring::Ferdie.public()),
					ValidatorId::from(Sr25519Keyring::One.public()),
				],
				random_seed: [99; 32],
				..Default::default()
			}),
			_ => None,
		});

		let groups = ValidatorGroups::<Test>::get();
		assert_eq!(groups.len(), 7);

		// Every validator gets its own group, even though there are 2 cores.
		for i in 0..7 {
			assert_eq!(groups[i].len(), 1);
		}
	});
}

#[test]
fn fill_claimqueue_fills() {
	let config = default_config();
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);
	let para_c = ParaId::from(5_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);
	let assignment_c = Assignment::Bulk(para_c);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(2);
		let coretime_ttl = config.scheduler_params.ttl;

		// Add 3 paras
		schedule_blank_para(para_a);
		schedule_blank_para(para_b);
		schedule_blank_para(para_c);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		// add some para assignments.
		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());
		MockAssigner::add_test_assignment(assignment_c.clone());

		run_to_block(2, |_| None);

		{
			assert_eq!(Scheduler::claim_queue_len(), 3);
			let scheduled: BTreeMap<_, _> = scheduled_entries().collect();

			// Was added a block later, note the TTL.
			assert_eq!(
				scheduled.get(&CoreIndex(0)).unwrap(),
				&ParasEntry {
					assignment: assignment_a.clone(),
					availability_timeouts: 0,
					ttl: 2 + coretime_ttl
				},
			);
			// Sits on the same core as `para_a`
			assert_eq!(
				scheduler::ClaimQueue::<Test>::get().get(&CoreIndex(0)).unwrap()[1],
				ParasEntry {
					assignment: assignment_b.clone(),
					availability_timeouts: 0,
					ttl: 2 + coretime_ttl
				}
			);
			assert_eq!(
				scheduled.get(&CoreIndex(1)).unwrap(),
				&ParasEntry {
					assignment: assignment_c.clone(),
					availability_timeouts: 0,
					ttl: 2 + coretime_ttl
				},
			);
		}
	});
}

#[test]
fn schedule_schedules_including_just_freed() {
	let mut config = default_config();
	// NOTE: This test expects on demand cores to each get slotted on to a different core
	// and not fill up the claimqueue of each core first.
	config.scheduler_params.lookahead = 1;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);
	let para_c = ParaId::from(5_u32);
	let para_d = ParaId::from(6_u32);
	let para_e = ParaId::from(7_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);
	let assignment_c = Assignment::Bulk(para_c);
	let assignment_d = Assignment::Bulk(para_d);
	let assignment_e = Assignment::Bulk(para_e);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(3);

		// add 5 paras
		schedule_blank_para(para_a);
		schedule_blank_para(para_b);
		schedule_blank_para(para_c);
		schedule_blank_para(para_d);
		schedule_blank_para(para_e);

		// start a new session to activate, 3 validators for 3 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		// add a couple of para claims now that paras are live
		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_c.clone());

		let mut now = 2;
		run_to_block(now, |_| None);

		assert_eq!(Scheduler::scheduled_paras().collect::<Vec<_>>().len(), 2);

		// cores 0, 1 should be occupied. mark them as such.
		let mut occupied_map: BTreeMap<CoreIndex, ParaId> = BTreeMap::new();
		occupied_map.insert(CoreIndex(0), para_a);
		occupied_map.insert(CoreIndex(1), para_c);
		Scheduler::occupied(occupied_map);

		{
			let cores = AvailabilityCores::<Test>::get();

			// cores 0, 1 are `CoreOccupied::Paras(ParasEntry...)`
			assert!(cores[0] != CoreOccupied::Free);
			assert!(cores[1] != CoreOccupied::Free);

			// core 2 is free
			assert!(cores[2] == CoreOccupied::Free);

			assert!(Scheduler::scheduled_paras().collect::<Vec<_>>().is_empty());

			// All `core_queue`s should be empty
			scheduler::ClaimQueue::<Test>::get()
				.iter()
				.for_each(|(_core_idx, core_queue)| assert_eq!(core_queue.len(), 0))
		}

		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_c.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());
		MockAssigner::add_test_assignment(assignment_d.clone());
		MockAssigner::add_test_assignment(assignment_e.clone());
		now = 3;
		run_to_block(now, |_| None);

		{
			let scheduled: BTreeMap<_, _> = scheduled_entries().collect();

			assert_eq!(scheduled.len(), 3);
			assert_eq!(
				scheduled.get(&CoreIndex(2)).unwrap(),
				&ParasEntry {
					assignment: Assignment::Bulk(para_b),
					availability_timeouts: 0,
					ttl: 8
				},
			);
		}

		// now note that cores 0 and 1 were freed.
		let just_updated: BTreeMap<CoreIndex, FreedReason> = vec![
			(CoreIndex(0), FreedReason::Concluded),
			(CoreIndex(1), FreedReason::TimedOut), // should go back on queue.
		]
		.into_iter()
		.collect();
		Scheduler::free_cores_and_fill_claim_queue(just_updated, now);

		{
			let scheduled: BTreeMap<_, _> = scheduled_entries().collect();

			// 1 thing scheduled before, + 2 cores freed.
			assert_eq!(scheduled.len(), 3);
			assert_eq!(
				scheduled.get(&CoreIndex(0)).unwrap(),
				&ParasEntry {
					// Next entry in queue is `a` again:
					assignment: Assignment::Bulk(para_a),
					availability_timeouts: 0,
					ttl: 8
				},
			);
			// Although C was descheduled, the core `2` was occupied so C goes back to the queue.
			assert_eq!(
				scheduler::ClaimQueue::<Test>::get()[&CoreIndex(1)][1],
				ParasEntry {
					assignment: Assignment::Bulk(para_c),
					// End of the queue should be the pushed back entry:
					availability_timeouts: 1,
					// ttl 1 higher:
					ttl: 9
				},
			);
			assert_eq!(
				scheduled.get(&CoreIndex(1)).unwrap(),
				&ParasEntry {
					assignment: Assignment::Bulk(para_c),
					availability_timeouts: 0,
					ttl: 8
				},
			);
			assert_eq!(
				scheduled.get(&CoreIndex(2)).unwrap(),
				&ParasEntry {
					assignment: Assignment::Bulk(para_b),
					availability_timeouts: 0,
					ttl: 8
				},
			);

			assert!(claimqueue_contains_para_ids::<Test>(vec![para_c]));
			assert!(!availability_cores_contains_para_ids::<Test>(vec![para_a, para_c]));
		}
	});
}

#[test]
fn schedule_clears_availability_cores() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 1;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);
	let para_c = ParaId::from(3_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);
	let assignment_c = Assignment::Bulk(para_c);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(3);

		// register 3 paras
		schedule_blank_para(para_a);
		schedule_blank_para(para_b);
		schedule_blank_para(para_c);

		// Adding assignments then running block to populate claim queue
		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());
		MockAssigner::add_test_assignment(assignment_c.clone());

		// start a new session to activate, 3 validators for 3 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		run_to_block(2, |_| None);

		assert_eq!(scheduler::ClaimQueue::<Test>::get().len(), 3);

		// cores 0, 1, and 2 should be occupied. mark them as such.
		Scheduler::occupied(
			vec![(CoreIndex(0), para_a), (CoreIndex(1), para_b), (CoreIndex(2), para_c)]
				.into_iter()
				.collect(),
		);

		{
			let cores = AvailabilityCores::<Test>::get();

			assert_eq!(cores[0].is_free(), false);
			assert_eq!(cores[1].is_free(), false);
			assert_eq!(cores[2].is_free(), false);

			// All `core_queue`s should be empty
			scheduler::ClaimQueue::<Test>::get()
				.iter()
				.for_each(|(_core_idx, core_queue)| assert!(core_queue.len() == 0))
		}

		// Add more assignments
		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());
		MockAssigner::add_test_assignment(assignment_c.clone());

		run_to_block(3, |_| None);

		// now note that cores 0 and 2 were freed.
		Scheduler::free_cores_and_fill_claim_queue(
			vec![(CoreIndex(0), FreedReason::Concluded), (CoreIndex(2), FreedReason::Concluded)]
				.into_iter()
				.collect::<Vec<_>>(),
			3,
		);

		{
			let claimqueue = ClaimQueue::<Test>::get();
			let claimqueue_0 = claimqueue.get(&CoreIndex(0)).unwrap().clone();
			let claimqueue_2 = claimqueue.get(&CoreIndex(2)).unwrap().clone();
			let entry_ttl = 8;
			assert_eq!(claimqueue_0.len(), 1);
			assert_eq!(claimqueue_2.len(), 1);
			let queue_0_expectation: VecDeque<ParasEntryType<Test>> =
				vec![ParasEntry::new(assignment_a, entry_ttl as u32)].into_iter().collect();
			let queue_2_expectation: VecDeque<ParasEntryType<Test>> =
				vec![ParasEntry::new(assignment_c, entry_ttl as u32)].into_iter().collect();
			assert_eq!(claimqueue_0, queue_0_expectation);
			assert_eq!(claimqueue_2, queue_2_expectation);

			// The freed cores should be `Free` in `AvailabilityCores`.
			let cores = AvailabilityCores::<Test>::get();
			assert!(cores[0].is_free());
			assert!(cores[2].is_free());
		}
	});
}

#[test]
fn schedule_rotates_groups() {
	let config = {
		let mut config = default_config();
		config.scheduler_params.lookahead = 1;
		config
	};

	let rotation_frequency = config.scheduler_params.group_rotation_frequency;
	let on_demand_cores = 2;

	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(on_demand_cores);

		schedule_blank_para(para_a);
		schedule_blank_para(para_b);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		let session_start_block = scheduler::SessionStartBlock::<Test>::get();
		assert_eq!(session_start_block, 1);

		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());

		let mut now = 2;
		run_to_block(now, |_| None);

		let assert_groups_rotated = |rotations: u32, now: &BlockNumberFor<Test>| {
			let scheduled: BTreeMap<_, _> = Scheduler::scheduled_paras().collect();
			assert_eq!(scheduled.len(), 2);
			assert_eq!(
				Scheduler::group_assigned_to_core(CoreIndex(0), *now).unwrap(),
				GroupIndex((0u32 + rotations) % on_demand_cores)
			);
			assert_eq!(
				Scheduler::group_assigned_to_core(CoreIndex(1), *now).unwrap(),
				GroupIndex((1u32 + rotations) % on_demand_cores)
			);
		};

		assert_groups_rotated(0, &now);

		// one block before first rotation.
		now = rotation_frequency;
		run_to_block(rotation_frequency, |_| None);

		assert_groups_rotated(0, &now);

		// first rotation.
		now = now + 1;
		run_to_block(now, |_| None);
		assert_groups_rotated(1, &now);

		// one block before second rotation.
		now = rotation_frequency * 2;
		run_to_block(now, |_| None);
		assert_groups_rotated(1, &now);

		// second rotation.
		now = now + 1;
		run_to_block(now, |_| None);
		assert_groups_rotated(2, &now);
	});
}

#[test]
fn on_demand_claims_are_pruned_after_timing_out() {
	let max_timeouts = 20;
	let mut config = default_config();
	config.scheduler_params.lookahead = 1;
	// Need more timeouts for this test
	config.scheduler_params.max_availability_timeouts = max_timeouts;
	config.scheduler_params.ttl = BlockNumber::from(5u32);
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);

	let assignment_a = Assignment::Bulk(para_a);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(2);
		schedule_blank_para(para_a);

		// #1
		let mut now = 1;
		run_to_block(now, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		MockAssigner::add_test_assignment(assignment_a.clone());

		// #2
		now += 1;
		run_to_block(now, |_| None);
		assert_eq!(scheduler::ClaimQueue::<Test>::get().len(), 1);
		// ParaId a is in the claimqueue.
		assert!(claimqueue_contains_para_ids::<Test>(vec![para_a]));

		Scheduler::occupied(vec![(CoreIndex(0), para_a)].into_iter().collect());
		// ParaId a is no longer in the claimqueue.
		assert!(!claimqueue_contains_para_ids::<Test>(vec![para_a]));
		// It is in availability cores.
		assert!(availability_cores_contains_para_ids::<Test>(vec![para_a]));

		// #3
		now += 1;
		// Run to block #n over the max_retries value.
		// In this case, both validator groups with time out on availability and
		// the assignment will be dropped.
		for n in now..=(now + max_timeouts + 1) {
			// #n
			run_to_block(n, |_| None);
			// Time out on core 0.
			let just_updated: BTreeMap<CoreIndex, FreedReason> = vec![
				(CoreIndex(0), FreedReason::TimedOut), // should go back on queue.
			]
			.into_iter()
			.collect();
			Scheduler::free_cores_and_fill_claim_queue(just_updated, now);

			// ParaId a exists in the claim queue until max_retries is reached.
			if n < max_timeouts + now {
				assert!(claimqueue_contains_para_ids::<Test>(vec![para_a]));
			} else {
				assert!(!claimqueue_contains_para_ids::<Test>(vec![para_a]));
			}

			let core_assignments = Scheduler::scheduled_paras().collect();
			Scheduler::occupied(core_assignments);
		}

		// ParaId a does not exist in the claimqueue/availability_cores after
		// threshold has been reached.
		assert!(!claimqueue_contains_para_ids::<Test>(vec![para_a]));
		assert!(!availability_cores_contains_para_ids::<Test>(vec![para_a]));

		// #25
		now += max_timeouts + 2;

		// Add assignment back to the mix.
		MockAssigner::add_test_assignment(assignment_a.clone());
		run_to_block(now, |_| None);

		assert!(claimqueue_contains_para_ids::<Test>(vec![para_a]));

		// #26
		now += 1;
		// Run to block #n but this time have group 1 conclude the availability.
		for n in now..=(now + max_timeouts + 1) {
			// #n
			run_to_block(n, |_| None);
			// Time out core 0 if group 0 is assigned to it, if group 1 is assigned, conclude.
			let mut just_updated: BTreeMap<CoreIndex, FreedReason> = BTreeMap::new();
			if let Some(group) = Scheduler::group_assigned_to_core(CoreIndex(0), n) {
				match group {
					GroupIndex(0) => {
						just_updated.insert(CoreIndex(0), FreedReason::TimedOut); // should go back on queue.
					},
					GroupIndex(1) => {
						just_updated.insert(CoreIndex(0), FreedReason::Concluded);
					},
					_ => panic!("Should only have 2 groups here"),
				}
			}

			Scheduler::free_cores_and_fill_claim_queue(just_updated, now);

			// ParaId a exists in the claim queue until groups are rotated.
			if n < 31 {
				assert!(claimqueue_contains_para_ids::<Test>(vec![para_a]));
			} else {
				assert!(!claimqueue_contains_para_ids::<Test>(vec![para_a]));
			}

			let core_assignments = Scheduler::scheduled_paras().collect();
			Scheduler::occupied(core_assignments);
		}

		// ParaId a does not exist in the claimqueue/availability_cores after
		// being concluded
		assert!(!claimqueue_contains_para_ids::<Test>(vec![para_a]));
		assert!(!availability_cores_contains_para_ids::<Test>(vec![para_a]));
	});
}

#[test]
fn availability_predicate_works() {
	let genesis_config = genesis_config(&default_config());

	let SchedulerParams { group_rotation_frequency, paras_availability_period, .. } =
		default_config().scheduler_params;

	new_test_ext(genesis_config).execute_with(|| {
		run_to_block(1 + paras_availability_period, |_| None);

		assert!(!Scheduler::availability_timeout_check_required());

		run_to_block(1 + group_rotation_frequency, |_| None);

		{
			let now = System::block_number();
			assert!(Scheduler::availability_timeout_check_required());
			let pred = Scheduler::availability_timeout_predicate();
			let last_rotation = Scheduler::group_rotation_info(now).last_rotation_at();

			let would_be_timed_out = now - paras_availability_period;
			let should_not_be_timed_out = last_rotation;

			assert!(pred(would_be_timed_out).timed_out);
			assert!(!pred(should_not_be_timed_out).timed_out);
			assert!(!pred(now).timed_out);

			// check the threshold is exact.
			assert!(!pred(would_be_timed_out + 1).timed_out);
		}
	});
}

#[test]
fn next_up_on_available_uses_next_scheduled_or_none() {
	let genesis_config = genesis_config(&default_config());

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(1);
		schedule_blank_para(para_a);
		schedule_blank_para(para_b);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		let entry_a = ParasEntry {
			assignment: Assignment::Bulk(para_a),
			availability_timeouts: 0 as u32,
			ttl: 5 as u32,
		};
		let entry_b = ParasEntry {
			assignment: Assignment::Bulk(para_b),
			availability_timeouts: 0 as u32,
			ttl: 5 as u32,
		};

		Scheduler::add_to_claim_queue(CoreIndex(0), entry_a.clone());

		run_to_block(2, |_| None);

		{
			assert_eq!(Scheduler::claim_queue_len(), 1);
			assert_eq!(scheduler::AvailabilityCores::<Test>::get().len(), 1);

			let mut map = BTreeMap::new();
			map.insert(CoreIndex(0), para_a);
			Scheduler::occupied(map);

			let cores = scheduler::AvailabilityCores::<Test>::get();
			match &cores[0] {
				CoreOccupied::Paras(entry) => assert_eq!(entry, &entry_a),
				_ => panic!("There should only be one test assigner core"),
			}

			assert!(Scheduler::next_up_on_available(CoreIndex(0)).is_none());

			Scheduler::add_to_claim_queue(CoreIndex(0), entry_b);

			assert_eq!(
				Scheduler::next_up_on_available(CoreIndex(0)).unwrap(),
				ScheduledCore { para_id: para_b, collator: None }
			);
		}
	});
}

#[test]
fn next_up_on_time_out_reuses_claim_if_nothing_queued() {
	let genesis_config = genesis_config(&default_config());

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(1);
		schedule_blank_para(para_a);
		schedule_blank_para(para_b);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		MockAssigner::add_test_assignment(assignment_a.clone());

		run_to_block(2, |_| None);

		{
			assert_eq!(scheduler::ClaimQueue::<Test>::get().len(), 1);
			assert_eq!(scheduler::AvailabilityCores::<Test>::get().len(), 1);

			let mut map = BTreeMap::new();
			map.insert(CoreIndex(0), para_a);
			Scheduler::occupied(map);

			let cores = scheduler::AvailabilityCores::<Test>::get();
			match cores.get(0).unwrap() {
				CoreOccupied::Paras(entry) => {
					assert_eq!(entry.assignment, assignment_a.clone());
				},
				_ => panic!("There should only be a single test assigner core"),
			}

			// There's nothing more to pop for core 0 from the assignment provider.
			assert!(MockAssigner::pop_assignment_for_core(CoreIndex(0)).is_none());

			assert_eq!(
				Scheduler::next_up_on_time_out(CoreIndex(0)).unwrap(),
				ScheduledCore { para_id: para_a, collator: None }
			);

			MockAssigner::add_test_assignment(assignment_b.clone());

			// Pop assignment_b into the claimqueue
			Scheduler::free_cores_and_fill_claim_queue(BTreeMap::new(), 2);

			//// Now that there is an earlier next-up, we use that.
			assert_eq!(
				Scheduler::next_up_on_available(CoreIndex(0)).unwrap(),
				ScheduledCore { para_id: para_b, collator: None }
			);
		}
	});
}

#[test]
fn session_change_requires_reschedule_dropping_removed_paras() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 1;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);

	new_test_ext(genesis_config).execute_with(|| {
		// Setting explicit core count
		MockAssigner::set_core_count(5);
		let coretime_ttl = configuration::ActiveConfig::<Test>::get().scheduler_params.ttl;

		schedule_blank_para(para_a);
		schedule_blank_para(para_b);

		// Add assignments
		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());

		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
					ValidatorId::from(Sr25519Keyring::Dave.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
					ValidatorId::from(Sr25519Keyring::Ferdie.public()),
					ValidatorId::from(Sr25519Keyring::One.public()),
				],
				random_seed: [99; 32],
				..Default::default()
			}),
			_ => None,
		});

		assert_eq!(scheduler::ClaimQueue::<Test>::get().len(), 2);

		let groups = ValidatorGroups::<Test>::get();
		assert_eq!(groups.len(), 5);

		assert_ok!(Paras::schedule_para_cleanup(para_b));

		// Add assignment
		MockAssigner::add_test_assignment(assignment_a.clone());

		run_to_end_of_block(2, |number| match number {
			2 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
					ValidatorId::from(Sr25519Keyring::Dave.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
					ValidatorId::from(Sr25519Keyring::Ferdie.public()),
					ValidatorId::from(Sr25519Keyring::One.public()),
				],
				random_seed: [99; 32],
				..Default::default()
			}),
			_ => None,
		});

		Scheduler::free_cores_and_fill_claim_queue(BTreeMap::new(), 3);

		assert_eq!(
			scheduler::ClaimQueue::<Test>::get(),
			vec![(
				CoreIndex(0),
				vec![ParasEntry::new(
					Assignment::Bulk(para_a),
					// At end of block 2
					coretime_ttl + 2
				)]
				.into_iter()
				.collect()
			)]
			.into_iter()
			.collect()
		);

		// Add para back
		schedule_blank_para(para_b);

		// Add assignments
		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());

		run_to_block(3, |number| match number {
			3 => Some(SessionChangeNotification {
				new_config: default_config(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
					ValidatorId::from(Sr25519Keyring::Dave.public()),
					ValidatorId::from(Sr25519Keyring::Eve.public()),
					ValidatorId::from(Sr25519Keyring::Ferdie.public()),
					ValidatorId::from(Sr25519Keyring::One.public()),
				],
				random_seed: [99; 32],
				..Default::default()
			}),
			_ => None,
		});

		assert_eq!(scheduler::ClaimQueue::<Test>::get().len(), 2);

		let groups = ValidatorGroups::<Test>::get();
		assert_eq!(groups.len(), 5);

		Scheduler::free_cores_and_fill_claim_queue(BTreeMap::new(), 4);

		assert_eq!(
			scheduler::ClaimQueue::<Test>::get(),
			vec![
				(
					CoreIndex(0),
					vec![ParasEntry::new(
						Assignment::Bulk(para_a),
						// At block 3
						coretime_ttl + 3
					)]
					.into_iter()
					.collect()
				),
				(
					CoreIndex(1),
					vec![ParasEntry::new(
						Assignment::Bulk(para_b),
						// At block 3
						coretime_ttl + 3
					)]
					.into_iter()
					.collect()
				),
			]
			.into_iter()
			.collect()
		);
	});
}
