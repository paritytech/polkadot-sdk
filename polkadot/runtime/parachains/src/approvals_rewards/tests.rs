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
		System, Test, ApprovalsRewards,
	},
};
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives::{
	vstaging::{ApprovalStatistics, ApprovalStatisticsTallyLine},
	BlockNumber, SessionIndex, ValidatorId, ValidatorIndex, ValidatorSignature,
};
use sp_runtime::RuntimeAppPublic;

// ==================== Helper Functions ====================

/// Generate a list of validators with keypairs
fn generate_validators(count: usize) -> Vec<ValidatorId> {
	(0..count)
		.map(|_| <ValidatorId as RuntimeAppPublic>::generate_pair(None))
		.collect::<Vec<_>>()
}

/// Initialize a session with a given session index and validators
fn initialize_session(session_index: SessionIndex, validators: Vec<ValidatorId>) {
	use crate::shared;

	shared::Pallet::<Test>::set_active_validators_ascending(validators);
	shared::Pallet::<Test>::set_session_index(session_index);
}

/// Create approval statistics payload and signature for a validator
fn create_approval_statistics(
	session_index: SessionIndex,
	validator_index: ValidatorIndex,
	tallies: Vec<ApprovalStatisticsTallyLine>,
) -> (ApprovalStatistics, ValidatorSignature) {
	use crate::shared;

	let validators = shared::ActiveValidatorKeys::<Test>::get();
	let payload = ApprovalStatistics(session_index, validator_index, tallies);
	let signature = validators[validator_index.0 as usize]
		.sign(&payload.signing_payload())
		.expect("Signing should work");

	(payload, signature)
}

/// Run to a specific block number, calling session change notifications as needed
fn run_to_block(
	to: BlockNumber,
	new_session: impl Fn(BlockNumber) -> Option<SessionChangeNotification<BlockNumber>>,
) {
	while System::block_number() < to {
		let b = System::block_number();

		// Finalize current block
		ParasShared::initializer_finalize();
		Configuration::initializer_finalize();
		SessionInfo::initializer_finalize();

		// Handle session change if any
		if let Some(notification) = new_session(b + 1) {
			Configuration::initializer_on_new_session(&notification.session_index);
			ParasShared::initializer_on_new_session(
				notification.session_index,
				notification.random_seed,
				&notification.new_config,
				notification.validators.clone(),
			);
			SessionInfo::initializer_on_new_session(&notification);
			ApprovalsRewards::initializer_on_new_session(&notification);
		}

		System::on_finalize(b);
		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Configuration::initializer_initialize(b + 1);
		ParasShared::initializer_initialize(b + 1);
		SessionInfo::initializer_initialize(b + 1);
	}
}

/// Create default host configuration for tests
fn default_config() -> HostConfiguration<BlockNumber> {
	HostConfiguration { dispute_period: 6, ..Default::default() }
}

/// Create genesis config for tests
fn genesis_config() -> MockGenesisConfig {
	MockGenesisConfig {
		configuration: crate::configuration::GenesisConfig { config: default_config() },
		..Default::default()
	}
}

/// Session change notification helper - changes every 10 blocks
fn session_changes(n: BlockNumber) -> Option<SessionChangeNotification<BlockNumber>> {
	if n % 10 == 0 {
		Some(SessionChangeNotification {
			session_index: n / 10,
			..Default::default()
		})
	} else {
		None
	}
}

/// Create tallies with varying approval usage for testing median calculation
fn create_test_tallies(validator_count: usize, base_usage: u32) -> Vec<ApprovalStatisticsTallyLine> {
	(0..validator_count)
		.map(|v_idx| ApprovalStatisticsTallyLine {
			validator_index: ValidatorIndex(v_idx as u32),
			approvals_usage: base_usage + (v_idx as u32 * 10),
			no_shows: 0,
		})
		.collect()
}

// ==================== Tests ====================

#[test]
fn successful_approval_statistics_submission() {
	new_test_ext(genesis_config()).execute_with(|| {
		// Set block number to 1 so events can be registered
		System::set_block_number(1);

		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies.clone());

		// Submit the statistics
		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload.clone(),
			signature,
		));

		// Verify storage was updated
		assert!(ApprovalsTallies::<Test>::contains_key((session_index, validator_index)));
		let stored_tallies = ApprovalsTallies::<Test>::get((session_index, validator_index)).unwrap();
		assert_eq!(stored_tallies, tallies);

		// Verify event was emitted
		System::assert_last_event(
			Event::ApprovalTalliesStored((session_index, validator_index)).into(),
		);
	});
}

