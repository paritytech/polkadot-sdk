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
use polkadot_node_subsystem::messages::{
	AllMessages, ChainApiResponseChannel, RewardsStatisticsCollectorMessage, RuntimeApiMessage,
	RuntimeApiRequest,
};
use polkadot_primitives::{AssignmentId, GroupIndex, SessionIndex, SessionInfo};
use std::ptr::hash;

type VirtualOverseer = polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<
	RewardsStatisticsCollectorMessage,
>;
use polkadot_node_subsystem::ActivatedLeaf;
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_primitives::{Hash, Header};
use sp_application_crypto::Pair as PairT;
use sp_authority_discovery::AuthorityPair as AuthorityDiscoveryPair;
use test_helpers::mock::new_leaf;

async fn activate_leaf(
	virtual_overseer: &mut VirtualOverseer,
	activated: ActivatedLeaf,
	leaf_header: Header,
	session_index: SessionIndex,
	session_info: Option<SessionInfo>,
) {
	let activated_leaf_hash = activated.hash;
	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			activated,
		))))
		.await;

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ChainApi(
			ChainApiMessage::BlockHeader(relay_hash, tx)
		) if relay_hash == activated_leaf_hash => {
			tx.send(Ok(Some(leaf_header))).unwrap();
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
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionInfo(req_session, tx))
			) if req_session == session_index => {
				tx.send(Ok(Some(session_info))).unwrap();
			}
		);
	}
}

async fn finalize_block(
	virtual_overseer: &mut VirtualOverseer,
	finalized: (Hash, BlockNumber),
	session_index: SessionIndex,
) {
	let fin_block_hash = finalized.0;
	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::BlockFinalized(fin_block_hash, finalized.1)))
		.await;
}

async fn candidate_approved(
	virtual_overseer: &mut VirtualOverseer,
	candidate_hash: CandidateHash,
	rb_hash: Hash,
	approvals: Vec<ValidatorIndex>,
) {
	let msg = FromOrchestra::Communication {
		msg: RewardsStatisticsCollectorMessage::CandidateApproved(
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
		msg: RewardsStatisticsCollectorMessage::NoShows(
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
			let collected =
				approvals_for.$field.clone().into_iter().collect::<Vec<ValidatorIndex>>();

			assert_eq!(collected.len(), expected_votes.len());
			for item in collected {
				assert!(expected_votes.contains(&item));
			}
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

	let (mut context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, view, &Metrics(None)).await {
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
}

#[test]
fn single_candidate_approved() {
	let validator_idx = ValidatorIndex(2);
	let candidate_hash: CandidateHash = CandidateHash(Hash::from_low_u64_be(111));

	let rb_hash = Hash::from_low_u64_be(132);
	let leaf = new_leaf(rb_hash.clone(), 1);

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		activate_leaf(
			&mut virtual_overseer,
			leaf.clone(),
			default_header(),
			1,
			Some(default_session_info(1)),
		)
		.await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash.clone(),
			rb_hash,
			vec![validator_idx.clone()],
		)
		.await;
		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 1);
	let stats_for = view.per_relay.get(&rb_hash).unwrap();
	let approvals_for = stats_for.approvals_stats.get(&candidate_hash).unwrap();

	let expected_votes = vec![validator_idx];
	let collected_votes = approvals_for.clone().votes.into_iter().collect::<Vec<ValidatorIndex>>();

	assert_eq!(expected_votes, collected_votes);
}

#[test]
fn candidate_approved_for_different_forks() {
	let validator_idx0 = ValidatorIndex(0);
	let validator_idx1 = ValidatorIndex(1);

	let candidate_hash: CandidateHash = CandidateHash(Hash::from_low_u64_be(111));

	let rb_hash_fork_0 = Hash::from_low_u64_be(132);
	let rb_hash_fork_1 = Hash::from_low_u64_be(231);

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		let leaf0 = new_leaf(rb_hash_fork_0.clone(), 1);

		let leaf1 = new_leaf(rb_hash_fork_1.clone(), 1);

		activate_leaf(
			&mut virtual_overseer,
			leaf0.clone(),
			default_header(),
			1,
			Some(default_session_info(1)),
		)
		.await;

		activate_leaf(&mut virtual_overseer, leaf1.clone(), default_header(), 1, None).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash,
			rb_hash_fork_0,
			vec![validator_idx1],
		)
		.await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash,
			rb_hash_fork_1,
			vec![validator_idx0],
		)
		.await;

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

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		let leaf1 = new_leaf(rb_hash.clone(), 1);
		activate_leaf(
			&mut virtual_overseer,
			leaf1.clone(),
			default_header(),
			1,
			Some(default_session_info(1)),
		)
		.await;

		candidate_approved(&mut virtual_overseer, candidate_hash, rb_hash, approvals_from).await;

		no_shows(&mut virtual_overseer, candidate_hash, rb_hash, no_show_validators).await;

		virtual_overseer
	});

	assert_eq!(view.per_relay.len(), 1);
	let expected_validators = vec![ValidatorIndex(0), ValidatorIndex(3)];
	assert_votes(&view, rb_hash, candidate_hash.clone(), expected_validators);
}

