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

use alloc::collections::btree_map::BTreeMap;
use frame_support::assert_ok;
use polkadot_primitives::{
	BlockNumber, SchedulerParams, SessionIndex, ValidationCode, ValidatorId,
};
use sp_keyring::Sr25519Keyring;

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
			Scheduler::pre_new_session(std::iter::empty());

			Paras::initializer_on_new_session(&notification_with_session_index);
			Scheduler::initializer_on_new_session(&notification_with_session_index);

			Scheduler::post_new_session(0);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Paras::initializer_initialize(b + 1);
		Scheduler::initializer_initialize(b + 1);
		Scheduler::advance_claim_queue(&Default::default());
	}
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

/// Internal access to assignments at the top of the claim queue.
fn scheduled_assignments() -> impl Iterator<Item = (CoreIndex, Assignment)> {
	let claim_queue = ClaimQueue::<Test>::get();
	claim_queue
		.into_iter()
		.filter_map(|(core_idx, v)| v.front().map(|a| (core_idx, a.clone())))
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
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);
	let para_c = ParaId::from(5_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);
	let assignment_c = Assignment::Bulk(para_c);

	new_test_ext(genesis_config).execute_with(|| {
		MockAssigner::set_core_count(2);

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
			assert_eq!(Scheduler::claim_queue_len(), 5);
			let mut claim_queue = scheduler::ClaimQueue::<Test>::get();

			// Because the claim queue used to be empty, the first assignment is doubled so that the
			// first para gets a fair shot at backing something.
			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[assignment_a.clone(), assignment_a, assignment_b]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[assignment_c.clone(), assignment_c].into_iter().collect::<VecDeque<_>>()
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

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);
	let para_c = ParaId::from(3_u32);
	let para_d = ParaId::from(4_u32);
	let para_e = ParaId::from(5_u32);

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

		assert_eq!(Scheduler::eligible_paras(&Default::default()).collect::<Vec<_>>().len(), 2);

		{
			// cores 0, 1 are occupied
			// core 2 is free
			assert!(Scheduler::eligible_paras(&[CoreIndex(0), CoreIndex(1)].into_iter().collect())
				.collect::<Vec<_>>()
				.is_empty());

			Scheduler::advance_claim_queue(&Default::default());

			// Queues of all cores should be empty
			assert_eq!(Scheduler::claim_queue_len(), 0);
		}

		MockAssigner::add_test_assignment(assignment_a.clone());
		MockAssigner::add_test_assignment(assignment_c.clone());
		MockAssigner::add_test_assignment(assignment_b.clone());
		MockAssigner::add_test_assignment(assignment_d.clone());
		MockAssigner::add_test_assignment(assignment_e.clone());
		now = 3;
		run_to_block(now, |_| None);

		{
			let scheduled: BTreeMap<_, _> = scheduled_assignments().collect();

			assert_eq!(scheduled.len(), 3);
			assert_eq!(scheduled.get(&CoreIndex(2)).unwrap(), &Assignment::Bulk(para_b));
		}

		// now note that cores 0 and 1 were freed.
		Scheduler::advance_claim_queue(&std::iter::once(CoreIndex(2)).collect());

		{
			let scheduled: BTreeMap<_, _> = scheduled_assignments().collect();

			// 1 thing scheduled before, + 2 cores freed.
			assert_eq!(scheduled.len(), 3);
			assert_eq!(scheduled.get(&CoreIndex(0)).unwrap(), &Assignment::Bulk(para_d));
			assert_eq!(scheduled.get(&CoreIndex(1)).unwrap(), &Assignment::Bulk(para_e));
			assert_eq!(scheduled.get(&CoreIndex(2)).unwrap(), &Assignment::Bulk(para_b));
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

		let mut now = 2;
		run_to_block(now, |_| None);

		let assert_groups_rotated = |rotations: u32, now: &BlockNumberFor<Test>| {
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
		run_to_block(now, |_| None);

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

		MockAssigner::add_test_assignment(Assignment::Bulk(para_a));

		run_to_block(2, |_| None);

		{
			// Two assignments for A on core 0, because the claim queue used to be empty.
			assert_eq!(Scheduler::claim_queue_len(), 2);

			assert!(Scheduler::next_up_on_available(CoreIndex(1)).is_none());

			assert_eq!(
				Scheduler::next_up_on_available(CoreIndex(0)).unwrap(),
				ScheduledCore { para_id: para_a, collator: None }
			);

			Scheduler::advance_claim_queue(&Default::default());
			assert_eq!(
				Scheduler::next_up_on_available(CoreIndex(0)).unwrap(),
				ScheduledCore { para_id: para_a, collator: None }
			);

			Scheduler::advance_claim_queue(&Default::default());
			assert!(Scheduler::next_up_on_available(CoreIndex(0)).is_none());
		}
	});
}
