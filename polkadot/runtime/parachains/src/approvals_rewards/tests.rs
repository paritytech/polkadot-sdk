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
		new_test_ext, ApprovalsRewards, Configuration, MockGenesisConfig, ParasShared,
		RuntimeOrigin, SessionInfo, System, Test, APPROVAL_REWARDS,
	},
};
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives::{
	node_features::FeatureIndex,
	vstaging::{ApprovalStatistics, ApprovalStatisticsTallyLine},
	BlockNumber, SessionIndex, ValidatorId, ValidatorIndex, ValidatorSignature,
};
use sp_runtime::RuntimeAppPublic;

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

/// Enable the SubmitApprovalStatistics feature and open a submission window for the given session.
///
/// Sets:
/// - `SubmitApprovalStatistics` node feature flag
/// - `shared::CurrentSessionIndex` = session_index + 1 (so prev_session = session_index)
/// - `session_info::Sessions` for session_index (needed for validator lookup during submission)
/// - `CurrentSessionStartBlock` = current block number
/// - `SettlingForSession` = session_index
fn enable_feature_and_open_window(session_index: SessionIndex, validators: Vec<ValidatorId>) {
	// Enable SubmitApprovalStatistics feature
	crate::configuration::ActiveConfig::<Test>::mutate(|config| {
		let feature_idx = FeatureIndex::SubmitApprovalStatistics as usize;
		config.node_features.resize(feature_idx + 1, false);
		config.node_features.set(feature_idx, true);
	});

	// Set active validators and advance session so prev_session = session_index
	crate::shared::Pallet::<Test>::set_active_validators_ascending(validators.clone());
	crate::shared::Pallet::<Test>::set_session_index(session_index + 1);

	// Store session info for session_index (needed for validator public key lookup)
	crate::session_info::Sessions::<Test>::insert(
		session_index,
		polkadot_primitives::SessionInfo {
			validators: validators.into(),
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

	// Insert AccountKeys (required by calculate_medians_and_reward's defensive_proof).
	// Use empty keys by default; tests that verify rewards must overwrite this.
	crate::session_info::AccountKeys::<Test>::insert(
		session_index,
		Vec::<crate::mock::AccountId>::new(),
	);

	// Open the submission window
	let current_block = frame_system::Pallet::<Test>::block_number();
	CurrentSessionStartBlock::<Test>::put(current_block);
	SettlingForSession::<Test>::put(session_index);
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
		ApprovalsRewards::on_initialize(b + 1);
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
		Some(SessionChangeNotification { session_index: n / 10, ..Default::default() })
	} else {
		None
	}
}

/// Create tallies with varying approval usage for testing median calculation
fn create_test_tallies(
	validator_count: usize,
	base_usage: u32,
) -> Vec<ApprovalStatisticsTallyLine> {
	(0..validator_count)
		.map(|v_idx| ApprovalStatisticsTallyLine {
			validator_index: ValidatorIndex(v_idx as u32),
			approvals_usage: base_usage + (v_idx as u32 * 10),
			no_shows: 0,
		})
		.collect()
}

#[test]
fn successful_approval_statistics_submission() {
	new_test_ext(genesis_config()).execute_with(|| {
		// Set block number to 1 so events can be registered
		System::set_block_number(1);

		let validators = generate_validators(5);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

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
		assert!(ApprovalsTallies::<Test>::contains_key(session_index, validator_index));
		let stored_tallies = ApprovalsTallies::<Test>::get(session_index, validator_index).unwrap();
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
		enable_feature_and_open_window(session_index, validators.clone());

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
			assert!(ApprovalsTallies::<Test>::contains_key(session_index, validator_index));
		}

		// Verify all 5 submissions are in storage
		let count = ApprovalsTallies::<Test>::iter_keys()
			.filter(|(s, _)| *s == session_index)
			.count();
		assert_eq!(count, 5);
	});
}

#[test]
fn reject_invalid_signature() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let payload = ApprovalStatistics(session_index, validator_index, tallies);

		// Create a wrong signature (from different validator)
		let wrong_signature =
			validators[1].sign(&payload.signing_payload()).expect("Signing should work");

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
		// Open window for session 1 (current shared index = 2, prev = 1)
		enable_feature_and_open_window(current_session, validators.clone());

		// Try to submit for a session that is not prev_session (1+5=6 != 1)
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
		// session_index = 10: current shared index = 11, prev_session = 10
		let session_index = 10;
		enable_feature_and_open_window(session_index, validators.clone());

		// Try to submit for a session that is not prev_session (old sessions are rejected
		// with ApprovalRewardsFutureSession since only prev_session is accepted)
		let old_session = 3;
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
			Error::<Test>::ApprovalRewardsFutureSession
		);
	});
}

