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

use std::collections::HashSet;

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
	configuration::HostConfiguration,
	initializer::SessionChangeNotification,
	mock::{
		new_test_ext, Configuration, MockGenesisConfig, Paras, ParasShared, RuntimeOrigin,
		Scheduler, System, Test,
	},
	on_demand,
	paras::{ParaGenesisArgs, ParaKind},
	scheduler::{self},
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
		.filter_map(|(core_idx, v)| v.front().map(|a| (core_idx, *a)))
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
fn assignments_stay_stable_when_pushed_back() {
	// This test verifies that when assignments are blocked and pushed back to on-demand,
	// the existing assignments in the claim queue remain stable (unchanged).
	// Only new assignments should appear at the end of the queue.
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	config.scheduler_params.num_cores = 2;
	let genesis_config = genesis_config(&config);

	let paras = (3..9).map(ParaIs the build finished already?Id::from);

	new_test_ext(genesis_config).execute_with(|| {
		// Register paras
		for para in paras.clone() {
    		register_para(para);
		}

		// Start a new session
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

		// Assign both cores to Pool (on-demand)
		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(1),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();

		// Add plenty of orders to fill the claim queue
		for para in paras.clone() {
    		on_demand::Pallet::<Test>::push_back_order(para);
		}

		run_to_block(2, |_| None);

		let mut remaining_assignments: HashSet<_> = paras.clone().collect();

		// Take a snapshot of the initial claim queue
		let claim_queue_initial = scheduler::Pallet::<Test>::claim_queue();
		let core_0_initial = claim_queue_initial.get(&CoreIndex(0)).cloned().unwrap();
		let core_1_initial = claim_queue_initial.get(&CoreIndex(1)).cloned().unwrap();

		assert!(!core_0_initial.is_empty(), "Core 0 should have assignments in the claim queue");
		assert!(!core_1_initial.is_empty(), "Core 1 should have assignments in the claim queue");

		// Record the assignments for verification
		let core_0_rest: Vec<_> = core_0_initial.iter().skip(1).copied().collect();
		let core_1_rest: Vec<_> = core_1_initial.iter().skip(1).copied().collect();

		// Set block number for the next advance
		System::set_block_number(3);

		// Now advance with core 0 blocked - this should push back para to on-demand
		let popped = scheduler::Pallet::<Test>::advance_claim_queue(|core_idx| {
			core_idx == CoreIndex(0) // Block core 0
		});

		for p in popped.values() {
    		remaining_assignments.remove(p);
		}

		// Core 0 should not have been popped (it was blocked)
		assert_eq!(
			popped.get(&CoreIndex(0)),
			None,
			"Core 0 was blocked, so nothing should be popped"
		);

		// Core 1 should have been popped normally
		assert!(popped.get(&CoreIndex(1)).is_some(), "Core 1 should have been popped");

		// Now check the claim queue after the advance with blocking
		let claim_queue_after_block = scheduler::Pallet::<Test>::claim_queue();
		let core_0_after_block = claim_queue_after_block.get(&CoreIndex(0)).cloned().unwrap();
		let core_1_after_block = claim_queue_after_block.get(&CoreIndex(1)).cloned().unwrap();

		// The claim queue advanced for ALL cores (including blocked ones)
		// Core 0's first assignment was popped and pushed back to on-demand
		// So what was at position 1 should now be at position 0
		let core_0_after_vec: Vec<_> = core_0_after_block.iter().copied().collect();
		assert_eq!(
			&core_0_after_vec[..core_0_rest.len()],
			&core_0_rest[..],
			"Core 0: After advancing, assignments should match the original queue shifted by 1 (what was at positions [1..] is now at [0..])"
		);

		// Core 1 should have advanced normally: first element removed, rest shifted up
		let core_1_after_prefix: Vec<_> =
			core_1_after_block.iter().take(core_1_rest.len()).copied().collect();
		assert_eq!(
			core_1_after_prefix, core_1_rest,
			"Core 1: Should have advanced normally with assignments shifted"
		);

		// Get a snapshot of what the claim queue looks like now (before we start advancing)
		let mut claim_queue_snapshot = scheduler::Pallet::<Test>::claim_queue();

		// Advance several times and verify each popped assignment matches what we expected
		loop {
			System::set_block_number(System::block_number() + 1);
			let popped = scheduler::Pallet::<Test>::advance_claim_queue(|_| false);

			if popped.is_empty() {
    			break
			}

			for (core_idx, para) in popped.into_iter() {
          		remaining_assignments.remove(&para);
                if let Some(queue) = claim_queue_snapshot.get_mut(&core_idx) {
                    assert_eq!(queue.pop_front(), Some(para), "Advance assignments is meant to match claim queue prediction.");
                }
            }
		}
		assert!(remaining_assignments.is_empty(), "All items should have been served still.")
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
		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(1),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();

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
				[para_a, para_a, para_b].into_iter().collect::<VecDeque<_>>()
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

		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(1),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		on_demand::Pallet::<Test>::push_back_order(para_a);

		// This will call advance_claim_queue. With duplication, a single order gets consumed
		// over 2 blocks, so check at block 2 before it's fully consumed.
		run_to_block(2, |_| None);

		{
			let mut claim_queue = Scheduler::claim_queue();

			// Core 0 should have para_a in its claim queue (may be duplicated for async backing)
			let core_0_queue =
				claim_queue.remove(&CoreIndex(0)).expect("Core 0 should have entries");
			assert!(core_0_queue.contains(&para_a), "Core 0 should have para_a assigned");

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
	config.scheduler_params.lookahead = 3;
	config.scheduler_params.num_cores = 3;

	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(1_u32);
	let para_b = ParaId::from(2_u32);
	let para_c = ParaId::from(3_u32);
	let para_d = ParaId::from(4_u32);
	let para_e = ParaId::from(5_u32);

	new_test_ext(genesis_config).execute_with(|| {
		// add 5 paras
		register_para(para_a);
		register_para(para_b);
		register_para(para_c);
		register_para(para_d);
		register_para(para_e);

		for core in 0..3 {
			Pallet::<Test>::assign_core(
				CoreIndex(core),
				0,
				vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
				None,
			)
			.unwrap();
		}

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
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_c);

		// Duplication of first entries (claim queue was empty):
		assert_eq!(Scheduler::claim_queue_len(), 6);

		run_to_block(2, |_| None);

		// First assignment should have been consumed.
		assert_eq!(Scheduler::claim_queue_len(), 3);

		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_d);
		on_demand::Pallet::<Test>::push_back_order(para_e);

		run_to_block(3, |_| None);

		{
			let scheduled: BTreeMap<_, _> = next_assignments().collect();

			assert_eq!(scheduled.len(), 3);
			assert_eq!(*scheduled.get(&CoreIndex(0)).unwrap(), para_b);
			assert_eq!(*scheduled.get(&CoreIndex(1)).unwrap(), para_d);
			assert_eq!(*scheduled.get(&CoreIndex(2)).unwrap(), para_e);
		}

		// now note that cores 0 and 1 were freed.
		System::set_block_number(4);
		Scheduler::advance_claim_queue(|CoreIndex(ix)| ix == 2);

		{
			let scheduled: BTreeMap<_, _> = next_assignments().collect();

			assert_eq!(scheduled.len(), 1);
			assert_eq!(*scheduled.get(&CoreIndex(0)).unwrap(), para_e);
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

		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(1),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();

		// This will call advance_claim_queue
		run_to_block(2, |_| None);

		{
			on_demand::Pallet::<Test>::push_back_order(para_a);
			on_demand::Pallet::<Test>::push_back_order(para_b);
			assert_eq!(Scheduler::claim_queue_len(), 4);
			let mut claim_queue = Scheduler::claim_queue();

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a, para_a].into_iter().collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_b, para_b].into_iter().collect::<VecDeque<_>>()
			);
		}

		// Increase number of cores to 4.
		let old_config = config;
		let mut new_config = old_config.clone();
		new_config.scheduler_params.num_cores = 4;

		// add another assignment for para b.
		on_demand::Pallet::<Test>::push_back_order(para_b);

		Pallet::<Test>::assign_core(
			CoreIndex(2),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(3),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();

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
				[para_a, para_b].into_iter().collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
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

		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		// Leave a hole for core 1.
		Pallet::<Test>::assign_core(
			CoreIndex(2),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_b);

		// Decrease number of cores to 1.
		let old_config = config;
		let mut new_config = old_config.clone();
		new_config.scheduler_params.num_cores = 1;

		// Session change.
		// First two assignments had their shot already.
		// The two next assignments will be pushed back to the assignment provider.
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
			[para_a].into_iter().collect::<VecDeque<_>>()
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

		for core in 0..2 {
			Pallet::<Test>::assign_core(
				CoreIndex(core),
				0,
				vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
				None,
			)
			.unwrap();
		}
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_b);

		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_a);

		// Lookahead is currently 2.

		run_to_block(2, |_| None);

		{
			let mut claim_queue = Scheduler::claim_queue();
			assert_eq!(Scheduler::claim_queue_len(), 4);

			assert_eq!(
				claim_queue.remove(&CoreIndex(0)).unwrap(),
				[para_a, para_a].into_iter().collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_b, para_b].into_iter().collect::<VecDeque<_>>()
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
				[para_a, para_a, para_a].into_iter().collect::<VecDeque<_>>()
			);
			assert_eq!(
				claim_queue.remove(&CoreIndex(1)).unwrap(),
				[para_b, para_b, para_b].into_iter().collect::<VecDeque<_>>()
			);
		}
	});
}

