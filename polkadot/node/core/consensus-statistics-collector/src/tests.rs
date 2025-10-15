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
use polkadot_primitives::{AssignmentId, GroupIndex, SessionIndex, SessionInfo};
use polkadot_node_subsystem::messages::{AllMessages, ChainApiResponseChannel, ConsensusStatisticsCollectorMessage, RuntimeApiMessage, RuntimeApiRequest};

type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<ConsensusStatisticsCollectorMessage>;
use polkadot_node_subsystem::{ActivatedLeaf};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_primitives::{Hash, Header};
use sp_application_crypto::Pair as PairT;
use sp_authority_discovery::AuthorityPair as AuthorityDiscoveryPair;
use test_helpers::mock::new_leaf;

async fn activate_leaf(
	virtual_overseer: &mut VirtualOverseer,
	activated: ActivatedLeaf,
) {
	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			activated,
		))))
		.await;
}

async fn candidate_approved(
	virtual_overseer: &mut VirtualOverseer,
	candidate_hash: CandidateHash,
	rb_hash: Hash,
	approvals: Vec<ValidatorIndex>,
) {
	let msg = FromOrchestra::Communication {
		msg: ConsensusStatisticsCollectorMessage::CandidateApproved(
			candidate_hash.clone(),
			rb_hash.clone(),
			approvals,
		),
	};
	virtual_overseer.send(msg).await;
}

async fn no_shows(
	virtual_overseer: &mut VirtualOverseer,
	candidate_hash: CandidateHash,
	rb_hash: Hash,
	no_shows: Vec<ValidatorIndex>,
) {
	let msg = FromOrchestra::Communication {
		msg: ConsensusStatisticsCollectorMessage::NoShows(
			candidate_hash.clone(),
			rb_hash.clone(),
			no_shows,
		),
	};
	virtual_overseer.send(msg).await;
}

macro_rules! approvals_stats_assertion {
    ($fn_name:ident, $field:ident) => {
        fn $fn_name(
            view: &View,
            rb_hash: Hash,
            candidate_hash: CandidateHash,
            expected_votes: Vec<ValidatorIndex>,
        ) {
            let stats_for = view.per_relay.get(&rb_hash).unwrap();
            let approvals_for = stats_for.approvals_stats.get(&candidate_hash).unwrap();
            let collected_votes = approvals_for
                .$field
                .clone()
                .into_iter()
                .collect::<Vec<ValidatorIndex>>();

            assert_eq!(expected_votes, collected_votes);
        }
    };
}

approvals_stats_assertion!(assert_votes, votes);
approvals_stats_assertion!(assert_no_shows, no_shows);

fn test_harness<T: Future<Output = VirtualOverseer>>(
	test: impl FnOnce(VirtualOverseer) -> T,
) -> View {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();

	let (mut context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let mut view = View::new();

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, &mut view, &Metrics(None)).await {
			panic!("{:?}", e);
		}

		view
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);
	let (_, view) = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test_fut.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));

	view
}

#[test]
fn single_candidate_approved() {
    let validator_idx = ValidatorIndex(2);
    let candidate_hash: CandidateHash = CandidateHash(
        Hash::from_low_u64_be(111));
        
    let rb_hash = Hash::from_low_u64_be(132);
    let leaf = new_leaf(
        rb_hash.clone(),
        1,
    );

    let view = test_harness(|mut virtual_overseer| async move {
        activate_leaf(&mut virtual_overseer, leaf.clone()).await;
		candidate_approved(&mut virtual_overseer, candidate_hash.clone(), rb_hash, vec![validator_idx.clone()]).await;
        virtual_overseer
    });

    assert_eq!(view.per_relay.len(), 1);
    let stats_for = view.per_relay.get(&rb_hash).unwrap();
    let approvals_for = stats_for.approvals_stats.get(&candidate_hash).unwrap();

	println!("{:?}", approvals_for.votes);
    
    let expected_votes = vec![validator_idx];
    let collected_votes= approvals_for
		.clone()
		.votes
        .into_iter()
        .collect::<Vec<ValidatorIndex>>();

    assert_eq!(expected_votes, collected_votes);
}

#[test]
fn candidate_approved_for_different_forks() {
	let validator_idx0 = ValidatorIndex(0);
	let validator_idx1 = ValidatorIndex(1);

	let candidate_hash: CandidateHash = CandidateHash(
		Hash::from_low_u64_be(111));

	let rb_hash_fork_0 = Hash::from_low_u64_be(132);
	let rb_hash_fork_1 = Hash::from_low_u64_be(231);

	let view = test_harness(|mut virtual_overseer| async move {
		let leaf1 = new_leaf(
			rb_hash_fork_0.clone(),
			1,
		);

		let leaf2 = new_leaf(
			rb_hash_fork_1.clone(),
			1,
		);

		activate_leaf(&mut virtual_overseer, leaf1.clone()).await;
		activate_leaf(&mut virtual_overseer, leaf2.clone()).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash,
			rb_hash_fork_0,
			vec![validator_idx1],
		).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash,
			rb_hash_fork_1,
			vec![validator_idx0],
		).await;

		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 2);

	let expected_fork_0 = vec![validator_idx1];
	assert_votes(&view, rb_hash_fork_0, candidate_hash.clone(), expected_fork_0);

	let expected_fork_1 = vec![validator_idx0];
	assert_votes(&view, rb_hash_fork_1, candidate_hash.clone(), expected_fork_1);
}

