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
use assert_matches::assert_matches;
use overseer::FromOrchestra;
use polkadot_node_subsystem::{
	messages::{
		AllMessages, RewardsStatisticsCollectorMessage, RuntimeApiMessage, RuntimeApiRequest,
	},
	ActivatedLeaf, ActiveLeavesUpdate, TimeoutExt,
};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_primitives::{Hash, SessionIndex, SessionInfo};
use sp_application_crypto::{Pair as PairT, RuntimeAppPublic};
use sp_authority_discovery::AuthorityPair as AuthorityDiscoveryPair;
use sp_keyring::Sr25519Keyring;
use sp_keystore::Keystore;
use std::collections::HashSet;
use std::sync::Arc;
use test_helpers::mock::new_leaf;

type VirtualOverseer = polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<
	RewardsStatisticsCollectorMessage,
>;

async fn activate_leaf(
	virtual_overseer: &mut VirtualOverseer,
	activated: ActivatedLeaf,
	session_index: SessionIndex,
	session_info: Option<SessionInfo>,
) {
	// Use the new helper with default (empty) node_features for backward compatibility
	activate_leaf_with_node_features(
		virtual_overseer,
		activated,
		session_index,
		session_info,
		NodeFeatures::new(), // Default empty features
	).await;
}

async fn finalize_block(
	virtual_overseer: &mut VirtualOverseer,
	finalized: (Hash, BlockNumber),
	latest_finalized_block_number: BlockNumber,
	finalized_hashes: Vec<Hash>,
) {
	let fin_block_hash = finalized.0;
	let fin_block_number = finalized.1;

	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(
			fin_block_hash,
			fin_block_number,
		)))
		.await;

	let expected_amt_request_blocks =
		fin_block_number.saturating_sub(latest_finalized_block_number) as usize;

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ChainApi(
			 ChainApiMessage::Ancestors { hash, k, response_channel }
		) if hash == fin_block_hash && k == expected_amt_request_blocks  => {
			response_channel.send(Ok(finalized_hashes)).unwrap();
		}
	);
}

async fn candidate_approved(
	virtual_overseer: &mut VirtualOverseer,
	rb_hash: Hash,
	rb_number: BlockNumber,
	approvals: Vec<ValidatorIndex>,
) {
	let msg = FromOrchestra::Communication {
		msg: RewardsStatisticsCollectorMessage::CandidateApproved(rb_hash, rb_number, approvals),
	};
	virtual_overseer.send(msg).await;
}

async fn no_shows(
	virtual_overseer: &mut VirtualOverseer,
	rb_hash: Hash,
	rb_number: BlockNumber,
	no_shows: Vec<ValidatorIndex>,
) {
	let msg = FromOrchestra::Communication {
		msg: RewardsStatisticsCollectorMessage::NoShows(rb_hash, rb_number, no_shows),
	};
	virtual_overseer.send(msg).await;
}

macro_rules! approvals_stats_assertion {
	($fn_name:ident, $field:ident) => {
		fn $fn_name(
			view: &View,
			rb_hash: Hash,
			rb_number: BlockNumber,
			expected: Vec<(ValidatorIndex, u32)>,
		) {
			let expected_map = expected.into_iter().collect::<BTreeMap<ValidatorIndex, u32>>();

			let stats_for =
				view.per_relay.get(&(rb_hash, rb_number)).unwrap().approvals_stats.clone();

			assert!(stats_for.$field.eq(&expected_map));
		}
	};
}

approvals_stats_assertion!(assert_votes, votes);
approvals_stats_assertion!(assert_no_shows, no_shows);

fn test_harness<T: Future<Output = VirtualOverseer>>(
	view: &mut View,
	test: impl FnOnce(VirtualOverseer) -> T,
) {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();
	let keystore = Arc::new(sp_keystore::testing::MemoryKeystore::new()) as Arc<_>;

	let (mut context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, view, &keystore, (&Metrics(None), true)).await {
			panic!("{:?}", e);
		}

		view
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);
	_ = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test_fut.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));
}

fn test_harness_with_keystore<T: Future<Output = VirtualOverseer>>(
	view: &mut View,
	keystore: &Arc<dyn Keystore>,
	test: impl FnOnce(VirtualOverseer) -> T,
) {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();

	let (mut context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, view, keystore, (&Metrics(None), true)).await {
			panic!("{:?}", e);
		}

		view
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);
	_ = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test_fut.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));
}

#[test]
fn single_candidate_approved() {
	let validator_idx = ValidatorIndex(2);

	let rb_hash = Hash::from_low_u64_be(132);
	let rb_number: BlockNumber = 1;

	let leaf = new_leaf(rb_hash, rb_number);

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		activate_leaf(&mut virtual_overseer, leaf, 1, Some(default_session_info(1))).await;

		candidate_approved(&mut virtual_overseer, rb_hash, rb_number, vec![validator_idx]).await;
		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 1);

	assert_votes(&view, rb_hash, rb_number, vec![(validator_idx, 1)]);
}