#[test]
fn reject_validator_index_out_of_bounds() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

		// Try with validator index beyond bounds
		let invalid_index = ValidatorIndex(100);
		let tallies = create_test_tallies(5, 100);

		let payload = ApprovalStatistics(session_index, invalid_index, tallies);
		// Sign with valid validator but use invalid index in payload
		let signature =
			validators[0].sign(&payload.signing_payload()).expect("Signing should work");

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
		enable_feature_and_open_window(session_index, validators.clone());

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
		// session_index = 4: current = 5, prev = 4. Enable feature + open window
		// but intentionally do NOT store session info for session 4.
		let session_index = 4;

		crate::configuration::ActiveConfig::<Test>::mutate(|config| {
			let feature_idx = FeatureIndex::SubmitApprovalStatistics as usize;
			config.node_features.resize(feature_idx + 1, false);
			config.node_features.set(feature_idx, true);
		});
		crate::shared::Pallet::<Test>::set_active_validators_ascending(validators.clone());
		crate::shared::Pallet::<Test>::set_session_index(session_index + 1);
		// Open window but skip session_info insertion
		CurrentSessionStartBlock::<Test>::put(0u32);
		SettlingForSession::<Test>::put(session_index);

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);

		let payload = ApprovalStatistics(session_index, validator_index, tallies);
		let signature = validators[validator_index.0 as usize]
			.sign(&payload.signing_payload())
			.expect("Signing should work");

		// Fails because session info is not available
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

#[test]
fn median_calculation_on_session_change() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 10;
		let validators = generate_validators(validator_count);
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let session_1 = config.dispute_period + 1; // Avoid underflow in pruning logic

		// Open window at block 0: current_session = session_1+1, prev = session_1
		enable_feature_and_open_window(session_1, validators.clone());

		// Submit tallies from all validators within the window (block 0 ≤ window_end 5)
		for i in 0..validator_count {
			let validator_index = ValidatorIndex(i as u32);
			let tallies = create_test_tallies(validator_count, (i * 50) as u32);
			let (payload, signature) =
				create_approval_statistics(session_1, validator_index, tallies);

			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Advance block past window_end (window_end = 0 + 5 = 5, so use block 6)
		System::set_block_number(6);

		// Trigger session change: window expired → settles session_1
		let notification =
			SessionChangeNotification { session_index: session_1 + 1, ..Default::default() };
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

		// Open window at block 0
		enable_feature_and_open_window(session_1, validators.clone());

		// Submit from fewer than byzantine threshold (only threshold-1 out of 10)
		let threshold = polkadot_primitives::byzantine_threshold(validator_count);
		for i in 0..(threshold - 1) {
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

		// Advance block past window_end
		System::set_block_number(6);

		// Trigger session change: window expired → settles with insufficient tallies
		let notification =
			SessionChangeNotification { session_index: session_1 + 1, ..Default::default() };
		ApprovalsRewards::initializer_on_new_session(&notification);

		// Verify NO medians were calculated (below byzantine threshold)
		assert!(!AvailableApprovalsMedians::<Test>::contains_key(session_1));
		assert!(IncompleteSessions::<Test>::contains_key(session_1));
	});
}

#[test]
fn old_data_pruning_on_session_change() {
	new_test_ext(genesis_config()).execute_with(|| {
		let config = crate::configuration::ActiveConfig::<Test>::get();
		let dispute_period = config.dispute_period;

		// Enable SubmitApprovalStatistics feature so initializer_on_new_session runs pruning logic
		crate::configuration::ActiveConfig::<Test>::mutate(|config| {
			let feature_idx = FeatureIndex::SubmitApprovalStatistics as usize;
			config.node_features.resize(feature_idx + 1, false);
			config.node_features.set(feature_idx, true);
		});

		// Directly insert incomplete-session entries and tally data for multiple sessions.
		// Incomplete sessions are what initializer_on_new_session actually prunes.
		let start_session = dispute_period + 1;
		for session in start_session..=(start_session + 9) {
			IncompleteSessions::<Test>::insert(session, ());
			ApprovalsTallies::<Test>::insert(
				session,
				ValidatorIndex(0),
				vec![ApprovalStatisticsTallyLine {
					validator_index: ValidatorIndex(0),
					approvals_usage: 100,
					no_shows: 0,
				}],
			);
		}

		let current_session = start_session + 9;
		let next_session = current_session + 1;
		let notification =
			SessionChangeNotification { session_index: next_session, ..Default::default() };
		ApprovalsRewards::initializer_on_new_session(&notification);

		// min_session_to_keep = next_session - dispute_period
		let min_session_to_keep = next_session.saturating_sub(dispute_period);

		// Sessions before min_session_to_keep: tallies pruned via IncompleteSessions
		for session in start_session..min_session_to_keep {
			assert!(
				!ApprovalsTallies::<Test>::contains_key(session, ValidatorIndex(0)),
				"Session {} should be pruned",
				session
			);
			assert!(!IncompleteSessions::<Test>::contains_key(session));
		}

		// Sessions from min_session_to_keep onward: still present
		for session in min_session_to_keep..=current_session {
			assert!(
				ApprovalsTallies::<Test>::contains_key(session, ValidatorIndex(0)),
				"Session {} should be kept",
				session
			);
			assert!(IncompleteSessions::<Test>::contains_key(session));
		}
	});
}