#[test]
fn candidate_approval_stats_with_no_shows() {
	let approvals_from = vec![ValidatorIndex(0), ValidatorIndex(3)];
	let no_show_validators = vec![ValidatorIndex(1), ValidatorIndex(2)];

	let rb_hash = Hash::from_low_u64_be(111);
	let candidate_hash: CandidateHash = CandidateHash(Hash::from_low_u64_be(132));

	let view = test_harness(|mut virtual_overseer| async move {
		let leaf1 = new_leaf(rb_hash.clone(), 1);
		activate_leaf(&mut virtual_overseer, leaf1.clone()).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash,
			rb_hash,
			approvals_from,
		).await;

		no_shows(
			&mut virtual_overseer,
			candidate_hash,
			rb_hash,
			no_show_validators
		).await;

		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 1);
	let expected_validators = vec![ValidatorIndex(0), ValidatorIndex(3)];
	assert_votes(&view, rb_hash, candidate_hash.clone(), expected_validators);
}

#[test]
fn note_chunks_downloaded() {
	let candidate_hash = CandidateHash(Hash::from_low_u64_be(132));
	let session_idx: SessionIndex = 2 ;
	let chunk_downloads = vec![
		(ValidatorIndex(0), 10u64),
		(ValidatorIndex(1), 2),
	];

	let view = test_harness(|mut virtual_overseer| async move {
		virtual_overseer.send(FromOrchestra::Communication {
			msg: ConsensusStatisticsCollectorMessage::ChunksDownloaded(
				session_idx, candidate_hash.clone(), HashMap::from_iter(chunk_downloads.clone().into_iter()),
			),
		}).await;

		// should increment only validator 0
		let second_round_of_downloads = vec![
			(ValidatorIndex(0), 5u64)
		];
		virtual_overseer.send(FromOrchestra::Communication {
			msg: ConsensusStatisticsCollectorMessage::ChunksDownloaded(
				session_idx, candidate_hash.clone(), HashMap::from_iter(second_round_of_downloads.into_iter()),
			),
		}).await;

		virtual_overseer
	});

	assert_eq!(view.availability_chunks.len(), 1);
	let ac = view.availability_chunks.get(&session_idx).unwrap();

	assert_eq!(ac.downloads_per_candidate.len(), 1);
	let amt_per_validator = ac.downloads_per_candidate
		.get(&candidate_hash)
		.unwrap();

	let expected = vec![
		(ValidatorIndex(0), 15u64),
		(ValidatorIndex(1), 2),
	];

	for (vidx, expected_count) in expected {
		let count = amt_per_validator.get(&vidx).unwrap();
		assert_eq!(*count, expected_count);
	}
}

fn default_header() -> Header {
	Header {
		parent_hash: Hash::zero(),
		number: 100500,
		state_root: Hash::zero(),
		extrinsics_root: Hash::zero(),
		digest: Default::default(),
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
	let leaf1 = new_leaf(activated_leaf_hash.clone(), 1);
	let leaf1_header = default_header();
	let session_index: SessionIndex = 2;
	let mut session_info: SessionInfo = default_session_info(session_index);

	let validator_idx_pair = AuthorityDiscoveryPair::generate();
	let validator_idx_auth_id: AuthorityDiscoveryId = validator_idx_pair.0.public().into();

	session_info.discovery_keys = vec![
		validator_idx_auth_id.clone(),
	];

	let candidate_hash: CandidateHash = CandidateHash(Hash::from_low_u64_be(132));

	let view = test_harness(|mut virtual_overseer| async move {
		virtual_overseer.send(FromOrchestra::Signal(
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate{
				activated: Some(leaf1),
				deactivated: Default::default(),
			}),
		)).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(
				ChainApiMessage::BlockHeader(relay_hash, tx)
			) if relay_hash == activated_leaf_hash => {
				tx.send(Ok(Some(leaf1_header))).unwrap();
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

		// given that session index is not cached yet
		// the subsystem will retrieve the session info
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionInfo(req_session, tx))
			) if req_session == session_index => {
				tx.send(Ok(Some(session_info))).unwrap();
			}
		);

		virtual_overseer.send(FromOrchestra::Communication {
			msg: ConsensusStatisticsCollectorMessage::ChunkUploaded(
				candidate_hash.clone(), HashSet::from_iter(vec![validator_idx_auth_id.clone()]),
			),
		}).await;

		virtual_overseer
	});

	let validator_idx_auth_id: AuthorityDiscoveryId = validator_idx_pair.0.public().into();

	// assert that the leaf was activated and the session info is present
	let expected_view = PerSessionView::new(
		HashMap::from_iter(vec![(validator_idx_auth_id.clone(), ValidatorIndex(0))]));

	assert_eq!(view.per_session.len(),1);
	assert_eq!(view.per_session.get(&2).unwrap().clone(), expected_view);

	assert_matches!(view.availability_chunks.len(), 1);

	let mut expected_av_chunks = AvailabilityChunks::new();
	expected_av_chunks.note_candidate_chunk_uploaded(
		candidate_hash, ValidatorIndex(0), 1);

	assert_matches!(view.availability_chunks.get(&2).unwrap(), expected_av_chunks);
}