#[test]
fn note_chunks_downloaded() {
	let candidate_hash = CandidateHash(Hash::from_low_u64_be(132));
	let session_idx: SessionIndex = 2;
	let chunk_downloads = vec![(ValidatorIndex(0), 10u64), (ValidatorIndex(1), 2)];

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: RewardsStatisticsCollectorMessage::ChunksDownloaded(
					session_idx,
					candidate_hash.clone(),
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
					candidate_hash.clone(),
					HashMap::from_iter(second_round_of_downloads.into_iter()),
				),
			})
			.await;

		virtual_overseer
	});

	assert_eq!(view.availability_chunks.len(), 1);
	let ac = view.availability_chunks.get(&session_idx).unwrap();

	assert_eq!(ac.downloads_per_candidate.len(), 1);
	let amt_per_validator = ac.downloads_per_candidate.get(&candidate_hash).unwrap();

	let expected = vec![(ValidatorIndex(0), 15u64), (ValidatorIndex(1), 2)];

	for (vidx, expected_count) in expected {
		let count = amt_per_validator.get(&vidx).unwrap();
		assert_eq!(*count, expected_count);
	}
}

fn default_header() -> Header {
	Header {
		parent_hash: Hash::zero(),
		number: 1,
		state_root: Hash::zero(),
		extrinsics_root: Hash::zero(),
		digest: Default::default(),
	}
}

fn header_with_number_and_parent(block_number: BlockNumber, parent_hash: Hash) -> Header {
	let mut header = default_header();
	header.number = block_number;
	header.parent_hash = parent_hash;
	header
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

	session_info.discovery_keys = vec![validator_idx_auth_id.clone()];

	let candidate_hash: CandidateHash = CandidateHash(Hash::from_low_u64_be(132));

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		activate_leaf(
			&mut virtual_overseer,
			leaf1,
			leaf1_header,
			session_index,
			Some(session_info),
		)
		.await;

		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: RewardsStatisticsCollectorMessage::ChunkUploaded(
					candidate_hash.clone(),
					HashSet::from_iter(vec![validator_idx_auth_id.clone()]),
				),
			})
			.await;

		virtual_overseer
	});

	let validator_idx_auth_id: AuthorityDiscoveryId = validator_idx_pair.0.public().into();

	// assert that the leaf was activated and the session info is present
	let expected_view = PerSessionView::new(HashMap::from_iter(vec![(
		validator_idx_auth_id.clone(),
		ValidatorIndex(0),
	)]));

	assert_eq!(view.per_session.len(), 1);
	assert_eq!(view.per_session.get(&2).unwrap().clone(), expected_view);

	assert_matches!(view.availability_chunks.len(), 1);

	let mut expected_av_chunks = AvailabilityChunks::new();
	expected_av_chunks.note_candidate_chunk_uploaded(candidate_hash, ValidatorIndex(0), 1);

	assert_matches!(view.availability_chunks.get(&2).unwrap(), expected_av_chunks);
}