#[test]
fn session_change_without_session_info() {
	new_test_ext(genesis_config()).execute_with(|| {
		let session_1 = 1;
		// Open window but do NOT store session info for session_1 (that's what we're testing)
		crate::configuration::ActiveConfig::<Test>::mutate(|config| {
			let feature_idx = FeatureIndex::SubmitApprovalStatistics as usize;
			config.node_features.resize(feature_idx + 1, false);
			config.node_features.set(feature_idx, true);
		});
		crate::shared::Pallet::<Test>::set_session_index(session_1 + 1);
		CurrentSessionStartBlock::<Test>::put(0u32);
		SettlingForSession::<Test>::put(session_1);

		// Directly insert a tally (bypassing submission since there's no session info)
		ApprovalsTallies::<Test>::insert(session_1, ValidatorIndex(0), create_test_tallies(5, 100));

		// Advance past window_end then trigger session change without session info
		System::set_block_number(6);
		let notification =
			SessionChangeNotification { session_index: session_1 + 1, ..Default::default() };

		// Should not panic - settle_specific_session returns early when session info missing
		ApprovalsRewards::initializer_on_new_session(&notification);

		// No medians should be calculated
		assert!(!AvailableApprovalsMedians::<Test>::contains_key(session_1));
	});
}

#[test]
fn validate_unsigned_accepts_valid_transaction() {
	use sp_runtime::transaction_validity::{TransactionPriority, TransactionSource};

	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies.clone());

		let call =
			Call::include_approvals_rewards_statistics { payload: payload.clone(), signature };

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
		enable_feature_and_open_window(session_index, validators.clone());

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

		assert!(<ApprovalsRewards as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&call_1,
		)
		.is_ok());
		assert!(<ApprovalsRewards as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&call_2,
		)
		.is_ok());
	});
}

#[test]
fn pre_dispatch_accepts_any_call() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies);

		let call = Call::include_approvals_rewards_statistics { payload, signature };

		// pre_dispatch should always return Ok(())
		let result = <ApprovalsRewards as ValidateUnsigned>::pre_dispatch(&call);
		assert_ok!(result);
	});
}

#[test]
fn end_to_end_approval_rewards_flow() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 20;
		let validators = generate_validators(validator_count);

		let config = crate::configuration::ActiveConfig::<Test>::get();
		let session_1 = config.dispute_period + 1;

		// Open window for session_1 at block 0
		enable_feature_and_open_window(session_1, validators.clone());

		// All validators submit tallies
		for i in 0..validator_count {
			let validator_index = ValidatorIndex(i as u32);
			let tallies = create_test_tallies(validator_count, (i * 20) as u32);
			let (payload, signature) =
				create_approval_statistics(session_1, validator_index, tallies);
			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Advance past window_end and trigger session transition → medians calculated
		System::set_block_number(6);
		let session_2 = session_1 + 1;
		ApprovalsRewards::initializer_on_new_session(&SessionChangeNotification {
			session_index: session_2,
			..Default::default()
		});

		// Verify medians calculated for session 1 (tallies cleared by calculate_medians_and_reward)
		assert!(AvailableApprovalsMedians::<Test>::contains_key(session_1));
		assert_eq!(
			AvailableApprovalsMedians::<Test>::get(session_1).unwrap().len(),
			validator_count
		);
		assert!(!ApprovalsTallies::<Test>::iter_keys().any(|(s, _)| s == session_1));

		// Seed IncompleteSessions for later sessions to test tally pruning
		let final_session = session_2 + 10;
		for session in session_2..final_session {
			IncompleteSessions::<Test>::insert(session, ());
			ApprovalsTallies::<Test>::insert(
				session,
				ValidatorIndex(0),
				create_test_tallies(1, 50),
			);
		}

		// Advance many sessions so old ones fall outside the dispute period
		ApprovalsRewards::initializer_on_new_session(&SessionChangeNotification {
			session_index: final_session,
			..Default::default()
		});

		let min_session = final_session.saturating_sub(config.dispute_period);
		assert!(
			!ApprovalsTallies::<Test>::iter_keys().any(|(s, _)| s < min_session),
			"Incomplete sessions older than dispute period should be pruned"
		);
	});
}

