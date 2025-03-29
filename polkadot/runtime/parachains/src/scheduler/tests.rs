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
use assigner_coretime::PartsOf57600;
use frame_support::assert_ok;
use pallet_broker::CoreAssignment;
use polkadot_primitives::{
	BlockNumber, SchedulerParams, SessionIndex, ValidationCode, ValidatorId,
};
use sp_keyring::Sr25519Keyring;

use crate::{
	configuration::HostConfiguration, initializer::SessionChangeNotification, mock::{
		new_test_ext, Configuration, CoretimeAssigner, MockGenesisConfig, Paras, ParasShared, RuntimeOrigin, Scheduler, System, Test
	}, on_demand, paras::{ParaGenesisArgs, ParaKind}, scheduler::{self}
};

fn register_para(id: ParaId) {
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

		if let Some(mut notification) = new_session(b + 1) {
			// We will make every session change trigger an action queue. Normally this may require
			// 2 or more session changes.
			if notification.session_index == SessionIndex::default() {
				notification.session_index = ParasShared::scheduled_session();
			}

			Configuration::force_set_active_config(notification.new_config.clone());

			Paras::initializer_on_new_session(&notification);

			Scheduler::initializer_on_new_session(&notification);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Paras::initializer_initialize(b + 1);
		Scheduler::initializer_initialize(b + 1);

		Scheduler::advance_claim_queue(|_| false);
	}
}

fn default_config() -> HostConfiguration<BlockNumber> {
	HostConfiguration {
		// This field does not affect anything that scheduler does. However, `HostConfiguration`
		// is still a subject to consistency test. It requires that
		// `minimum_validation_upgrade_delay` is greater than `chain_availability_period` and
		// `thread_availability_period`.
		minimum_validation_upgrade_delay: 6,
		#[allow(deprecated)]
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
fn next_assignments() -> impl Iterator<Item = (CoreIndex, ParaId)> {
	let claim_queue = Scheduler::claim_queue();
	claim_queue
		.into_iter()
		.filter_map(|(core_idx, v)| v.front().map(|a| (core_idx, a.clone())))
}

#[test]
fn session_change_shuffles_validators() {
	let mut config = default_config();
	// Need five cores for this test
	config.scheduler_params.num_cores = 5;
	let genesis_config = genesis_config(&config);

	new_test_ext(genesis_config).execute_with(|| {
		assert!(ValidatorGroups::<Test>::get().is_empty());

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
		// Simulate 2 cores between all usage types
		config.scheduler_params.num_cores = 2;
		config.scheduler_params.max_validators_per_core = Some(1);
		config
	};

	let genesis_config = genesis_config(&config);

	new_test_ext(genesis_config).execute_with(|| {
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
// Test that `advance_claim_queue` doubles the first assignment only for a core that didn't use to
// have any assignments.
fn advance_claim_queue_doubles_assignment_only_if_empty() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	config.scheduler_params.num_cores = 2;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);
	let para_c = ParaId::from(5_u32);

	new_test_ext(genesis_config).execute_with(|| {
		// Add 3 paras
		register_para(para_a);
		register_para(para_b);
		register_para(para_c);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		// add some para assignments.
		CoretimeAssigner::assign_core(CoreIndex(0), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();
		CoretimeAssigner::assign_core(CoreIndex(1), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();

		// This will call advance_claim_queue
		run_to_block(2, |_| None);

		{
			on_demand::Pallet::<Test>::push_back_order(para_a);
			on_demand::Pallet::<Test>::push_back_order(para_c);
			on_demand::Pallet::<Test>::push_back_order(para_b);

			let mut claim_queue = scheduler::Pallet::<Test>::claim_queue();

			// Because the claim queue used to be empty, the first assignment is doubled for every
			// core so that the first para gets a fair shot at backing something.
			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a, para_a, para_b]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_c, para_c].into_iter().collect::<VecDeque<_>>()
			);
		}
	});
}

#[test]
// Test that `advance_claim_queue` doesn't populate for cores which have no assignments.
fn advance_claim_queue_no_entry_if_empty() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	config.scheduler_params.num_cores = 2;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);

	new_test_ext(genesis_config).execute_with(|| {
		// Add 1 para
		register_para(para_a);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		CoretimeAssigner::assign_core(CoreIndex(0), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();
		CoretimeAssigner::assign_core(CoreIndex(1), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();
		on_demand::Pallet::<Test>::push_back_order(para_a);

		// This will call advance_claim_queue
		run_to_block(3, |_| None);

		{
			let mut claim_queue = Scheduler::claim_queue();

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a].into_iter().collect::<VecDeque<_>>()
			);

			// Even though core 1 exists, there's no assignment for it so it's not present in the
			// claim queue.
			assert!(claim_queue.remove(&CoreIndex(1)).is_none());
		}
	});
}

#[test]
// Test that `advance_claim_queue` only advances for cores that are not part of the `except_for`
// set.
fn advance_claim_queue_except_for() {
	let mut config = default_config();
	// NOTE: This test expects on demand cores to each get slotted on to a different core
	// and not fill up the claimqueue of each core first.
	config.scheduler_params.lookahead = 1;
	config.scheduler_params.num_cores = 3;

	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);
	let para_c = ParaId::from(3_u32);
	let para_d = ParaId::from(4_u32);
	let para_e = ParaId::from(5_u32);

	CoretimeAssigner::assign_core(CoreIndex(0), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();

	new_test_ext(genesis_config).execute_with(|| {
		// add 5 paras
		register_para(para_a);
		register_para(para_b);
		register_para(para_c);
		register_para(para_d);
		register_para(para_e);

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

		// add a couple of para claims now that paras are live
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_c);

		run_to_block(2, |_| None);

		Scheduler::advance_claim_queue(|_| false);

		// Queues of all cores should be empty
		assert_eq!(Scheduler::claim_queue_len(), 0);

		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_c);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_d);
		on_demand::Pallet::<Test>::push_back_order(para_e);

		run_to_block(3, |_| None);

		{
			let scheduled: BTreeMap<_, _> = next_assignments().collect();

			assert_eq!(scheduled.len(), 3);
			assert_eq!(scheduled.get(&CoreIndex(0)).unwrap(), para_a);
			assert_eq!(scheduled.get(&CoreIndex(1)).unwrap(), para_c);
			assert_eq!(scheduled.get(&CoreIndex(2)).unwrap(), para_b);
		}

		// now note that cores 0 and 1 were freed.
		Scheduler::advance_claim_queue(|CoreIndex(ix)| ix == 2);

		{
			let scheduled: BTreeMap<_, _> = next_assignments().collect();

			// 1 thing scheduled before, + 2 cores freed.
			assert_eq!(scheduled.len(), 3);
			assert_eq!(scheduled.get(&CoreIndex(0)).unwrap(), para_d);
			assert_eq!(scheduled.get(&CoreIndex(1)).unwrap(), para_e);
			assert_eq!(scheduled.get(&CoreIndex(2)).unwrap(), para_b);
		}
	});
}