#[test]
fn candidate_approved_for_different_forks() {
	let validator_idx0 = ValidatorIndex(0);
	let validator_idx1 = ValidatorIndex(1);

	let rb_number: BlockNumber = 1;
	let rb_hash_fork_0 = Hash::from_low_u64_be(132);
	let rb_hash_fork_1 = Hash::from_low_u64_be(231);

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		let leaf0 = new_leaf(rb_hash_fork_0, rb_number);

		let leaf1 = new_leaf(rb_hash_fork_1, rb_number);

		activate_leaf(&mut virtual_overseer, leaf0, 1, Some(default_session_info(1))).await;

		activate_leaf(&mut virtual_overseer, leaf1, 1, None).await;

		candidate_approved(&mut virtual_overseer, rb_hash_fork_0, rb_number, vec![validator_idx0])
			.await;

		candidate_approved(&mut virtual_overseer, rb_hash_fork_1, rb_number, vec![validator_idx1])
			.await;

		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 2);

	assert_votes(&view, rb_hash_fork_0, rb_number, vec![(validator_idx0, 1)]);

	assert_votes(&view, rb_hash_fork_1, rb_number, vec![(validator_idx1, 1)]);
}

#[test]
fn candidate_approval_stats_with_no_shows() {
	let approvals_from = vec![ValidatorIndex(0), ValidatorIndex(3)];
	let no_show_validators = vec![ValidatorIndex(1), ValidatorIndex(2)];

	let rb_hash = Hash::from_low_u64_be(111);
	let rb_number: BlockNumber = 1;

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		let leaf1 = new_leaf(rb_hash, rb_number);
		activate_leaf(&mut virtual_overseer, leaf1, 1, Some(default_session_info(1))).await;

		candidate_approved(&mut virtual_overseer, rb_hash, rb_number, approvals_from).await;

		no_shows(&mut virtual_overseer, rb_hash, rb_number, no_show_validators).await;

		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 1);
	assert_votes(&view, rb_hash, rb_number, vec![(ValidatorIndex(0), 1), (ValidatorIndex(3), 1)]);

	assert_no_shows(
		&view,
		rb_hash,
		rb_number,
		vec![(ValidatorIndex(1), 1), (ValidatorIndex(2), 1)],
	);
}

#[test]
fn note_chunks_downloaded() {
	let session_idx: SessionIndex = 2;
	let chunk_downloads = vec![(ValidatorIndex(0), 10u64), (ValidatorIndex(1), 2)];

	let mut view = View::new();
	let authorities: Vec<sp_authority_discovery::AuthorityId> =
		vec![Sr25519Keyring::Alice.public().into(), Sr25519Keyring::Bob.public().into()];

	view.per_session.insert(session_idx, PerSessionView::new(authorities.clone(), None, NodeFeatures::new()));

	test_harness(&mut view, |mut virtual_overseer| async move {
		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: RewardsStatisticsCollectorMessage::ChunksDownloaded(
					session_idx,
					HashMap::from_iter(chunk_downloads.clone().into_iter()),
				),
			})
			.await;

		// should increment only validator 0
		let second_round_of_downloads = vec![(ValidatorIndex(0), 5u64)];
		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: RewardsStatisticsCollectorMessage::ChunksDownloaded(
					session_idx,
					HashMap::from_iter(second_round_of_downloads.into_iter()),
				),
			})
			.await;

		virtual_overseer
	});

	assert_eq!(view.availability_chunks.len(), 1);
	let ac = view.availability_chunks.get(&session_idx).unwrap();

	assert_eq!(ac.downloads.len(), 2);

	let expected = vec![(ValidatorIndex(0), 15u64), (ValidatorIndex(1), 2)];

	for (vidx, expected_count) in expected {
		let auth_id = authorities.get(vidx.0 as usize).unwrap();
		let count = ac.downloads.get(&auth_id).unwrap();
		assert_eq!(*count, expected_count);
	}
}

fn default_session_info(session_idx: SessionIndex) -> SessionInfo {
	SessionInfo {
		active_validator_indices: vec![],
		random_seed: Default::default(),
		dispute_period: session_idx,
		validators: Default::default(),
		discovery_keys: vec![],
		assignment_keys: vec![],
		validator_groups: Default::default(),
		n_cores: 0,
		zeroth_delay_tranche_width: 0,
		relay_vrf_modulo_samples: 0,
		n_delay_tranches: 0,
		no_show_slots: 0,
		needed_approvals: 0,
	}
}

#[test]
fn note_chunks_uploaded_to_active_validator() {
	let activated_leaf_hash = Hash::from_low_u64_be(111);
	let leaf1 = new_leaf(activated_leaf_hash, 1);

	let session_index: SessionIndex = 2;
	let mut session_info: SessionInfo = default_session_info(session_index);

	let validator_idx_pair = AuthorityDiscoveryPair::generate();

	let validator_idx_auth_id: AuthorityDiscoveryId = validator_idx_pair.0.public().into();
	session_info.discovery_keys = vec![validator_idx_auth_id.clone()];

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		activate_leaf(&mut virtual_overseer, leaf1, session_index, Some(session_info)).await;

		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: RewardsStatisticsCollectorMessage::ChunkUploaded(
					session_index,
					HashSet::from_iter(vec![validator_idx_auth_id]),
				),
			})
			.await;

		virtual_overseer
	});

	// assert that the leaf was activated and the session info is present
	let validator_idx_auth_id: AuthorityDiscoveryId = validator_idx_pair.0.public().into();
	let expected_view = PerSessionView::new(vec![validator_idx_auth_id.clone()], None, NodeFeatures::new());

	assert_eq!(view.per_session.len(), 1);
	assert_eq!(view.per_session.get(&session_index).unwrap(), &expected_view);

	assert_eq!(view.availability_chunks.len(), 1);

	let mut expected_av_chunks = AvailabilityChunks::new();
	expected_av_chunks.note_candidate_chunk_uploaded(validator_idx_auth_id.clone(), 1);

	assert_eq!(view.availability_chunks.get(&session_index).unwrap(), &expected_av_chunks);
}