#[test]
fn peek_claim_queue_predicts_scheduling() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	config.scheduler_params.num_cores = 2;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);
	let para_c = ParaId::from(5_u32);

	new_test_ext(genesis_config).execute_with(|| {
		// Register paras
		register_para(para_a);
		register_para(para_b);
		register_para(para_c);

		// Start a new session
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

		// Assign cores
		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(1),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();

		// Add plenty of orders to fill the claim queue
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_c);
		on_demand::Pallet::<Test>::push_back_order(para_a);
		on_demand::Pallet::<Test>::push_back_order(para_b);
		on_demand::Pallet::<Test>::push_back_order(para_c);

		run_to_block(2, |_| None);

		// Set block number to 3 for the next advance
		System::set_block_number(3);

		// Peek at claim queue - should not modify state
		let peeked_first = scheduler::Pallet::<Test>::claim_queue();
		let peeked_second = scheduler::Pallet::<Test>::claim_queue();

		// Multiple peeks should return identical results
		assert_eq!(peeked_first, peeked_second, "Peek modified the claim queue state!");

		// Record what we peeked
		let core_0_peek = peeked_first.get(&CoreIndex(0)).cloned().unwrap();
		let core_1_peek = peeked_first.get(&CoreIndex(1)).cloned().unwrap();

		// Now advance the claim queue (simulate what happens during block processing)
		let popped = scheduler::Pallet::<Test>::advance_claim_queue(|_| false);

		// Verify what was popped matches the first element of what we peeked
		assert_eq!(
			popped.get(&CoreIndex(0)).copied(),
			core_0_peek.front().copied(),
			"Core 0: Popped assignment doesn't match first peeked entry"
		);
		assert_eq!(
			popped.get(&CoreIndex(1)).copied(),
			core_1_peek.front().copied(),
			"Core 1: Popped assignment doesn't match first peeked entry"
		);

		// After advancing, peek again to see what the next assignments would be
		let claim_queue_after_pop = scheduler::Pallet::<Test>::claim_queue();

		// The claim queue after pop should have:
		// - The first element removed (what was popped)
		// - Potentially new elements added at the end (to maintain lookahead)
		// So we verify that the beginning of the new queue matches the tail of the peeked queue
		let core_0_after = claim_queue_after_pop.get(&CoreIndex(0)).cloned().unwrap();
		let core_1_after = claim_queue_after_pop.get(&CoreIndex(1)).cloned().unwrap();

		// Check that after popping, the first elements match what was at position 1 in the peek
		assert_eq!(
			core_0_after.front().copied(),
			core_0_peek.get(1).copied(),
			"Core 0: First element after pop should match second element of peek"
		);
		assert_eq!(
			core_1_after.front().copied(),
			core_1_peek.get(1).copied(),
			"Core 1: First element after pop should match second element of peek"
		);
	});
}