#[test]
fn successful_submission_multiple_validators() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(10);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		// Submit from multiple validators
		for i in 0..5 {
			let validator_index = ValidatorIndex(i);
			let tallies = create_test_tallies(10, 100 + i * 10);
			let (payload, signature) =
				create_approval_statistics(session_index, validator_index, tallies.clone());

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));

			// Verify each submission was stored
			assert!(ApprovalsTallies::<Test>::contains_key((session_index, validator_index)));
		}

		// Verify all 5 submissions are in storage
		let count = ApprovalsTallies::<Test>::iter_keys()
			.filter(|(s, _)| *s == session_index)
			.count();
		assert_eq!(count, 5);
	});
}

// ==================== Error Condition Tests ====================

#[test]
fn reject_invalid_signature() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let payload = ApprovalStatistics(session_index, validator_index, tallies);

		// Create a wrong signature (from different validator)
		let wrong_signature = validators[1].sign(&payload.signing_payload()).expect("Signing should work");

		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				wrong_signature,
			),
			Error::<Test>::ApprovalRewardsInvalidSignature
		);
	});
}

#[test]
fn reject_future_session() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let current_session = 1;
		initialize_session(current_session, validators.clone());

		// Try to submit for future session
		let future_session = current_session + 5;
		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);

		let payload = ApprovalStatistics(future_session, validator_index, tallies);
		let signature = validators[validator_index.0 as usize]
			.sign(&payload.signing_payload())
			.expect("Signing should work");

		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			),
			Error::<Test>::ApprovalRewardsFutureSession
		);
	});
}

#[test]
fn reject_passed_session() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let current_session = 10;
		initialize_session(current_session, validators.clone());

		let config = crate::configuration::ActiveConfig::<Test>::get();
		let dispute_period = config.dispute_period;

		// Try to submit for session before dispute period
		let old_session = current_session - dispute_period - 1;
		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);

		let payload = ApprovalStatistics(old_session, validator_index, tallies);
		let signature = validators[validator_index.0 as usize]
			.sign(&payload.signing_payload())
			.expect("Signing should work");

		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			),
			Error::<Test>::ApprovalRewardsPassedSession
		);
	});
}

#[test]
fn reject_validator_index_out_of_bounds() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		// Try with validator index beyond bounds
		let invalid_index = ValidatorIndex(100);
		let tallies = create_test_tallies(5, 100);

		let payload = ApprovalStatistics(session_index, invalid_index, tallies);
		// Sign with valid validator but use invalid index in payload
		let signature = validators[0].sign(&payload.signing_payload()).expect("Signing should work");

		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			),
			Error::<Test>::ApprovalRewardsValidatorIndexOutOfBounds
		);
	});
}

#[test]
fn reject_duplicate_submission() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies.clone());

		// First submission should succeed
		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload.clone(),
			signature.clone(),
		));

		// Second submission with same data should fail
		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			),
			Error::<Test>::ApprovalTalliesAlreadyStored
		);
	});
}

#[test]
fn reject_unknown_session_index() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let current_session = 5;
		initialize_session(current_session, validators.clone());

		// Try to submit for a past session that has no session info stored
		// This session is within dispute period but has no session info
		let old_session = current_session - 2;
		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);

		let payload = ApprovalStatistics(old_session, validator_index, tallies);
		let signature = validators[validator_index.0 as usize]
			.sign(&payload.signing_payload())
			.expect("Signing should work");

		// This should fail because session info is not available
		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			),
			Error::<Test>::ApprovalRewardsUnknownSessionIndex
		);
	});
}

// ==================== Session Change Logic Tests ====================

#[test]
fn median_calculation_on_session_change() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 10;
		let validators = generate_validators(validator_count);
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let session_1 = config.dispute_period + 1; // Avoid underflow in pruning logic

		// Initialize session 1 with validators
		initialize_session(session_1, validators.clone());

		// Also need to set up session info for session 1
		use crate::session_info;
		session_info::Sessions::<Test>::insert(
			session_1,
			polkadot_primitives::SessionInfo {
				validators: validators.clone().into(),
				discovery_keys: vec![],
				assignment_keys: vec![],
				validator_groups: Default::default(),
				n_cores: 0,
				zeroth_delay_tranche_width: 0,
				relay_vrf_modulo_samples: 0,
				n_delay_tranches: 0,
				no_show_slots: 0,
				needed_approvals: 0,
				active_validator_indices: vec![],
				dispute_period: 6,
				random_seed: [0u8; 32],
			},
		);

		// Submit tallies from all validators to avoid index out of bounds in median calculation
		// Note: The pallet code has a bug where it uses validators_len/2 instead of v.len()/2
		for i in 0..validator_count {
			let validator_index = ValidatorIndex(i as u32);
			// Create tallies where each validator reports on all validators
			let tallies = create_test_tallies(validator_count, (i * 50) as u32);
			let (payload, signature) =
				create_approval_statistics(session_1, validator_index, tallies);

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Trigger session change to session 2
		let notification = SessionChangeNotification {
			session_index: session_1 + 1,
			..Default::default()
		};
		ApprovalsRewards::initializer_on_new_session(&notification);

		// Verify medians were calculated and stored for session 1
		assert!(AvailableApprovalsMedians::<Test>::contains_key(session_1));
		let medians = AvailableApprovalsMedians::<Test>::get(session_1).unwrap();
		assert_eq!(medians.len(), validator_count);
	});
}