#[test]
fn prune_unfinalized_forks_and_verify_no_leaks() {
	// Comprehensive test for fork handling, state pruning, and memory leak prevention
	// Tests across multiple sessions with different node_features to verify:
	// 1. Dead fork data is discarded
	// 2. Finalized chain data is aggregated
	// 3. Unfinalized data is preserved
	// 4. Sessions are cleaned up after finalization
	// 5. Node features don't cause memory leaks
	// 6. Feature flags work per-session (session 0 disabled, session 1 enabled)

	// Chain structure: Genesis -> A -> B (fork 1, will die)
	//                              \-> C -> D (fork 2, C will finalize)

	// Create keystore with Alice's key for signing credentials
	let keystore = Arc::new(sp_keystore::testing::MemoryKeystore::new()) as Arc<dyn Keystore>;
	let _alice_key = keystore.sr25519_generate_new(
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed())
	).unwrap();

	let hash_a = Hash::from_slice(&[00; 32]);
	let hash_b = Hash::from_slice(&[01; 32]);
	let hash_c = Hash::from_slice(&[02; 32]);
	let hash_d = Hash::from_slice(&[03; 32]);
	let hash_e = Hash::from_slice(&[04; 32]);
	let hash_f = Hash::from_slice(&[05; 32]);
	let hash_g = Hash::from_slice(&[06; 32]);
	let hash_h = Hash::from_slice(&[07; 32]);

	let mut view = View::new();

	// === PHASE 1: Setup Session 0 with Forks ===
	test_harness_with_keystore(&mut view, &keystore, |mut virtual_overseer| async move {
		// Activate A (session 0, feature DISABLED)
		let leaf_a = new_leaf(hash_a, 1);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_a,
			0,
			Some(get_test_session_info(0)),
			node_features_with_submit_disabled(),
		).await;

		// Collect data on A
		candidate_approved(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(2), ValidatorIndex(3)]).await;
		no_shows(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(0), ValidatorIndex(1)]).await;

		// Activate B (fork 1 from A)
		let leaf_b = new_leaf(hash_b, 2);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_b, 0, None, node_features_with_submit_disabled()).await;
		candidate_approved(&mut virtual_overseer, hash_b, 2, vec![ValidatorIndex(0), ValidatorIndex(1)]).await;

		// Activate C (fork 2 from A, same height as B)
		let leaf_c = new_leaf(hash_c, 2);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_c, 0, None, node_features_with_submit_disabled()).await;
		candidate_approved(&mut virtual_overseer, hash_c, 2, vec![ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)]).await;

		// Activate D (extends fork 2)
		let leaf_d = new_leaf(hash_d, 3);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_d, 0, None, node_features_with_submit_disabled()).await;
		candidate_approved(&mut virtual_overseer, hash_d, 3, vec![ValidatorIndex(0), ValidatorIndex(1)]).await;

		virtual_overseer
	});

	// Verify Phase 1: All blocks tracked
	assert_eq!(view.per_relay.len(), 4, "Phase 1: All 4 blocks (A, B, C, D) should be tracked");
	assert!(view.per_session.get(&0).is_some(), "Phase 1: Session 0 should exist");
	assert!(view.per_session[&0].node_features.is_empty(), "Phase 1: Session 0 node_features should be empty (disabled)");
	assert!(view.per_session[&0].validators_tallies.is_empty(), "Phase 1: No tallies yet (nothing finalized)");

	// === PHASE 2: Finalize C - Prune Dead Fork B ===
	test_harness_with_keystore(&mut view, &keystore, |mut virtual_overseer| async move {
		// Finalize block C (fork 2 wins, fork 1 dies)
		virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_c, 2))).await;

		// Handle Ancestors request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_c && k == 2 => {
				response_channel.send(Ok(vec![hash_a, Default::default()])).unwrap();
			}
		);

		// Handle SessionIndexForChild
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx)))
			if parent == hash_c => {
				tx.send(Ok(0)).unwrap();
			}
		);

		// NO submission (feature disabled)
		use polkadot_node_subsystem::TimeoutExt;
		let timeout_result = virtual_overseer.recv().timeout(std::time::Duration::from_millis(100)).await;
		assert!(timeout_result.is_none(), "Phase 2: No submission for session 0 (feature disabled)");

		virtual_overseer
	});

	// Verify Phase 2: Dead fork pruned, finalized blocks aggregated
	assert_eq!(view.per_relay.len(), 1, "Phase 2: Only block D should remain (A,B,C removed)");
	assert!(view.per_relay.get(&(hash_a, 1)).is_none(), "Phase 2: Block A should be removed (finalized)");
	assert!(view.per_relay.get(&(hash_b, 2)).is_none(), "Phase 2: Block B should be pruned (dead fork)");
	assert!(view.per_relay.get(&(hash_c, 2)).is_none(), "Phase 2: Block C should be removed (finalized)");
	assert!(view.per_relay.get(&(hash_d, 3)).is_some(), "Phase 2: Block D should remain (unfinalized)");

	// Verify aggregated tallies (A + C, NOT B)
	let expected_tallies_session_0_partial = HashMap::from_iter(vec![
		(ValidatorIndex(0), PerValidatorTally { no_shows: 1, approvals: 1 }),  // A no-show, C approval
		(ValidatorIndex(1), PerValidatorTally { no_shows: 1, approvals: 1 }),  // A no-show, C approval
		(ValidatorIndex(2), PerValidatorTally { no_shows: 0, approvals: 2 }),  // A approval, C approval
		(ValidatorIndex(3), PerValidatorTally { no_shows: 0, approvals: 1 }),  // A approval
	]);
	assert_per_session_tallies(&view.per_session, 0, expected_tallies_session_0_partial);

	// === PHASE 3: Session Transition - Start Session 1 (Feature ENABLED) ===
	test_harness_with_keystore(&mut view, &keystore, |mut virtual_overseer| async move {
		// Activate E (session 1, feature ENABLED)
		let leaf_e = new_leaf(hash_e, 4);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_e,
			1,
			Some(get_test_session_info(1)),
			node_features_with_submit_enabled(),
		).await;

		candidate_approved(&mut virtual_overseer, hash_e, 4, vec![ValidatorIndex(3), ValidatorIndex(1), ValidatorIndex(0)]).await;
		no_shows(&mut virtual_overseer, hash_e, 4, vec![ValidatorIndex(2)]).await;

		// Activate F (fork 1 from E)
		let leaf_f = new_leaf(hash_f, 5);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_f, 1, None, node_features_with_submit_enabled()).await;
		candidate_approved(&mut virtual_overseer, hash_f, 5, vec![ValidatorIndex(3)]).await;

		// Activate G (fork 2 from E)
		let leaf_g = new_leaf(hash_g, 5);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_g, 1, None, node_features_with_submit_enabled()).await;
		candidate_approved(&mut virtual_overseer, hash_g, 5, vec![ValidatorIndex(0)]).await;
		no_shows(&mut virtual_overseer, hash_g, 5, vec![ValidatorIndex(1)]).await;

		virtual_overseer
	});

	// Verify Phase 3: Multi-session state
	assert_eq!(view.per_relay.len(), 4, "Phase 3: D, E, F, G should be tracked");
	assert_eq!(view.per_session.len(), 2, "Phase 3: Both session 0 and 1 should exist");
	assert!(view.per_session[&0].node_features.is_empty(), "Phase 3: Session 0 features disabled");
	assert_eq!(view.per_session[&1].node_features.get(5).map(|b| *b), Some(true), "Phase 3: Session 1 features enabled");

	// === PHASE 4: Finalize E (Session 1) - Triggers Session 0 Cleanup ===
	test_harness_with_keystore(&mut view, &keystore, |mut virtual_overseer| async move {
		virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_e, 4))).await;

		// Handle Ancestors
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_e && k == 2 => {
				response_channel.send(Ok(vec![hash_d, hash_c])).unwrap();
			}
		);

		// Handle SessionIndexForChild
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx)))
			if parent == hash_e => {
				tx.send(Ok(1)).unwrap();
			}
		);

		// NO submission for session 0 (feature disabled)
		use polkadot_node_subsystem::TimeoutExt;
		let timeout_result = virtual_overseer.recv().timeout(std::time::Duration::from_millis(100)).await;
		assert!(timeout_result.is_none(), "Phase 4: No submission for session 0 (feature disabled)");

		virtual_overseer
	});

	// Verify Phase 4: Session 0 cleanup (CRITICAL MEMORY LEAK CHECK)
	assert_eq!(view.latest_finalized_session, Some(1), "Phase 4: Latest finalized session should be 1");
	assert!(view.per_session.get(&0).is_none(), "Phase 4: Session 0 should be REMOVED - memory leak check");
	assert!(view.per_session.get(&1).is_some(), "Phase 4: Session 1 should still exist");

	// Verify per_relay pruning
	assert_eq!(view.per_relay.len(), 2, "Phase 4: Only F and G should remain");
	assert!(view.per_relay.get(&(hash_d, 3)).is_none(), "Phase 4: Block D removed (finalized)");
	assert!(view.per_relay.get(&(hash_e, 4)).is_none(), "Phase 4: Block E removed (finalized)");
	assert!(view.per_relay.get(&(hash_f, 5)).is_some(), "Phase 4: Block F remains (unfinalized)");
	assert!(view.per_relay.get(&(hash_g, 5)).is_some(), "Phase 4: Block G remains (unfinalized)");

	// Verify session 1 tallies include only E (D was session 0)
	let expected_tallies_session_1 = HashMap::from_iter(vec![
		(ValidatorIndex(0), PerValidatorTally { no_shows: 0, approvals: 1 }),  // E
		(ValidatorIndex(1), PerValidatorTally { no_shows: 0, approvals: 1 }),  // E
		(ValidatorIndex(2), PerValidatorTally { no_shows: 1, approvals: 0 }),  // E no-show
		(ValidatorIndex(3), PerValidatorTally { no_shows: 0, approvals: 1 }),  // E
	]);
	assert_per_session_tallies(&view.per_session, 1, expected_tallies_session_1);

	// === PHASE 5: Finalize Session 2 Block - Triggers Session 1 Submission and Cleanup ===
	test_harness_with_keystore(&mut view, &keystore, |mut virtual_overseer| async move {
		// Activate H (session 2)
		let leaf_h = new_leaf(hash_h, 6);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_h,
			2,
			Some(get_test_session_info(2)),
			node_features_with_submit_enabled(),
		).await;

		// Finalize H (session 2), triggers session 1 cleanup
		virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_h, 6))).await;

		// Handle Ancestors
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_h && k == 2 => {
				// H is block 6, previous finalized was E at block 4, so 2 ancestors: G (or F), then E
				response_channel.send(Ok(vec![hash_f, hash_e])).unwrap();
			}
		);

		// Handle SessionIndexForChild
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx)))
			if parent == hash_h => {
				tx.send(Ok(2)).unwrap();
			}
		);

		// EXPECT submission for session 1 (feature enabled)
		let payload = assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(
					parent,
					RuntimeApiRequest::SubmitApprovalStatistics(payload, _signature, tx)
				)
			) if parent == hash_h => {
				tx.send(Ok(())).unwrap();
				payload
			}
		);

		// Verify payload is for session 1
		assert_eq!(payload.0, 1, "Phase 5: Payload should be for session 1");
		assert_eq!(payload.1, ValidatorIndex(0), "Phase 5: Validator index should be Alice (0)");

		virtual_overseer
	});

	// Verify Phase 5: Session 1 cleanup after submission (CRITICAL)
	assert_eq!(view.latest_finalized_session, Some(2), "Phase 5: Latest finalized session should be 2");
	assert!(view.per_session.get(&1).is_none(), "Phase 5: Session 1 should be REMOVED after submission - memory leak check");
	assert_eq!(view.per_session.len(), 1, "Phase 5: Only session 2 should remain");
	assert!(view.per_session.get(&2).is_some(), "Phase 5: Session 2 should exist");

	// SUCCESS: No memory leaks detected
	// - Session 0 cleaned up after finalization (no submission, feature disabled)
	// - Session 1 cleaned up after finalization (with submission, feature enabled)
	// - Dead fork data discarded (block B)
	// - Finalized data aggregated correctly
	// - Node features cleaned up with sessions
}