#[test]
fn prune_unfinalized_forks() {
	// testing pruning capabilities
	// the pruning happens when a session is finalized
	// means that all the collected data for the finalized session
	// should be kept and the collected data that belongs to unfinalized
	// should be pruned

	// Building a "chain" with the following relay blocks (all in the same session)
	// A -> B
	// A -> C -> D

	let hash_a = Hash::from_slice(&[00; 32]);
	let hash_b = Hash::from_slice(&[01; 32]);
	let hash_c = Hash::from_slice(&[02; 32]);
	let hash_d = Hash::from_slice(&[03; 32]);
	let session_zero: SessionIndex = 0;

	let candidate_hash_a: CandidateHash = CandidateHash(Hash::from_low_u64_be(100));
	let candidate_hash_b: CandidateHash = CandidateHash(Hash::from_low_u64_be(200));
	let candidate_hash_c: CandidateHash = CandidateHash(Hash::from_low_u64_be(300));
	let candidate_hash_d: CandidateHash = CandidateHash(Hash::from_low_u64_be(400));

	let mut view = View::new();
	test_harness(&mut view, |mut virtual_overseer| async move {
		let leaf_a = new_leaf(hash_a.clone(), 1);
		let leaf_a_header = default_header();

		activate_leaf(
			&mut virtual_overseer,
			leaf_a,
			leaf_a_header,
			session_zero,
			Some(default_session_info(session_zero)),
		)
		.await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash_a,
			hash_a,
			vec![ValidatorIndex(2), ValidatorIndex(3)],
		)
		.await;
		no_shows(
			&mut virtual_overseer,
			candidate_hash_a,
			hash_a,
			vec![ValidatorIndex(0), ValidatorIndex(1)],
		)
		.await;

		let leaf_b = new_leaf(hash_b.clone(), 2);
		let leaf_b_header = header_with_number_and_parent(2, hash_a.clone());

		activate_leaf(&mut virtual_overseer, leaf_b, leaf_b_header, session_zero, None).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash_b,
			hash_b,
			vec![ValidatorIndex(0), ValidatorIndex(1)],
		)
		.await;

		let leaf_c = new_leaf(hash_c.clone(), 2);
		let leaf_c_header = header_with_number_and_parent(2, hash_a.clone());

		activate_leaf(&mut virtual_overseer, leaf_c, leaf_c_header, session_zero, None).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash_c,
			hash_c,
			vec![ValidatorIndex(0), ValidatorIndex(1), ValidatorIndex(2)],
		)
		.await;

		let leaf_d = new_leaf(hash_d.clone(), 3);
		let leaf_d_header = header_with_number_and_parent(3, hash_c.clone());

		activate_leaf(&mut virtual_overseer, leaf_d, leaf_d_header, session_zero, None).await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash_d,
			hash_d,
			vec![ValidatorIndex(0), ValidatorIndex(1)],
		)
		.await;

		virtual_overseer
	});

	let expect = vec![
		// relay node A should have 2 children (B, C)
		(hash_a.clone(), (None, vec![hash_b.clone(), hash_c.clone()])),
		//  relay node B should link to A and have no children
		(hash_b.clone(), (Some(hash_a.clone()), vec![])),
		//  relay node C should link to A and have 1 child (D)
		(hash_c.clone(), (Some(hash_a.clone()), vec![hash_d.clone()])),
		//  relay node D should link to C and have no children
		(hash_d.clone(), (Some(hash_c.clone()), vec![])),
	];

	// relay node A should be the root
	assert_roots_and_relay_views(&view, vec![hash_a], expect.clone());

	// Finalizing block C should prune the current unfinalized mapping
	// and aggregate data of the finalized chain on the per session view
	// the collected data for block D should remain untouched
	test_harness(&mut view, |mut virtual_overseer| async move {
		finalize_block(&mut virtual_overseer, (hash_c.clone(), 2), session_zero).await;

		virtual_overseer
	});

	let expect = vec![
		//  relay node D should link to C and have no children
		(hash_d.clone(), (Some(hash_c.clone()), vec![])),
	];

	assert_roots_and_relay_views(&view, vec![hash_d], expect.clone());

	// check if the data was aggregated correctly for the session view
	// it should aggregat approvals and noshows collected on blocks
	// A and C.
	// Data collected on block B should be discarded
	// Data collected on block D should remain in the mapping as it was not finalized or pruned
	let expected_tallies = HashMap::from_iter(vec![
		(ValidatorIndex(0), PerValidatorTally { no_shows: 1, approvals: 1 }),
		(ValidatorIndex(1), PerValidatorTally { no_shows: 1, approvals: 1 }),
		(ValidatorIndex(2), PerValidatorTally { no_shows: 0, approvals: 2 }),
		(ValidatorIndex(3), PerValidatorTally { no_shows: 0, approvals: 1 }),
	]);

	assert_per_session_tallies(&view.per_session, 0, expected_tallies);
	// creating more 3 relay block (E, F, G), all in session 1
	// D -> E -> F
	//        -> G

	let hash_e = Hash::from_slice(&[04; 32]);
	let hash_f = Hash::from_slice(&[05; 32]);
	let hash_g = Hash::from_slice(&[06; 32]);

	let candidate_hash_e = CandidateHash(Hash::from_low_u64_be(0xEE0011));
	let session_one: SessionIndex = 1;

	test_harness(&mut view, |mut virtual_overseer| async move {
		let leaf_e = new_leaf(hash_e.clone(), 4);
		let leaf_e_header = header_with_number_and_parent(4, hash_d.clone());

		activate_leaf(
			&mut virtual_overseer,
			leaf_e,
			leaf_e_header,
			session_one,
			Some(default_session_info(session_one)),
		)
		.await;

		candidate_approved(
			&mut virtual_overseer,
			candidate_hash_e,
			hash_e,
			vec![ValidatorIndex(3), ValidatorIndex(1), ValidatorIndex(0)],
		)
		.await;
		no_shows(&mut virtual_overseer, candidate_hash_e, hash_e, vec![ValidatorIndex(2)]).await;

		let leaf_f = new_leaf(hash_f.clone(), 5);
		let leaf_f_header = header_with_number_and_parent(5, hash_e.clone());

		activate_leaf(&mut virtual_overseer, leaf_f, leaf_f_header, session_one, None).await;

		let leaf_g = new_leaf(hash_g.clone(), 5);
		let leaf_g_header = header_with_number_and_parent(5, hash_e.clone());

		activate_leaf(&mut virtual_overseer, leaf_g, leaf_g_header, session_one, None).await;

		// finalizing relay block E
		finalize_block(&mut virtual_overseer, (hash_e.clone(), 4), session_one).await;

		virtual_overseer
	});

	// Finalizing block E triggers the pruning mechanism
	// now it should aggregate collected data from block D and E
	// keeping only blocks F and E on the mapping
	let expect = vec![
		// relay node F should link to E and have no children
		(hash_f.clone(), (Some(hash_e), vec![])),
		//  relay node G should link to E and have no children
		(hash_g.clone(), (Some(hash_e), vec![])),
	];

	// relay node A should be the root
	assert_roots_and_relay_views(&view, vec![hash_f, hash_g], expect.clone());

	let expected_tallies = HashMap::from_iter(vec![
		(
			ValidatorIndex(0),
			PerValidatorTally {
				no_shows: 1,
				// validator 0 approvals increased from 1 to 2
				// as block D with more collected approvals
				// was finalized
				approvals: 2,
			},
		),
		(ValidatorIndex(1), PerValidatorTally { no_shows: 1, approvals: 2 }),
		(ValidatorIndex(2), PerValidatorTally { no_shows: 0, approvals: 2 }),
		(ValidatorIndex(3), PerValidatorTally { no_shows: 0, approvals: 1 }),
	]);

	assert_per_session_tallies(&view.per_session, 0, expected_tallies);

	let expected_tallies = HashMap::from_iter(vec![
		(ValidatorIndex(0), PerValidatorTally { no_shows: 0, approvals: 1 }),
		(ValidatorIndex(1), PerValidatorTally { no_shows: 0, approvals: 1 }),
		(ValidatorIndex(2), PerValidatorTally { no_shows: 1, approvals: 0 }),
		(ValidatorIndex(3), PerValidatorTally { no_shows: 0, approvals: 1 }),
	]);

	assert_per_session_tallies(&view.per_session, 1, expected_tallies);
}

fn assert_roots_and_relay_views(
	view: &View,
	roots: Vec<Hash>,
	relay_views: Vec<(Hash, (Option<Hash>, Vec<Hash>))>,
) {
	assert_eq!(view.roots, HashSet::from_iter(roots));
	assert_eq!(view.per_relay.len(), relay_views.len());

	for (rb_hash, checks) in relay_views.into_iter() {
		let rb_view = view.per_relay.get(&rb_hash).unwrap();
		assert_eq!(rb_view.parent_hash, checks.0);
		assert_eq!(rb_view.children.len(), checks.1.len());

		for child in checks.1.iter() {
			assert!(rb_view.children.contains(child));
		}
	}
}

fn assert_per_session_tallies(
	per_session_view: &HashMap<SessionIndex, PerSessionView>,
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