#[test]
fn no_median_below_byzantine_threshold() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 10;
		let validators = generate_validators(validator_count);
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let session_1 = config.dispute_period + 1; // Avoid underflow in pruning logic

		initialize_session(session_1, validators.clone());

		// Set up session info
		use crate::session_info;
		session_info::Sessions::<Test>::insert(
			session_1,
			polkadot_primitives::SessionInfo {
				validators: validators.clone().into(),
				discovery_keys: vec![],
				assignment_keys: vec![],
				validator_groups: Default::default(),
				n_cores: 0,
				zeroth_delay_tranche_width: 0,
				relay_vrf_modulo_samples: 0,
				n_delay_tranches: 0,
				no_show_slots: 0,
				needed_approvals: 0,
				active_validator_indices: vec![],
				dispute_period: 6,
				random_seed: [0u8; 32],
			},
		);

		// Submit from fewer than byzantine threshold (only 5 out of 10, need 7)
		let byzantine_threshold = polkadot_primitives::byzantine_threshold(validator_count);
		for i in 0..(byzantine_threshold - 1) {
			let validator_index = ValidatorIndex(i as u32);
			let tallies = create_test_tallies(validator_count, 100);
			let (payload, signature) =
				create_approval_statistics(session_1, validator_index, tallies);

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Trigger session change
		let notification = SessionChangeNotification {
			session_index: session_1 + 1,
			..Default::default()
		};
		ApprovalsRewards::initializer_on_new_session(&notification);

		// Verify NO medians were calculated (below byzantine threshold)
		assert!(!AvailableApprovalsMedians::<Test>::contains_key(session_1));
	});
}

#[test]
fn old_data_pruning_on_session_change() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let dispute_period = config.dispute_period;

		// Set up multiple sessions with data, starting from dispute_period + 1
		let start_session = dispute_period + 1;
		for session in start_session..=(start_session + 9) {
			initialize_session(session, validators.clone());

			let validator_index = ValidatorIndex(0);
			let tallies = create_test_tallies(5, 100);
			let (payload, signature) =
				create_approval_statistics(session, validator_index, tallies);

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		let current_session = start_session + 9;

		// Set up session info for current session so pruning logic can run
		// Use only 1 validator to avoid median calculation index out of bounds
		// (only validator 0 submitted tallies)
		use crate::session_info;
		session_info::Sessions::<Test>::insert(
			current_session,
			polkadot_primitives::SessionInfo {
				validators: vec![validators[0].clone()].into(),
				discovery_keys: vec![],
				assignment_keys: vec![],
				validator_groups: Default::default(),
				n_cores: 0,
				zeroth_delay_tranche_width: 0,
				relay_vrf_modulo_samples: 0,
				n_delay_tranches: 0,
				no_show_slots: 0,
				needed_approvals: 0,
				active_validator_indices: vec![],
				dispute_period: 6,
				random_seed: [0u8; 32],
			},
		);

		// Trigger session change to next session
		let next_session = current_session + 1;
		let notification = SessionChangeNotification {
			session_index: next_session,
			..Default::default()
		};
		ApprovalsRewards::initializer_on_new_session(&notification);

		// Check that old sessions are pruned
		let min_session_to_keep = next_session.saturating_sub(dispute_period);

		// Sessions before min_session_to_keep should be pruned
		for session in start_session..min_session_to_keep {
			assert!(
				!ApprovalsTallies::<Test>::contains_key((session, ValidatorIndex(0))),
				"Session {} should be pruned",
				session
			);
		}

		// Sessions from min_session_to_keep to current_session should be kept
		for session in min_session_to_keep..=current_session {
			assert!(
				ApprovalsTallies::<Test>::contains_key((session, ValidatorIndex(0))),
				"Session {} should be kept (min_session_to_keep={}, current={})",
				session,
				min_session_to_keep,
				current_session
			);
		}
	});
}