#[test]
fn on_demand_order_on_empty_core_appears_in_next_two_blocks() {
	let mut config = default_config();
	config.scheduler_params.lookahead = 3;
	config.scheduler_params.num_cores = 2;
	let genesis_config = genesis_config(&config);

	let para_a = ParaId::from(3_u32);
	let para_b = ParaId::from(4_u32);

	new_test_ext(genesis_config).execute_with(|| {
		// Register paras
		register_para(para_a);
		register_para(para_b);

		// Start a new session
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

		// Assign both cores to Pool (on-demand)
		Pallet::<Test>::assign_core(
			CoreIndex(0),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();
		Pallet::<Test>::assign_core(
			CoreIndex(1),
			0,
			vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
			None,
		)
		.unwrap();

		run_to_block(2, |_| None);

		// At this point, both cores should have empty claim queues
		let claim_queue_initial = scheduler::Pallet::<Test>::claim_queue();
		assert!(
			claim_queue_initial.get(&CoreIndex(0)).map_or(true, |q| q.is_empty()),
			"Core 0 should start with empty claim queue"
		);
		assert!(
			claim_queue_initial.get(&CoreIndex(1)).map_or(true, |q| q.is_empty()),
			"Core 1 should start with empty claim queue"
		);

		// Now add an on-demand order for para_a
		on_demand::Pallet::<Test>::push_back_order(para_a);

		// Check the claim queue BEFORE advancing to see what will be scheduled
		let claim_queue_before = scheduler::Pallet::<Test>::claim_queue();
		let core_0_before = claim_queue_before.get(&CoreIndex(0)).cloned().unwrap();

		// The on-demand order on an empty core should be duplicated
		assert_eq!(core_0_before.len(), 2, "On-demand order on empty core should be duplicated");
		assert_eq!(core_0_before.front(), Some(&para_a), "First position should be para_a");
		assert_eq!(
			core_0_before.get(1),
			Some(&para_a),
			"Second position should also be para_a (duplicated)"
		);

		// Set block number for the next advance
		System::set_block_number(3);

		// Now advance to pop the first one
		scheduler::Pallet::<Test>::advance_claim_queue(|_| false);

		// After advancing, we should have 1 para_a left (the duplicate)
		let claim_queue_after_first_advance = scheduler::Pallet::<Test>::claim_queue();
		let core_0_after_first =
			claim_queue_after_first_advance.get(&CoreIndex(0)).cloned().unwrap();

		assert_eq!(
			core_0_after_first.len(),
			1,
			"After first advance, one para_a should remain from the duplicate"
		);
		assert_eq!(
			core_0_after_first.front(),
			Some(&para_a),
			"The remaining element should be para_a"
		);

		// Now add another order for para_b while core 0 already has assignments
		on_demand::Pallet::<Test>::push_back_order(para_b);

		// Advance again
		System::set_block_number(4);
		scheduler::Pallet::<Test>::advance_claim_queue(|_| false);

		// Check claim queue - para_b should NOT be duplicated since the core wasn't empty
		let claim_queue_after = scheduler::Pallet::<Test>::claim_queue();
		let core_0_after = claim_queue_after.get(&CoreIndex(0)).cloned().unwrap();

		// After popping para_a, we should have [para_b] (para_b is not duplicated)
		// because the core wasn't empty when para_b was added
		assert_eq!(core_0_after.get(0), Some(&para_b), "First position should be para_b");

		// There should only be 1 element (no duplication of para_b)
		assert_eq!(
			core_0_after.len(),
			1,
			"Para_b should not be duplicated since core wasn't empty"
		);
	});
}
