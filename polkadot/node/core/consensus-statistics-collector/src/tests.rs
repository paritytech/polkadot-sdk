use super::*;
use overseer::FromOrchestra;
use polkadot_node_subsystem::messages::{ConsensusStatisticsCollectorMessage};
type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<ConsensusStatisticsCollectorMessage>;
use polkadot_node_subsystem::{ActivatedLeaf};
use polkadot_node_subsystem_test_helpers as test_helpers;
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

fn test_harness<T: Future<Output = VirtualOverseer>>(
	test: impl FnOnce(VirtualOverseer) -> T,
) -> View {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();

	let (mut context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

    let metrics = Metrics;

	let mut view = View::new();
	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, &mut view, &metrics).await {
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
	assert_votes_for(&view, rb_hash_fork_0, candidate_hash.clone(), expected_fork_0);

	let expected_fork_1 = vec![validator_idx0];
	assert_votes_for(&view, rb_hash_fork_1, candidate_hash.clone(), expected_fork_1);
}

fn assert_votes_for(view: &View, rb_hash: Hash, candidate_hash: CandidateHash, expected_votes: Vec<ValidatorIndex>) {
	let stats_for = view.per_relay.get(&rb_hash).unwrap();
	let approvals_for = stats_for.approvals_stats.get(&candidate_hash).unwrap();
	let collected_votes= approvals_for
		.clone()
		.votes
		.into_iter()
		.collect::<Vec<ValidatorIndex>>();

	assert_eq!(expected_votes, collected_votes);
}