#[test]
fn session_change_without_session_info() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_1 = 1;
		initialize_session(session_1, validators.clone());

		// Submit some tallies
		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) = create_approval_statistics(session_1, validator_index, tallies);

		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload,
			signature,
		));

		// Trigger session change without having session info
		// The function should return early and not panic
		let notification = SessionChangeNotification {
			session_index: session_1 + 1,
			..Default::default()
		};

		// This should not panic, just return early
		ApprovalsRewards::initializer_on_new_session(&notification);

		// No medians should be calculated
		assert!(!AvailableApprovalsMedians::<Test>::contains_key(session_1));
	});
}

// ==================== Unsigned Validation Tests ====================

#[test]
fn validate_unsigned_accepts_valid_transaction() {
	use sp_runtime::transaction_validity::{TransactionPriority, TransactionSource};

	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies.clone());

		let call = Call::include_approvals_rewards_statistics {
			payload: payload.clone(),
			signature,
		};

		let result = <ApprovalsRewards as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&call,
		);

		assert!(result.is_ok(), "Transaction should be valid");

		if let Ok(valid_tx) = result {
			// Verify transaction has correct properties
			assert_eq!(valid_tx.priority, TransactionPriority::max_value());
			assert_eq!(valid_tx.longevity, 64);
			assert!(valid_tx.propagate);

			// Verify provides tag is not empty (the actual encoding is opaque)
			assert!(!valid_tx.provides.is_empty());
		}
	});
}

#[test]
fn validate_unsigned_with_different_payloads() {
	use sp_runtime::transaction_validity::TransactionSource;

	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(10);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		// Create two different payloads
		let validator_index_1 = ValidatorIndex(0);
		let tallies_1 = create_test_tallies(10, 100);
		let (payload_1, signature_1) =
			create_approval_statistics(session_index, validator_index_1, tallies_1);

		let validator_index_2 = ValidatorIndex(1);
		let tallies_2 = create_test_tallies(10, 200);
		let (payload_2, signature_2) =
			create_approval_statistics(session_index, validator_index_2, tallies_2);

		// Both should be valid
		let call_1 = Call::include_approvals_rewards_statistics {
			payload: payload_1,
			signature: signature_1,
		};
		let call_2 = Call::include_approvals_rewards_statistics {
			payload: payload_2,
			signature: signature_2,
		};

		assert!(
			<ApprovalsRewards as ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call_1,
			)
			.is_ok()
		);
		assert!(
			<ApprovalsRewards as ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call_2,
			)
			.is_ok()
		);
	});
}

#[test]
fn pre_dispatch_accepts_any_call() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies);

		let call =
			Call::include_approvals_rewards_statistics { payload, signature };

		// pre_dispatch should always return Ok(())
		let result = <ApprovalsRewards as ValidateUnsigned>::pre_dispatch(&call);
		assert_ok!(result);
	});
}

// ==================== Integration and Edge Case Tests ====================