#[test]
fn schedule_rotates_groups() {
	let on_demand_cores = 2;
	let config = {
		let mut config = default_config();
		config.scheduler_params.lookahead = 1;
		config.scheduler_params.num_cores = on_demand_cores;
		config
	};

	let rotation_frequency = config.scheduler_params.group_rotation_frequency;

	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);

	new_test_ext(genesis_config).execute_with(|| {
		register_para(para_a);
		register_para(para_b);

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
fn session_change_increasing_number_of_cores() {
	let mut config = default_config();
	config.scheduler_params.num_cores = 2;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);

	new_test_ext(genesis_config).execute_with(|| {
		// Add 2 paras
		register_para(para_a);
		register_para(para_b);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		CoretimeAssigner::assign_core(CoreIndex(0), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();
		CoretimeAssigner::assign_core(CoreIndex(1), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();

		// This will call advance_claim_queue
		run_to_block(2, |_| None);

		{
			on_demand::Pallet::<Test>::push_back_order(para_a);
			on_demand::Pallet::<Test>::push_back_order(para_b);
			assert_eq!(Scheduler::claim_queue_len(), 4);

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a.clone(), para_a.clone()]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_b.clone(), para_b.clone()]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
		}

		// Increase number of cores to 4.
		let old_config = config;
		let mut new_config = old_config.clone();
		new_config.scheduler_params.num_cores = 4;

		// add another assignment for para b.
		on_demand::Pallet::<Test>::push_back_order(para_b);

		CoretimeAssigner::assign_core(CoreIndex(2), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();
		CoretimeAssigner::assign_core(CoreIndex(3), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();

		run_to_block(3, |number| match number {
			3 => Some(SessionChangeNotification {
				new_config: new_config.clone(),
				prev_config: old_config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
					ValidatorId::from(Sr25519Keyring::Charlie.public()),
					ValidatorId::from(Sr25519Keyring::Dave.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		{
			let mut claim_queue = Scheduler::claim_queue();
			assert_eq!(Scheduler::claim_queue_len(), 3);

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a].into_iter().collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_b].into_iter().collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(2)).unwrap(),
				[para_b].into_iter().collect::<VecDeque<_>>()
			);
		}
	});
}

#[test]
fn session_change_decreasing_number_of_cores() {
	let mut config = default_config();
	config.scheduler_params.num_cores = 3;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);

	new_test_ext(genesis_config).execute_with(|| {
		// Add 2 paras
		register_para(para_a);
		register_para(para_b);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		scheduler::Pallet::<Test>::set_claim_queue(BTreeMap::from([
			(CoreIndex::from(0), VecDeque::from([assignment_a.clone()])),
			// Leave a hole for core 1.
			(CoreIndex::from(2), VecDeque::from([assignment_b.clone(), assignment_b.clone()])),
		]));

		// Decrease number of cores to 1.
		let old_config = config;
		let mut new_config = old_config.clone();
		new_config.scheduler_params.num_cores = 1;

		// Session change.
		// Assignment A had its shot already so will be dropped for good.
		// The two assignments of B will be pushed back to the assignment provider.
		run_to_block(3, |number| match number {
			3 => Some(SessionChangeNotification {
				new_config: new_config.clone(),
				prev_config: old_config.clone(),
				validators: vec![ValidatorId::from(Sr25519Keyring::Alice.public())],
				..Default::default()
			}),
			_ => None,
		});

		let mut claim_queue = Scheduler::claim_queue();
		assert_eq!(Scheduler::claim_queue_len(), 1);

		// There's only one assignment for B because run_to_block also calls advance_claim_queue at
		// the end.
		assert_eq!(
			claim_queue.remove(&CoreIndex(0)).unwrap(),
			[para_b.clone()].into_iter().collect::<VecDeque<_>>()
		);

		Scheduler::advance_claim_queue(|_| false);
		// No more assignments now.
		assert_eq!(Scheduler::claim_queue_len(), 0);

		// Retain number of cores to 1 but remove all validator groups. The claim queue length
		// should be the minimum of these two.

		// Add an assignment.
		on_demand::Pallet::<Test>::push_back_order(para_b);

		run_to_block(4, |number| match number {
			4 => Some(SessionChangeNotification {
				new_config: new_config.clone(),
				prev_config: new_config.clone(),
				validators: vec![],
				..Default::default()
			}),
			_ => None,
		});

		assert_eq!(Scheduler::claim_queue_len(), 0);
	});
}

#[test]
fn session_change_increasing_lookahead() {
	let mut config = default_config();
	config.scheduler_params.num_cores = 2;
	config.scheduler_params.lookahead = 2;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);

	let assignment_a = Assignment::Bulk(para_a);
	let assignment_b = Assignment::Bulk(para_b);

	new_test_ext(genesis_config).execute_with(|| {
		// Add 2 paras
		register_para(para_a);
		register_para(para_b);

		// start a new session to activate, 2 validators for 2 cores.
		run_to_block(1, |number| match number {
			1 => Some(SessionChangeNotification {
				new_config: config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		CoretimeAssigner::assign_core(CoreIndex(0), 0, vec![(CoreAssignment::Pool, PartsOf57600::FULL)], None).unwrap();
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_b);

		// Lookahead is currently 2.

		run_to_block(2, |_| None);

		{
			let mut claim_queue = Scheduler::claim_queue();
			assert_eq!(Scheduler::claim_queue_len(), 4);

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a, para_a]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_a, para_a]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
		}

		// Increase lookahead to 4.
		let old_config = config;
		let mut new_config = old_config.clone();
		new_config.scheduler_params.lookahead = 4;

		run_to_block(3, |number| match number {
			3 => Some(SessionChangeNotification {
				new_config: new_config.clone(),
				prev_config: old_config.clone(),
				validators: vec![
					ValidatorId::from(Sr25519Keyring::Alice.public()),
					ValidatorId::from(Sr25519Keyring::Bob.public()),
				],
				..Default::default()
			}),
			_ => None,
		});

		{
			let mut claim_queue = Scheduler::claim_queue();
			assert_eq!(Scheduler::claim_queue_len(), 6);

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a, para_a, para_b]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_a, para_b, para_b]
					.into_iter()
					.collect::<VecDeque<_>>()
			);
		}
	});
}