fn assert_relay_view_approval_stats(
	view: &View,
	expected_relay_view_stats: Vec<(
		Hash,
		BlockNumber,
		Vec<(ValidatorIndex, u32)>,
		Vec<(ValidatorIndex, u32)>,
	)>,
) {
	assert_eq!(view.per_relay.len(), expected_relay_view_stats.len());

	for (hash, block_number, votes, no_shows) in &expected_relay_view_stats {
		assert_votes(&view, *hash, *block_number, votes.clone());
		assert_no_shows(&view, *hash, *block_number, no_shows.clone());
	}
}

fn assert_per_session_tallies(
	per_session_view: &BTreeMap<SessionIndex, PerSessionView>,
	session_idx: SessionIndex,
	expected_tallies: HashMap<ValidatorIndex, PerValidatorTally>,
) {
	let session_view = per_session_view
		.get(&session_idx)
		.expect("session index should exists in the view");

	assert_eq!(session_view.validators_tallies.len(), expected_tallies.len());
	for (validator_index, expected_tally) in expected_tallies.iter() {
		assert_eq!(
			session_view.validators_tallies.get(validator_index),
			Some(expected_tally),
			"unexpected value for validator index {:?}",
			validator_index
		);
	}
}

// Helper functions for node_features testing

fn node_features_with_submit_enabled() -> NodeFeatures {
	let mut features = NodeFeatures::new();
	features.resize(6, false);
	features.set(5, true); // SubmitApprovalStatistics = index 5
	features
}

