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
use crate::{
	configuration::HostConfiguration,
	initializer::SessionChangeNotification,
	mock::{
		new_test_ext, Configuration, MockGenesisConfig, ParasShared, RuntimeOrigin, SessionInfo,
		System, Test,
	},
	util::take_active_subset,
};
use polkadot_primitives::{vstaging::SchedulerParams, BlockNumber, ValidatorId, ValidatorIndex};
use sp_keyring::Sr25519Keyring;

fn run_to_block(
	to: BlockNumber,
	new_session: impl Fn(BlockNumber) -> Option<SessionChangeNotification<BlockNumber>>,
) {
	while System::block_number() < to {
		let b = System::block_number();

		SessionInfo::initializer_finalize();
		ParasShared::initializer_finalize();
		Configuration::initializer_finalize();

		if let Some(notification) = new_session(b + 1) {
			Configuration::initializer_on_new_session(&notification.session_index);
			ParasShared::initializer_on_new_session(
				notification.session_index,
				notification.random_seed,
				&notification.new_config,
				notification.validators.clone(),
			);
			SessionInfo::initializer_on_new_session(&notification);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Configuration::initializer_initialize(b + 1);
		ParasShared::initializer_initialize(b + 1);
		SessionInfo::initializer_initialize(b + 1);
	}
}

fn default_config() -> HostConfiguration<BlockNumber> {
	HostConfiguration {
		dispute_period: 2,
		needed_approvals: 3,
		scheduler_params: SchedulerParams { num_cores: 1, ..Default::default() },
		..Default::default()
	}
}

fn genesis_config() -> MockGenesisConfig {
	MockGenesisConfig {
		configuration: configuration::GenesisConfig { config: default_config() },
		..Default::default()
	}
}

fn session_changes(n: BlockNumber) -> Option<SessionChangeNotification<BlockNumber>> {
	if n.is_multiple_of(10) {
		Some(SessionChangeNotification { session_index: n / 10, ..Default::default() })
	} else {
		None
	}
}

fn new_session_every_block(n: BlockNumber) -> Option<SessionChangeNotification<BlockNumber>> {
	Some(SessionChangeNotification { session_index: n, ..Default::default() })
}

#[test]
fn session_pruning_is_based_on_dispute_period() {
	new_test_ext(genesis_config()).execute_with(|| {
		// Dispute period starts at 2
		let config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(config.dispute_period, 2);

		// Move to session 10
		run_to_block(100, session_changes);
		// Earliest stored session is 10 - 2 = 8
		assert_eq!(EarliestStoredSession::<Test>::get(), 8);
		// Pruning works as expected
		assert!(Sessions::<Test>::get(7).is_none());
		assert!(Sessions::<Test>::get(8).is_some());
		assert!(Sessions::<Test>::get(9).is_some());

		// changing `dispute_period` works
		let dispute_period = 5;
		Configuration::set_dispute_period(RuntimeOrigin::root(), dispute_period).unwrap();

		// Dispute period does not automatically change
		let config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(config.dispute_period, 2);
		// Two sessions later it will though
		run_to_block(120, session_changes);
		let config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(config.dispute_period, 5);

		run_to_block(200, session_changes);
		assert_eq!(EarliestStoredSession::<Test>::get(), 20 - dispute_period);

		// Increase dispute period even more
		let new_dispute_period = 16;
		Configuration::set_dispute_period(RuntimeOrigin::root(), new_dispute_period).unwrap();

		run_to_block(210, session_changes);
		assert_eq!(EarliestStoredSession::<Test>::get(), 21 - dispute_period);

		// Two sessions later it kicks in
		run_to_block(220, session_changes);
		let config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(config.dispute_period, 16);
		// Earliest session stays the same
		assert_eq!(EarliestStoredSession::<Test>::get(), 21 - dispute_period);

		// We still don't have enough stored sessions to start pruning
		run_to_block(300, session_changes);
		assert_eq!(EarliestStoredSession::<Test>::get(), 21 - dispute_period);

		// now we do
		run_to_block(420, session_changes);
		assert_eq!(EarliestStoredSession::<Test>::get(), 42 - new_dispute_period);
	})
}

#[test]
fn session_info_is_based_on_config() {
	new_test_ext(genesis_config()).execute_with(|| {
		run_to_block(1, new_session_every_block);
		let session = Sessions::<Test>::get(&1).unwrap();
		assert_eq!(session.needed_approvals, 3);

		// change some param
		Configuration::set_needed_approvals(RuntimeOrigin::root(), 42).unwrap();
		// 2 sessions later
		run_to_block(3, new_session_every_block);
		let session = Sessions::<Test>::get(&3).unwrap();
		assert_eq!(session.needed_approvals, 42);
	})
}

#[test]
fn session_info_active_subsets() {
	let unscrambled = vec![
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Eve,
	];

	let active_set = vec![ValidatorIndex(4), ValidatorIndex(0), ValidatorIndex(2)];

	let unscrambled_validators: Vec<ValidatorId> =
		unscrambled.iter().map(|v| v.public().into()).collect();
	let unscrambled_discovery: Vec<AuthorityDiscoveryId> =
		unscrambled.iter().map(|v| v.public().into()).collect();
	let unscrambled_assignment: Vec<AssignmentId> =
		unscrambled.iter().map(|v| v.public().into()).collect();

	let validators = take_active_subset(&active_set, &unscrambled_validators);

	new_test_ext(genesis_config()).execute_with(|| {
		ParasShared::set_active_validators_with_indices(active_set.clone(), validators.clone());

		assert_eq!(shared::ActiveValidatorIndices::<Test>::get(), active_set);

		AssignmentKeysUnsafe::<Test>::set(unscrambled_assignment.clone());
		crate::mock::set_discovery_authorities(unscrambled_discovery.clone());
		assert_eq!(<Test>::authorities(), unscrambled_discovery);

		// invoke directly, because `run_to_block` will invoke `Shared`	and clobber our
		// values.
		SessionInfo::initializer_on_new_session(&SessionChangeNotification {
			session_index: 1,
			validators: validators.clone(),
			..Default::default()
		});
		let session = Sessions::<Test>::get(&1).unwrap();

		assert_eq!(session.validators.to_vec(), validators);
		assert_eq!(
			session.discovery_keys,
			take_active_subset_and_inactive(&active_set, &unscrambled_discovery),
		);
		assert_eq!(
			session.assignment_keys,
			take_active_subset(&active_set, &unscrambled_assignment),
		);
	})
}

#[test]
fn session_execution_config_is_stored_per_session() {
	new_test_ext(genesis_config()).execute_with(|| {
		run_to_block(1, new_session_every_block);

		// Session 1 should snapshot every field from the active config.
		let exec_config = SessionExecutionConfigs::<Test>::get(1).unwrap();
		let active_config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(exec_config.max_pov_size, active_config.max_pov_size);
		assert_eq!(
			exec_config.validation_code_bomb_limit,
			active_config.validation_code_bomb_limit()
		);
		assert_eq!(exec_config.max_code_size, active_config.max_code_size);
		assert_eq!(exec_config.max_head_data_size, active_config.max_head_data_size);
		assert_eq!(
			exec_config.max_upward_message_num_per_candidate,
			active_config.max_upward_message_num_per_candidate
		);
		assert_eq!(exec_config.max_upward_message_size, active_config.max_upward_message_size);
		assert_eq!(
			exec_config.hrmp_max_message_num_per_candidate,
			active_config.hrmp_max_message_num_per_candidate
		);

		// Snapshot the old (session 1) values for later divergence checks.
		let old_max_pov_size = exec_config.max_pov_size;
		let old_max_code_size = exec_config.max_code_size;
		let old_validation_code_bomb_limit = exec_config.validation_code_bomb_limit;
		let old_max_head_data_size = exec_config.max_head_data_size;
		let old_max_upward_message_num = exec_config.max_upward_message_num_per_candidate;
		let old_max_upward_message_size = exec_config.max_upward_message_size;
		let old_hrmp_max_message_num = exec_config.hrmp_max_message_num_per_candidate;

		// Change all fields that are snapshotted into SessionExecutionConfig.
		// Each change takes 2 sessions to activate.
		Configuration::set_max_pov_size(RuntimeOrigin::root(), 1024).unwrap();
		// max_code_size must be > 0 and <= MAX_CODE_SIZE; use a smaller value than default.
		Configuration::set_max_code_size(RuntimeOrigin::root(), 512 * 1024).unwrap();
		Configuration::set_max_head_data_size(RuntimeOrigin::root(), 32 * 1024).unwrap();
		Configuration::set_max_upward_message_num_per_candidate(RuntimeOrigin::root(), 7).unwrap();
		Configuration::set_max_upward_message_size(RuntimeOrigin::root(), 48 * 1024).unwrap();
		Configuration::set_hrmp_max_message_num_per_candidate(RuntimeOrigin::root(), 11).unwrap();

		// Takes 2 sessions to activate — run to session 3.
		run_to_block(3, new_session_every_block);
		let active_config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(active_config.max_pov_size, 1024);
		assert_eq!(active_config.max_code_size, 512 * 1024);
		assert_eq!(active_config.max_head_data_size, 32 * 1024);
		assert_eq!(active_config.max_upward_message_num_per_candidate, 7);
		assert_eq!(active_config.max_upward_message_size, 48 * 1024);
		assert_eq!(active_config.hrmp_max_message_num_per_candidate, 11);

		// Session 3's snapshot should reflect the new values.
		let exec_config_3 = SessionExecutionConfigs::<Test>::get(3).unwrap();
		assert_eq!(exec_config_3.max_pov_size, 1024);
		assert_eq!(exec_config_3.max_code_size, 512 * 1024);
		// validation_code_bomb_limit is derived from max_code_size.
		assert_eq!(
			exec_config_3.validation_code_bomb_limit,
			active_config.validation_code_bomb_limit()
		);
		assert_eq!(exec_config_3.max_head_data_size, 32 * 1024);
		assert_eq!(exec_config_3.max_upward_message_num_per_candidate, 7);
		assert_eq!(exec_config_3.max_upward_message_size, 48 * 1024);
		assert_eq!(exec_config_3.hrmp_max_message_num_per_candidate, 11);

		// Session 1's snapshot must still hold the *old* values — proving the snapshot is
		// genuinely per-session and not re-read from live config.
		let exec_config_1 = SessionExecutionConfigs::<Test>::get(1).unwrap();
		assert_eq!(exec_config_1.max_pov_size, old_max_pov_size);
		assert_ne!(exec_config_1.max_pov_size, 1024);
		assert_eq!(exec_config_1.max_code_size, old_max_code_size);
		assert_ne!(exec_config_1.max_code_size, 512 * 1024);
		assert_eq!(exec_config_1.validation_code_bomb_limit, old_validation_code_bomb_limit);
		assert_ne!(
			exec_config_1.validation_code_bomb_limit,
			exec_config_3.validation_code_bomb_limit
		);
		assert_eq!(exec_config_1.max_head_data_size, old_max_head_data_size);
		assert_ne!(exec_config_1.max_head_data_size, 32 * 1024);
		assert_eq!(exec_config_1.max_upward_message_num_per_candidate, old_max_upward_message_num);
		assert_ne!(exec_config_1.max_upward_message_num_per_candidate, 7);
		assert_eq!(exec_config_1.max_upward_message_size, old_max_upward_message_size);
		assert_ne!(exec_config_1.max_upward_message_size, 48 * 1024);
		assert_eq!(exec_config_1.hrmp_max_message_num_per_candidate, old_hrmp_max_message_num);
		assert_ne!(exec_config_1.hrmp_max_message_num_per_candidate, 11);
	})
}

#[test]
fn session_execution_config_is_pruned_with_dispute_window() {
	new_test_ext(genesis_config()).execute_with(|| {
		// dispute_period = 2
		let config = configuration::ActiveConfig::<Test>::get();
		assert_eq!(config.dispute_period, 2);

		run_to_block(100, session_changes);
		// Earliest stored session is 10 - 2 = 8
		assert_eq!(EarliestStoredSession::<Test>::get(), 8);

		// Session 7 should be pruned
		assert!(SessionExecutionConfigs::<Test>::get(7).is_none());
		// Sessions 8 and 9 should still exist
		assert!(SessionExecutionConfigs::<Test>::get(8).is_some());
		assert!(SessionExecutionConfigs::<Test>::get(9).is_some());
		// Current session 10 should exist
		assert!(SessionExecutionConfigs::<Test>::get(10).is_some());
	})
}