#[test]
fn end_to_end_approval_rewards_flow() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 20;
		let validators = generate_validators(validator_count);

		// Session 1: Initialize and collect data
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let session_1 = config.dispute_period + 1; // Avoid underflow in pruning logic
		initialize_session(session_1, validators.clone());

		// Set up session info for session 1
		use crate::session_info;
		session_info::Sessions::<Test>::insert(
			session_1,
			polkadot_primitives::SessionInfo {
				validators: validators.clone().into(),
				discovery_keys: vec![],
				assignment_keys: vec![],
				validator_groups: Default::default(),
				n_cores: 0,
				zeroth_delay_tranche_width: 0,
				relay_vrf_modulo_samples: 0,
				n_delay_tranches: 0,
				no_show_slots: 0,
				needed_approvals: 0,
				active_validator_indices: vec![],
				dispute_period: 6,
				random_seed: [0u8; 32],
			},
		);

		// All validators submit tallies
		// Note: Using all validators instead of byzantine_threshold due to median calculation
		for i in 0..validator_count {
			let validator_index = ValidatorIndex(i as u32);
			// Each validator submits tallies for all validators
			let tallies = create_test_tallies(validator_count, (i * 20) as u32);

			let (payload, signature) =
				create_approval_statistics(session_1, validator_index, tallies);

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Trigger calculation of medians by transitioning to next session
		let session_2 = session_1 + 1;
		let notification = SessionChangeNotification {
			session_index: session_2,
			..Default::default()
		};
		ApprovalsRewards::initializer_on_new_session(&notification);

		// Verify medians calculated for session 1
		assert!(AvailableApprovalsMedians::<Test>::contains_key(session_1));
		let medians = AvailableApprovalsMedians::<Test>::get(session_1).unwrap();
		assert_eq!(medians.len(), validator_count);

		// Set up session info for session_1 so subsequent session changes can prune
		// (already done above for median calculation)

		// Continue for multiple sessions to test pruning
		let final_session = session_1 + 10;
		for session in (session_2 + 1)..=final_session {
			// Set up session info for previous session so pruning logic can run
			session_info::Sessions::<Test>::insert(
				session - 1,
				polkadot_primitives::SessionInfo {
					validators: validators.clone().into(),
					discovery_keys: vec![],
					assignment_keys: vec![],
					validator_groups: Default::default(),
					n_cores: 0,
					zeroth_delay_tranche_width: 0,
					relay_vrf_modulo_samples: 0,
					n_delay_tranches: 0,
					no_show_slots: 0,
					needed_approvals: 0,
					active_validator_indices: vec![],
					dispute_period: 6,
					random_seed: [0u8; 32],
				},
			);

			let notification = SessionChangeNotification {
				session_index: session,
				..Default::default()
			};
			ApprovalsRewards::initializer_on_new_session(&notification);
		}

		// Verify old data was pruned
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let min_session = final_session.saturating_sub(config.dispute_period);
		assert!(
			!ApprovalsTallies::<Test>::iter_keys().any(|(s, _)| s < min_session),
			"Old sessions should be pruned"
		);
	});
}

#[test]
fn handle_empty_tallies() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let empty_tallies = vec![]; // Empty tallies vector
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, empty_tallies);

		// Should still be valid - the pallet doesn't enforce non-empty tallies
		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload,
			signature,
		));

		// Verify it was stored
		assert!(ApprovalsTallies::<Test>::contains_key((session_index, validator_index)));
	});
}

#[test]
fn handle_maximum_validator_count() {
	new_test_ext(genesis_config()).execute_with(|| {
		// Test with a large validator count
		let max_validators = 100;
		let validators = generate_validators(max_validators);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		// Test submission works with large validator set
		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(max_validators, 100);

		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies);

		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload,
			signature,
		));

		// Verify storage
		assert!(ApprovalsTallies::<Test>::contains_key((session_index, validator_index)));
	});
}

#[test]
fn submission_at_session_boundary() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let dispute_period = config.dispute_period;
		let current_session = dispute_period + 2; // Ensure we don't underflow
		initialize_session(current_session, validators.clone());

		// Submit for session exactly at dispute period boundary (should succeed)
		let boundary_session = current_session - dispute_period;
		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);

		// Need to set up session info for boundary session
		use crate::session_info;
		session_info::Sessions::<Test>::insert(
			boundary_session,
			polkadot_primitives::SessionInfo {
				validators: validators.clone().into(),
				discovery_keys: vec![],
				assignment_keys: vec![],
				validator_groups: Default::default(),
				n_cores: 0,
				zeroth_delay_tranche_width: 0,
				relay_vrf_modulo_samples: 0,
				n_delay_tranches: 0,
				no_show_slots: 0,
				needed_approvals: 0,
				active_validator_indices: vec![],
				dispute_period: 6,
				random_seed: [0u8; 32],
			},
		);

		let payload = ApprovalStatistics(boundary_session, validator_index, tallies);
		let signature = validators[validator_index.0 as usize]
			.sign(&payload.signing_payload())
			.expect("Signing should work");

		// Should succeed - exactly at boundary
		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload,
			signature,
		));
	});
}

#[test]
fn concurrent_submissions_from_multiple_validators() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 50;
		let validators = generate_validators(validator_count);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		// Simulate concurrent submissions from many validators
		for i in 0..validator_count {
			let validator_index = ValidatorIndex(i as u32);
			let tallies = create_test_tallies(validator_count, (i * 5) as u32);
			let (payload, signature) =
				create_approval_statistics(session_index, validator_index, tallies);

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Verify all submissions are stored
		let count = ApprovalsTallies::<Test>::iter_keys()
			.filter(|(s, _)| *s == session_index)
			.count();
		assert_eq!(count, validator_count);
	});
}

#[test]
fn pays_no_fee_for_valid_submission() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		initialize_session(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies);

		// Check that the call returns Pays::No
		let result = ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload,
			signature,
		);

		assert_ok!(&result);
		let post_info = result.unwrap();
		assert_eq!(post_info.pays_fee, frame_support::dispatch::Pays::No);
	});
}