#[test]
fn handle_empty_tallies() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

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
		assert!(ApprovalsTallies::<Test>::contains_key(session_index, validator_index));
	});
}

#[test]
fn handle_maximum_validator_count() {
	new_test_ext(genesis_config()).execute_with(|| {
		// Test with a large validator count
		let max_validators = 100;
		let validators = generate_validators(max_validators);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

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
		assert!(ApprovalsTallies::<Test>::contains_key(session_index, validator_index));
	});
}

#[test]
fn submission_at_session_boundary() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validators = generate_validators(5);
		let session_index = 1;
		// Window opens at block 0, window_end = 0 + 5 = 5
		enable_feature_and_open_window(session_index, validators.clone());

		let validator_index = ValidatorIndex(0);
		let tallies = create_test_tallies(5, 100);

		// Advance to exactly window_end (block 5) — submission should still succeed
		System::set_block_number(5);
		let (payload, signature) =
			create_approval_statistics(session_index, validator_index, tallies);
		assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
			RuntimeOrigin::none(),
			payload,
			signature,
		));

		// Advance past window_end (block 6) — second validator's submission should fail
		System::set_block_number(6);
		let validator_index_2 = ValidatorIndex(1);
		let tallies_2 = create_test_tallies(5, 200);
		let (payload_2, signature_2) =
			create_approval_statistics(session_index, validator_index_2, tallies_2);
		assert_noop!(
			ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload_2,
				signature_2,
			),
			Error::<Test>::ApprovalRewardsPassedSession
		);
	});
}

#[test]
fn concurrent_submissions_from_multiple_validators() {
	new_test_ext(genesis_config()).execute_with(|| {
		let validator_count = 50;
		let validators = generate_validators(validator_count);
		let session_index = 1;
		enable_feature_and_open_window(session_index, validators.clone());

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
		enable_feature_and_open_window(session_index, validators.clone());

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

#[test]
fn rewards_accumulate_after_settlement() {
	new_test_ext(genesis_config()).execute_with(|| {
		// Clear any rewards leftover from other tests in this thread
		APPROVAL_REWARDS.with(|r| r.borrow_mut().clear());

		System::set_block_number(1);

		let validator_count = 5;
		let validators = generate_validators(validator_count);
		let session_index = 1;

		// Open window at block 1 → window_end = 1 + 5 = 6
		enable_feature_and_open_window(session_index, validators.clone());

		// Insert AccountKeys so reward_by_ids receives actual account IDs.
		// MockValidatorSet::validators() returns empty, so we manually populate.
		let account_ids: Vec<crate::mock::AccountId> =
			(0..validator_count).map(|i| (10 + i) as u64).collect();
		crate::session_info::AccountKeys::<Test>::insert(session_index, account_ids.clone());

		// All validators submit the same tallies (all report 100, 110, 120, 130, 140
		// for validators 0-4 respectively).
		for i in 0..validator_count {
			let validator_index = ValidatorIndex(i as u32);
			let tallies = create_test_tallies(validator_count, 100);
			let (payload, signature) =
				create_approval_statistics(session_index, validator_index, tallies);
			assert_ok!(ApprovalsRewards::include_approvals_rewards_statistics(
				RuntimeOrigin::none(),
				payload,
				signature,
			));
		}

		// Run to window_end + 1 = block 7; on_initialize(7) triggers settlement
		run_to_block(7, |_| None);

		// Settlement should have calculated medians and called reward_by_ids
		assert!(
			AvailableApprovalsMedians::<Test>::contains_key(session_index),
			"medians should be stored after settlement"
		);

		let medians = AvailableApprovalsMedians::<Test>::get(session_index).unwrap();
		// All 5 reporters submitted the same tallies, so median == the single value
		// Validator i has median = 100 + i * 10
		for (i, &median) in medians.iter().enumerate() {
			assert_eq!(median, 100 + (i as u32 * 10), "median mismatch for validator {}", i);
		}

		// Verify reward_by_ids was called with (account_id, median * APPROVALS_POINTS)
		let recorded = APPROVAL_REWARDS.with(|r| r.borrow().clone());
		let expected: Vec<(crate::mock::AccountId, u32)> = (0..validator_count)
			.map(|i| {
				let median = 100 + (i as u32 * 10);
				(account_ids[i], median * APPROVALS_POINTS)
			})
			.collect();
		assert_eq!(recorded, expected, "recorded rewards should match median * APPROVALS_POINTS");
	});
}