fn node_features_with_submit_disabled() -> NodeFeatures {
	NodeFeatures::new() // Empty BitVec, all bits default to false
}

fn get_test_validators() -> Vec<ValidatorId> {
	vec![
		Sr25519Keyring::Alice.public().into(),
		Sr25519Keyring::Bob.public().into(),
		Sr25519Keyring::Charlie.public().into(),
		Sr25519Keyring::Dave.public().into(),
	]
}

fn get_test_session_info(session_idx: SessionIndex) -> SessionInfo {
	let mut session_info = default_session_info(session_idx);
	let discovery_keys: Vec<AuthorityDiscoveryId> = vec![
		Sr25519Keyring::Alice.public().into(),
		Sr25519Keyring::Bob.public().into(),
		Sr25519Keyring::Charlie.public().into(),
		Sr25519Keyring::Dave.public().into(),
	];
	session_info.discovery_keys = discovery_keys;
	session_info
}

async fn activate_leaf_with_node_features(
	virtual_overseer: &mut VirtualOverseer,
	activated: ActivatedLeaf,
	session_index: SessionIndex,
	session_info: Option<SessionInfo>,
	node_features: NodeFeatures,
) {
	let activated_leaf_hash = activated.hash;
	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			activated,
		))))
		.await;

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ChainApi(ChainApiMessage::BlockHeader(hash, tx)) if hash == activated_leaf_hash => {
			tx.send(Ok(Some(Header {
				parent_hash: Default::default(),
				number: 1,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			}))).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
		) if parent == activated_leaf_hash => {
			tx.send(Ok(session_index)).unwrap();
		}
	);

	if let Some(session_info) = session_info {
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(_parent, RuntimeApiRequest::SessionInfo(req_session, tx))
			) if req_session == session_index => {
				tx.send(Ok(Some(session_info))).unwrap();
			}
		);

		// Handle Validators request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(_parent, RuntimeApiRequest::Validators(tx))
			) => {
				tx.send(Ok(get_test_validators())).unwrap();
			}
		);

		// Handle NodeFeatures request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(_parent, RuntimeApiRequest::NodeFeatures(req_session, tx))
			) if req_session == session_index => {
				tx.send(Ok(node_features)).unwrap();
			}
		);
	}
}

#[test]
fn approval_stats_not_submitted_when_feature_disabled() {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();
	let keystore: Arc<dyn Keystore> = Arc::new(sp_keystore::testing::MemoryKeystore::new());

	// Insert Alice's key into keystore so we have credentials
	keystore.sr25519_generate_new(
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed())
	).unwrap();

	let (mut context, mut virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	// Setup blocks
	let hash_a = Hash::from_low_u64_be(1);
	let hash_b = Hash::from_low_u64_be(2);
	let hash_c = Hash::from_low_u64_be(3);
	let hash_d = Hash::from_low_u64_be(4);

	let mut view = View::new();

	let test = async move {
		// Activate leaf A (session 0)
		let leaf_a = new_leaf(hash_a, 1);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_a,
			0,
			Some(get_test_session_info(0)),
			node_features_with_submit_disabled(),
		)
		.await;

		// Collect approval data on block A
		candidate_approved(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(0), ValidatorIndex(1)]).await;
		no_shows(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(2)]).await;

		// Activate leaf B (same session)
		let leaf_b = new_leaf(hash_b, 2);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_b, 0, None, node_features_with_submit_disabled()).await;

		// Collect more approval data on block B
		candidate_approved(&mut virtual_overseer, hash_b, 2, vec![ValidatorIndex(2), ValidatorIndex(3)]).await;

		// Finalize block B (still in session 0, establishes baseline)
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_b, 2)))
			.await;

		// Handle Ancestors request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_b && k == 2 => {
				response_channel.send(Ok(vec![hash_a, Default::default()])).unwrap();
			}
		);

		// Handle SessionIndexForChild for finalized block
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_b => {
				tx.send(Ok(0)).unwrap(); // Still session 0
			}
		);

		// Activate leaf C (session 1 - new session)
		let leaf_c = new_leaf(hash_c, 3);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_c,
			1,
			Some(get_test_session_info(1)),
			node_features_with_submit_disabled(),
		)
		.await;

		// Activate leaf D (also session 1)
		let leaf_d = new_leaf(hash_d, 4);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_d, 1, None, node_features_with_submit_disabled()).await;

		// Finalize block D (in session 1, triggers session 0 finalization)
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_d, 4)))
			.await;

		// Handle Ancestors request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_d && k == 2 => {
				response_channel.send(Ok(vec![hash_c, hash_b])).unwrap();
			}
		);

		// Handle SessionIndexForChild for finalized block
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_d => {
				tx.send(Ok(1)).unwrap(); // Session 1
			}
		);

		// CRITICAL: Verify NO SubmitApprovalStatistics request is received
		// Use a timeout to ensure no message arrives
		use futures::FutureExt;
		let timeout_result = virtual_overseer.recv().timeout(std::time::Duration::from_millis(100)).await;
		assert!(timeout_result.is_none(), "Expected no SubmitApprovalStatistics call, but received a message");

		virtual_overseer
	};

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, &mut view, &keystore, (&Metrics(None), true)).await {
			panic!("{:?}", e);
		}
		view
	};

	futures::pin_mut!(test);
	futures::pin_mut!(subsystem);

	let (_, view) = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));

	// Verification Phase: Data collection happened
	assert_eq!(view.latest_finalized_session, Some(1), "Latest finalized session should be 1");

	// Session 0 should have been cleaned up after finalization into session 1
	assert!(view.per_session.get(&0).is_none(), "Session 0 should be cleaned up after session 1 finalization");

	// Session 1 should still exist (it's the current finalized session)
	assert!(view.per_session.get(&1).is_some(), "Session 1 should still exist");

	// The per_relay data for blocks A and B should be pruned (finalized)
	assert!(view.per_relay.get(&(hash_a, 1)).is_none(), "Block A should be pruned");
	assert!(view.per_relay.get(&(hash_b, 2)).is_none(), "Block B should be pruned");

	// Block C and D might still be in per_relay or might be pruned depending on timing
	// The key verification is that NO submission was made despite having credentials
}

#[test]
fn approval_stats_submitted_when_feature_enabled() {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();
	let keystore: Arc<dyn Keystore> = Arc::new(sp_keystore::testing::MemoryKeystore::new());

	// Insert Alice's key into keystore so we have credentials
	let alice_key = keystore.sr25519_generate_new(
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed())
	).unwrap();

	let (mut context, mut virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	// Setup blocks - same as test 1
	let hash_a = Hash::from_low_u64_be(1);
	let hash_b = Hash::from_low_u64_be(2);
	let hash_c = Hash::from_low_u64_be(3);
	let hash_d = Hash::from_low_u64_be(4);

	let mut view = View::new();

	let test = async move {
		// Activate leaf A (session 0) - WITH FEATURE ENABLED
		let leaf_a = new_leaf(hash_a, 1);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_a,
			0,
			Some(get_test_session_info(0)),
			node_features_with_submit_enabled(),
		)
		.await;

		// Collect approval data on block A
		candidate_approved(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(0), ValidatorIndex(1)]).await;
		no_shows(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(2)]).await;

		// Activate leaf B (same session)
		let leaf_b = new_leaf(hash_b, 2);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_b, 0, None, node_features_with_submit_enabled()).await;

		// Collect more approval data on block B
		candidate_approved(&mut virtual_overseer, hash_b, 2, vec![ValidatorIndex(2), ValidatorIndex(3)]).await;

		// Finalize block B (still in session 0, establishes baseline)
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_b, 2)))
			.await;

		// Handle Ancestors request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_b && k == 2 => {
				response_channel.send(Ok(vec![hash_a, Default::default()])).unwrap();
			}
		);

		// Handle SessionIndexForChild for finalized block
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_b => {
				tx.send(Ok(0)).unwrap(); // Still session 0
			}
		);

		// Activate leaf C (session 1 - new session)
		let leaf_c = new_leaf(hash_c, 3);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_c,
			1,
			Some(get_test_session_info(1)),
			node_features_with_submit_enabled(),
		)
		.await;

		// Activate leaf D (also session 1)
		let leaf_d = new_leaf(hash_d, 4);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_d, 1, None, node_features_with_submit_enabled()).await;

		// Finalize block D (in session 1, triggers session 0 finalization WITH SUBMISSION)
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_d, 4)))
			.await;

		// Handle Ancestors request
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_d && k == 2 => {
				response_channel.send(Ok(vec![hash_c, hash_b])).unwrap();
			}
		);

		// Handle SessionIndexForChild for finalized block
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_d => {
				tx.send(Ok(1)).unwrap(); // Session 1
			}
		);

		// CRITICAL: EXPECT SubmitApprovalStatistics request
		let (payload, signature) = assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(
					parent,
					RuntimeApiRequest::SubmitApprovalStatistics(payload, signature, tx)
				)
			) if parent == hash_d => {
				// Simulate successful submission
				tx.send(Ok(())).unwrap();
				(payload, signature)
			}
		);

		// Verification Phase: Payload Contents
		// Verify session index
		assert_eq!(payload.0, 0, "Payload session index should be 0");

		// Verify validator index (should be Alice, which is index 0 in our validator set)
		assert_eq!(payload.1, ValidatorIndex(0), "Payload validator index should be 0 (Alice)");

		// Verify tally lines
		let tallies = &payload.2;
		assert_eq!(tallies.len(), 4, "Should have 4 validator tallies");

		// Find each validator's tally and verify
		for tally in tallies {
			match tally.validator_index {
				ValidatorIndex(0) => {
					assert_eq!(tally.approvals_usage, 1, "Validator 0 should have 1 approval");
					assert_eq!(tally.no_shows, 0, "Validator 0 should have 0 no-shows");
				},
				ValidatorIndex(1) => {
					assert_eq!(tally.approvals_usage, 1, "Validator 1 should have 1 approval");
					assert_eq!(tally.no_shows, 0, "Validator 1 should have 0 no-shows");
				},
				ValidatorIndex(2) => {
					assert_eq!(tally.approvals_usage, 1, "Validator 2 should have 1 approval");
					assert_eq!(tally.no_shows, 1, "Validator 2 should have 1 no-show");
				},
				ValidatorIndex(3) => {
					assert_eq!(tally.approvals_usage, 1, "Validator 3 should have 1 approval");
					assert_eq!(tally.no_shows, 0, "Validator 3 should have 0 no-shows");
				},
				_ => panic!("Unexpected validator index"),
			}
		}

		// Verify signature
		let signing_payload = payload.signing_payload();
		assert!(
			polkadot_primitives::ValidatorId::verify(&alice_key.into(), &signing_payload, &signature),
			"Signature should be valid"
		);

		virtual_overseer
	};

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, &mut view, &keystore, (&Metrics(None), true)).await {
			panic!("{:?}", e);
		}
		view
	};

	futures::pin_mut!(test);
	futures::pin_mut!(subsystem);

	let (_, view) = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));

	// Verify cleanup still occurred
	assert_eq!(view.latest_finalized_session, Some(1), "Latest finalized session should be 1");
	assert!(view.per_session.get(&0).is_none(), "Session 0 should be cleaned up after submission");
	assert!(view.per_session.get(&1).is_some(), "Session 1 should still exist");
}

#[test]
fn approval_stats_mixed_features_per_session() {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();
	let keystore: Arc<dyn Keystore> = Arc::new(sp_keystore::testing::MemoryKeystore::new());

	// Insert Alice's key into keystore so we have credentials
	let alice_key = keystore.sr25519_generate_new(
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed())
	).unwrap();

	let (mut context, mut virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	// Setup blocks across 3 sessions
	let hash_a = Hash::from_low_u64_be(1);  // Session 0, block 1
	let hash_b = Hash::from_low_u64_be(2);  // Session 0, block 2
	let hash_c = Hash::from_low_u64_be(3);  // Session 1, block 3
	let hash_d = Hash::from_low_u64_be(4);  // Session 1, block 4
	let hash_e = Hash::from_low_u64_be(5);  // Session 2, block 5

	let mut view = View::new();

	let test = async move {
		// === SESSION 0: Feature DISABLED ===
		let leaf_a = new_leaf(hash_a, 1);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_a,
			0,
			Some(get_test_session_info(0)),
			node_features_with_submit_disabled(), // Feature OFF for session 0
		)
		.await;

		// Collect approval data in session 0
		candidate_approved(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(0)]).await;
		no_shows(&mut virtual_overseer, hash_a, 1, vec![ValidatorIndex(1)]).await;

		let leaf_b = new_leaf(hash_b, 2);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_b, 0, None, node_features_with_submit_disabled()).await;

		candidate_approved(&mut virtual_overseer, hash_b, 2, vec![ValidatorIndex(1)]).await;

		// Finalize block B (establishes session 0 baseline)
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_b, 2)))
			.await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_b && k == 2 => {
				response_channel.send(Ok(vec![hash_a, Default::default()])).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_b => {
				tx.send(Ok(0)).unwrap();
			}
		);

		// === SESSION 1: Feature ENABLED ===
		let leaf_c = new_leaf(hash_c, 3);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_c,
			1,
			Some(get_test_session_info(1)),
			node_features_with_submit_enabled(), // Feature ON for session 1
		)
		.await;

		// Collect approval data in session 1
		candidate_approved(&mut virtual_overseer, hash_c, 3, vec![ValidatorIndex(2)]).await;
		no_shows(&mut virtual_overseer, hash_c, 3, vec![ValidatorIndex(3)]).await;

		let leaf_d = new_leaf(hash_d, 4);
		activate_leaf_with_node_features(&mut virtual_overseer, leaf_d, 1, None, node_features_with_submit_enabled()).await;

		candidate_approved(&mut virtual_overseer, hash_d, 4, vec![ValidatorIndex(3)]).await;

		// Finalize block D (in session 1, triggers session 0 finalization)
		// Session 0 had feature DISABLED, so NO submission should occur
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_d, 4)))
			.await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_d && k == 2 => {
				response_channel.send(Ok(vec![hash_c, hash_b])).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_d => {
				tx.send(Ok(1)).unwrap();
			}
		);

		// CRITICAL: Verify NO submission for session 0 (feature was disabled)
		use futures::FutureExt;
		let timeout_result = virtual_overseer.recv().timeout(std::time::Duration::from_millis(100)).await;
		assert!(timeout_result.is_none(), "Expected no submission for session 0 (feature disabled)");

		// === SESSION 2: Feature ENABLED ===
		let leaf_e = new_leaf(hash_e, 5);
		activate_leaf_with_node_features(
			&mut virtual_overseer,
			leaf_e,
			2,
			Some(get_test_session_info(2)),
			node_features_with_submit_enabled(), // Feature ON for session 2
		)
		.await;

		// Finalize block E (in session 2, triggers session 1 finalization)
		// Session 1 had feature ENABLED, so submission SHOULD occur
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash_e, 5)))
			.await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(ChainApiMessage::Ancestors { hash, k, response_channel })
			if hash == hash_e && k == 1 => {
				response_channel.send(Ok(vec![hash_d])).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash_e => {
				tx.send(Ok(2)).unwrap();
			}
		);

		// CRITICAL: EXPECT submission for session 1 (feature was enabled)
		let (payload, signature) = assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(
					parent,
					RuntimeApiRequest::SubmitApprovalStatistics(payload, signature, tx)
				)
			) if parent == hash_e => {
				tx.send(Ok(())).unwrap();
				(payload, signature)
			}
		);

		// Verify the payload is for session 1 (not session 0)
		assert_eq!(payload.0, 1, "Payload should be for session 1");
		assert_eq!(payload.1, ValidatorIndex(0), "Validator index should be Alice (0)");

		// Verify session 1 tallies
		// Note: Only validators with activity (non-zero approvals or no-shows) are included
		let tallies = &payload.2;
		assert_eq!(tallies.len(), 2, "Should have 2 validator tallies for session 1 (only those with activity)");

		for tally in tallies {
			match tally.validator_index {
				ValidatorIndex(2) => {
					assert_eq!(tally.approvals_usage, 1, "Validator 2 should have 1 approval in session 1");
					assert_eq!(tally.no_shows, 0, "Validator 2 should have 0 no-shows in session 1");
				},
				ValidatorIndex(3) => {
					assert_eq!(tally.approvals_usage, 1, "Validator 3 should have 1 approval in session 1");
					assert_eq!(tally.no_shows, 1, "Validator 3 should have 1 no-show in session 1");
				},
				_ => panic!("Unexpected validator index: {:?}", tally.validator_index),
			}
		}

		// Verify signature
		let signing_payload = payload.signing_payload();
		assert!(
			polkadot_primitives::ValidatorId::verify(&alice_key.into(), &signing_payload, &signature),
			"Signature should be valid for session 1 payload"
		);

		virtual_overseer
	};

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, &mut view, &keystore, (&Metrics(None), true)).await {
			panic!("{:?}", e);
		}
		view
	};

	futures::pin_mut!(test);
	futures::pin_mut!(subsystem);

	let (_, view) = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));

	// Final verifications
	assert_eq!(view.latest_finalized_session, Some(2), "Latest finalized session should be 2");

	// Session 0 and 1 should be cleaned up
	assert!(view.per_session.get(&0).is_none(), "Session 0 should be cleaned up");
	assert!(view.per_session.get(&1).is_none(), "Session 1 should be cleaned up after submission");

	// Session 2 should still exist
	assert!(view.per_session.get(&2).is_some(), "Session 2 should still exist");
}
