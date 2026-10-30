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

//! The tests for Approval Voting Parallel Subsystem.

use std::{
	collections::{HashMap, HashSet},
	future::Future,
	sync::Arc,
	time::Duration,
};

use crate::{
	build_worker_handles, metrics::MetricsWatcher, prio_right, run_main_loop, start_workers,
	validator_index_for_msg, ApprovalVotingParallelSubsystem, Metrics, WorkProvider,
};
use assert_matches::assert_matches;
use futures::{channel::oneshot, future, stream::PollNext, StreamExt};
use itertools::Itertools;
use polkadot_node_core_approval_voting::{ApprovalVotingWorkProvider, Config};
use polkadot_node_network_protocol::{peer_set::ValidationVersion, ObservedRole, PeerId, View};
use polkadot_node_primitives::approval::{
	time::SystemClock,
	v1::RELAY_VRF_MODULO_CONTEXT,
	v2::{
		AssignmentCertKindV2, AssignmentCertV2, CoreBitfield, IndirectAssignmentCertV2,
		IndirectSignedApprovalVoteV2,
	},
};
use polkadot_node_subsystem::{
	messages::{ApprovalDistributionMessage, ApprovalVotingMessage, ApprovalVotingParallelMessage},
	FromOrchestra,
};
use polkadot_node_subsystem_test_helpers::{mock::new_leaf, TestSubsystemContext};
use polkadot_overseer::{ActiveLeavesUpdate, OverseerSignal, SpawnGlue, TimeoutExt};
use polkadot_primitives::{CandidateHash, CoreIndex, Hash, ValidatorIndex};
use sc_keystore::{Keystore, LocalKeystore};
use sp_consensus::SyncOracle;
use sp_consensus_babe::{VrfPreOutput, VrfProof, VrfSignature};
use sp_core::{testing::TaskExecutor, H256};
use sp_keyring::Sr25519Keyring;
type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<ApprovalVotingParallelMessage>;

const SLOT_DURATION_MILLIS: u64 = 6000;

pub mod test_constants {
	pub(crate) const DATA_COL: u32 = 0;
	pub(crate) const NUM_COLUMNS: u32 = 1;
}

fn fake_assignment_cert_v2(
	block_hash: Hash,
	validator: ValidatorIndex,
	core_bitfield: CoreBitfield,
) -> IndirectAssignmentCertV2 {
	let ctx = schnorrkel::signing_context(RELAY_VRF_MODULO_CONTEXT);
	let msg = b"WhenParachains?";
	let mut prng = rand_core::OsRng;
	let keypair = schnorrkel::Keypair::generate_with(&mut prng);
	let (inout, proof, _) = keypair.vrf_sign(ctx.bytes(msg));
	let preout = inout.to_preout();

	IndirectAssignmentCertV2 {
		block_hash,
		validator,
		cert: AssignmentCertV2 {
			kind: AssignmentCertKindV2::RelayVRFModuloCompact { core_bitfield },
			vrf: VrfSignature { pre_output: VrfPreOutput(preout), proof: VrfProof(proof) },
		},
	}
}

/// Creates a meaningless signature
pub fn dummy_signature() -> polkadot_primitives::ValidatorSignature {
	sp_core::crypto::UncheckedFrom::unchecked_from([1u8; 64])
}

fn build_subsystem(
	sync_oracle: Box<dyn SyncOracle + Send>,
) -> (
	ApprovalVotingParallelSubsystem,
	TestSubsystemContext<ApprovalVotingParallelMessage, SpawnGlue<TaskExecutor>>,
	VirtualOverseer,
) {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();
	let (context, virtual_overseer) = polkadot_node_subsystem_test_helpers::make_subsystem_context::<
		ApprovalVotingParallelMessage,
		_,
	>(pool.clone());

	let keystore = LocalKeystore::in_memory();
	let _ = keystore.sr25519_generate_new(
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	);

	let clock = Arc::new(SystemClock {});
	let db = kvdb_memorydb::create(test_constants::NUM_COLUMNS);
	let db = polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[]);

	(
		ApprovalVotingParallelSubsystem::with_config_and_clock(
			Config {
				col_approval_data: test_constants::DATA_COL,
				slot_duration_millis: SLOT_DURATION_MILLIS,
			},
			Arc::new(db),
			Arc::new(keystore),
			sync_oracle,
			Metrics::default(),
			clock.clone(),
			SpawnGlue(pool),
			None,
		),
		context,
		virtual_overseer,
	)
}

#[derive(Clone)]
struct TestSyncOracle {}

impl SyncOracle for TestSyncOracle {
	fn is_major_syncing(&self) -> bool {
		false
	}

	fn is_offline(&self) -> bool {
		unimplemented!("not used in network bridge")
	}
}

fn test_harness<T, Clos, State>(
	num_approval_distro_workers: usize,
	prio_right: Clos,
	subsystem_gracefully_exits: bool,
	test_fn: impl FnOnce(
		VirtualOverseer,
		WorkProvider<ApprovalVotingMessage, Clos, State>,
		Vec<WorkProvider<ApprovalDistributionMessage, Clos, State>>,
	) -> T,
) where
	T: Future<Output = VirtualOverseer>,
	Clos: Clone + FnMut(&mut State) -> PollNext,
	State: Default,
{
	let (subsystem, context, virtual_overseer) = build_subsystem(Box::new(TestSyncOracle {}));
	let mut metrics_watcher = MetricsWatcher::new(subsystem.metrics.clone());
	let channel_size = 5;

	let (to_approval_voting_worker, approval_voting_work_provider) =
		build_worker_handles::<ApprovalVotingMessage, _, _>(
			"to_approval_voting_worker".into(),
			channel_size,
			&mut metrics_watcher,
			prio_right.clone(),
		);

	let approval_distribution_channels = { 0..num_approval_distro_workers }
		.into_iter()
		.map(|worker_index| {
			build_worker_handles::<ApprovalDistributionMessage, _, _>(
				format!("to_approval_distro/{}", worker_index),
				channel_size,
				&mut metrics_watcher,
				prio_right.clone(),
			)
		})
		.collect_vec();

	let to_approval_distribution_workers =
		approval_distribution_channels.iter().map(|(tx, _)| tx.clone()).collect_vec();
	let approval_distribution_work_providers =
		approval_distribution_channels.into_iter().map(|(_, rx)| rx).collect_vec();

	let subsystem = async move {
		let result = run_main_loop(
			context,
			to_approval_voting_worker,
			to_approval_distribution_workers,
			metrics_watcher,
		)
		.await;

		if subsystem_gracefully_exits && result.is_err() {
			result
		} else {
			Ok(())
		}
	};

	let test_fut = test_fn(
		virtual_overseer,
		approval_voting_work_provider,
		approval_distribution_work_providers,
	);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);

	futures::executor::block_on(future::join(
		async move {
			let _overseer = test_fut.await;
		},
		subsystem,
	))
	.1
	.unwrap();
}

const TIMEOUT: Duration = Duration::from_millis(2000);

async fn overseer_signal(overseer: &mut VirtualOverseer, signal: OverseerSignal) {
	overseer
		.send(FromOrchestra::Signal(signal))
		.timeout(TIMEOUT)
		.await
		.expect(&format!("{:?} is more than enough for sending signals.", TIMEOUT));
}

async fn overseer_message(overseer: &mut VirtualOverseer, msg: ApprovalVotingParallelMessage) {
	overseer
		.send(FromOrchestra::Communication { msg })
		.timeout(TIMEOUT)
		.await
		.expect(&format!("{:?} is more than enough for sending signals.", TIMEOUT));
}

async fn run_start_workers() {
	let (subsystem, mut context, _) = build_subsystem(Box::new(TestSyncOracle {}));
	let mut metrics_watcher = MetricsWatcher::new(subsystem.metrics.clone());
	let _workers = start_workers(&mut context, subsystem, &mut metrics_watcher).await.unwrap();
}

// Test starting the workers succeeds.
#[test]
fn start_workers_succeeds() {
	futures::executor::block_on(run_start_workers());
}

// Test main loop forwards messages to the correct worker for all type of messages.
#[test]
fn test_main_loop_forwards_correctly() {
	let num_approval_distro_workers = 4;
	test_harness(
		num_approval_distro_workers,
		prio_right,
		true,
		|mut overseer, mut approval_voting_work_provider, mut rx_approval_distribution_workers| async move {
			// 1. Check Signals are correctly forwarded to the workers.
			let signal = OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				Hash::random(),
				1,
			)));
			overseer_signal(&mut overseer, signal.clone()).await;
			let approval_voting_receives = approval_voting_work_provider.recv().await.unwrap();
			assert_matches!(approval_voting_receives, FromOrchestra::Signal(_));
			for rx_approval_distribution_worker in rx_approval_distribution_workers.iter_mut() {
				let approval_distribution_receives =
					rx_approval_distribution_worker.next().await.unwrap();
				assert_matches!(approval_distribution_receives, FromOrchestra::Signal(_));
			}

			let (test_tx, _rx) = oneshot::channel();
			let test_hash = Hash::random();
			let test_block_nr = 2;
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::ApprovedAncestor(test_hash, test_block_nr, test_tx),
			)
			.await;
			assert_matches!(
				approval_voting_work_provider.recv().await.unwrap(),
				FromOrchestra::Communication {
					msg: ApprovalVotingMessage::ApprovedAncestor(hash, block_nr, _)
				} => {
					assert_eq!(hash, test_hash);
					assert_eq!(block_nr, test_block_nr);
				}
			);
			for rx_approval_distribution_worker in rx_approval_distribution_workers.iter_mut() {
				assert!(rx_approval_distribution_worker
					.next()
					.timeout(Duration::from_millis(200))
					.await
					.is_none());
			}

			// 2. Check GetApprovalSignaturesForCandidate is correctly forwarded to the workers.
			let (test_tx, _rx) = oneshot::channel();
			let test_hash = CandidateHash(Hash::random());
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::GetApprovalSignaturesForCandidate(
					test_hash, test_tx,
				),
			)
			.await;

			assert_matches!(
				approval_voting_work_provider.recv().await.unwrap(),
				FromOrchestra::Communication {
					msg: ApprovalVotingMessage::GetApprovalSignaturesForCandidate(hash, _)
				} => {
					assert_eq!(hash, test_hash);
				}
			);

			for rx_approval_distribution_worker in rx_approval_distribution_workers.iter_mut() {
				assert!(rx_approval_distribution_worker
					.next()
					.timeout(Duration::from_millis(200))
					.await
					.is_none());
			}

			// 3. Check NewBlocks is correctly forwarded to the workers.
			overseer_message(&mut overseer, ApprovalVotingParallelMessage::NewBlocks(vec![])).await;
			for rx_approval_distribution_worker in rx_approval_distribution_workers.iter_mut() {
				assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
					FromOrchestra::Communication {
						msg: ApprovalDistributionMessage::NewBlocks(blocks)
					} => {
						assert!(blocks.is_empty());
					}
				);
			}
			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			// 4. Check DistributeAssignment is correctly forwarded to the workers.
			let validator_index = ValidatorIndex(17);
			let assignment =
				fake_assignment_cert_v2(Hash::random(), validator_index, CoreIndex(1).into());
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::DistributeAssignment(assignment.clone(), 1.into()),
			)
			.await;

			for (index, rx_approval_distribution_worker) in
				rx_approval_distribution_workers.iter_mut().enumerate()
			{
				if index == validator_index.0 as usize % num_approval_distro_workers {
					assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
						FromOrchestra::Communication {
							msg: ApprovalDistributionMessage::DistributeAssignment(cert, bitfield)
						} => {
							assert_eq!(cert, assignment);
							assert_eq!(bitfield, 1.into());
						}
					);
				} else {
					assert!(rx_approval_distribution_worker
						.next()
						.timeout(Duration::from_millis(200))
						.await
						.is_none());
				}
			}
			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			// 5. Check DistributeApproval is correctly forwarded to the workers.
			let validator_index = ValidatorIndex(26);
			let expected_vote = IndirectSignedApprovalVoteV2 {
				block_hash: H256::random(),
				candidate_indices: 1.into(),
				validator: validator_index,
				signature: dummy_signature(),
			};

			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::DistributeApproval(expected_vote.clone()),
			)
			.await;

			for (index, rx_approval_distribution_worker) in
				rx_approval_distribution_workers.iter_mut().enumerate()
			{
				if index == validator_index.0 as usize % num_approval_distro_workers {
					assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
						FromOrchestra::Communication {
							msg: ApprovalDistributionMessage::DistributeApproval(vote)
						} => {
							assert_eq!(vote, expected_vote);
						}
					);
				} else {
					assert!(rx_approval_distribution_worker
						.next()
						.timeout(Duration::from_millis(200))
						.await
						.is_none());
				}
			}

			// 6. Check NetworkBridgeUpdate::PeerMessage is correctly forwarded just to one of the
			//    workers.
			let approvals = vec![
				IndirectSignedApprovalVoteV2 {
					block_hash: H256::random(),
					candidate_indices: 1.into(),
					validator: validator_index,
					signature: dummy_signature(),
				},
				IndirectSignedApprovalVoteV2 {
					block_hash: H256::random(),
					candidate_indices: 2.into(),
					validator: validator_index,
					signature: dummy_signature(),
				},
			];
			let expected_msg = polkadot_node_network_protocol::ValidationProtocols::V3(
				polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(
					approvals.clone(),
				),
			);
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::NetworkBridgeUpdate(
					polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
						PeerId::random(),
						expected_msg.clone(),
					),
				),
			)
			.await;

			for (index, rx_approval_distribution_worker) in
				rx_approval_distribution_workers.iter_mut().enumerate()
			{
				if index == validator_index.0 as usize % num_approval_distro_workers {
					assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
						FromOrchestra::Communication {
							msg: ApprovalDistributionMessage::NetworkBridgeUpdate(
								polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
									_,
									msg,
								),
							)
						} => {
							assert_eq!(msg, expected_msg);
						}
					);
				} else {
					assert!(rx_approval_distribution_worker
						.next()
						.timeout(Duration::from_millis(200))
						.await
						.is_none());
				}
			}
			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			// 7. Check NetworkBridgeUpdate::PeerConnected is correctly forwarded to all workers.
			let expected_peer_id = PeerId::random();
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::NetworkBridgeUpdate(
					polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerConnected(
						expected_peer_id,
						ObservedRole::Authority,
						ValidationVersion::V3.into(),
						None,
					),
				),
			)
			.await;

			for rx_approval_distribution_worker in rx_approval_distribution_workers.iter_mut() {
				assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
					FromOrchestra::Communication {
						msg: ApprovalDistributionMessage::NetworkBridgeUpdate(
							polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerConnected(
								peer_id,
								role,
								version,
								authority_id,
							),
						)
					} => {
						assert_eq!(peer_id, expected_peer_id);
						assert_eq!(role, ObservedRole::Authority);
						assert_eq!(version, ValidationVersion::V3.into());
						assert_eq!(authority_id, None);
					}
				);
			}
			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			// 8. Check ApprovalCheckingLagUpdate is correctly forwarded to all workers.
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::ApprovalCheckingLagUpdate(7),
			)
			.await;

			for rx_approval_distribution_worker in rx_approval_distribution_workers.iter_mut() {
				assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
					FromOrchestra::Communication {
						msg: ApprovalDistributionMessage::ApprovalCheckingLagUpdate(
							lag
						)
					} => {
						assert_eq!(lag, 7);
					}
				);
			}
			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;

			overseer
		},
	);
}

/// Test GetApprovalSignatures correctly gatheres the signatures from all workers.
#[test]
fn test_handle_get_approval_signatures() {
	let num_approval_distro_workers = 4;

	test_harness(
		num_approval_distro_workers,
		prio_right,
		true,
		|mut overseer, mut approval_voting_work_provider, mut rx_approval_distribution_workers| async move {
			let (tx, rx) = oneshot::channel();
			let first_block = Hash::random();
			let second_block = Hash::random();
			let expected_candidates: HashSet<_> =
				vec![(first_block, 2), (second_block, 3)].into_iter().collect();

			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::GetApprovalSignatures(
					expected_candidates.clone(),
					tx,
				),
			)
			.await;

			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());
			let mut all_votes = HashMap::new();
			for (index, rx_approval_distribution_worker) in
				rx_approval_distribution_workers.iter_mut().enumerate()
			{
				assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
					FromOrchestra::Communication {
						msg: ApprovalDistributionMessage::GetApprovalSignatures(
							candidates, tx
						)
					} => {
						assert_eq!(candidates, expected_candidates);
						let to_send: HashMap<_, _> = {0..10}.into_iter().map(|validator| {
							let validator_index = ValidatorIndex(validator as u32 * num_approval_distro_workers as u32 + index as u32);
							(validator_index, (first_block, vec![2, 4], dummy_signature()))
						}).collect();
						tx.send(to_send.clone()).unwrap();
						all_votes.extend(to_send.clone());

					}
				);
			}

			let received_votes = rx.await.unwrap();
			assert_eq!(received_votes, all_votes);
			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;

			overseer
		},
	)
}

/// Test subsystem exits with error when approval_voting_work_provider exits.
#[test]
fn test_subsystem_exits_with_error_if_approval_voting_worker_errors() {
	let num_approval_distro_workers = 4;

	test_harness(
		num_approval_distro_workers,
		prio_right,
		false,
		|overseer, approval_voting_work_provider, _rx_approval_distribution_workers| async move {
			// Drop the approval_voting_work_provider to simulate an error.
			std::mem::drop(approval_voting_work_provider);

			overseer
		},
	)
}

/// Test subsystem exits with error when approval_distribution_workers exits.
#[test]
fn test_subsystem_exits_with_error_if_approval_distribution_worker_errors() {
	let num_approval_distro_workers = 4;

	test_harness(
		num_approval_distro_workers,
		prio_right,
		false,
		|overseer, _approval_voting_work_provider, rx_approval_distribution_workers| async move {
			// Drop the approval_distribution_workers to simulate an error.
			std::mem::drop(rx_approval_distribution_workers.into_iter().next().unwrap());
			overseer
		},
	)
}

/// Test signals sent before messages are processed in order.
#[test]
fn test_signal_before_message_keeps_receive_order() {
	let num_approval_distro_workers = 4;

	test_harness(
		num_approval_distro_workers,
		prio_right,
		true,
		|mut overseer, mut approval_voting_work_provider, mut rx_approval_distribution_workers| async move {
			let signal = OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				Hash::random(),
				1,
			)));
			overseer_signal(&mut overseer, signal.clone()).await;

			let validator_index = ValidatorIndex(17);
			let assignment =
				fake_assignment_cert_v2(Hash::random(), validator_index, CoreIndex(1).into());
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::DistributeAssignment(assignment.clone(), 1.into()),
			)
			.await;

			let approval_voting_receives = approval_voting_work_provider.recv().await.unwrap();
			assert_matches!(approval_voting_receives, FromOrchestra::Signal(_));
			let rx_approval_distribution_worker = rx_approval_distribution_workers
				.get_mut(validator_index.0 as usize % num_approval_distro_workers)
				.unwrap();
			let approval_distribution_receives =
				rx_approval_distribution_worker.next().await.unwrap();
			assert_matches!(approval_distribution_receives, FromOrchestra::Signal(_));
			assert_matches!(
				rx_approval_distribution_worker.next().await.unwrap(),
				FromOrchestra::Communication {
					msg: ApprovalDistributionMessage::DistributeAssignment(_, _)
				}
			);

			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;
			overseer
		},
	)
}

/// Test signals sent after messages are processed with the highest priority.
#[test]
fn test_signal_is_prioritized_when_unread_messages_in_the_queue() {
	let num_approval_distro_workers = 4;

	test_harness(
		num_approval_distro_workers,
		prio_right,
		true,
		|mut overseer, mut approval_voting_work_provider, mut rx_approval_distribution_workers| async move {
			let validator_index = ValidatorIndex(17);
			let assignment =
				fake_assignment_cert_v2(Hash::random(), validator_index, CoreIndex(1).into());
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::DistributeAssignment(assignment.clone(), 1.into()),
			)
			.await;

			let signal = OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				Hash::random(),
				1,
			)));
			overseer_signal(&mut overseer, signal.clone()).await;

			let approval_voting_receives = approval_voting_work_provider.recv().await.unwrap();
			assert_matches!(approval_voting_receives, FromOrchestra::Signal(_));
			let rx_approval_distribution_worker = rx_approval_distribution_workers
				.get_mut(validator_index.0 as usize % num_approval_distro_workers)
				.unwrap();
			let approval_distribution_receives =
				rx_approval_distribution_worker.next().await.unwrap();
			assert_matches!(approval_distribution_receives, FromOrchestra::Signal(_));
			assert_matches!(
				rx_approval_distribution_worker.next().await.unwrap(),
				FromOrchestra::Communication {
					msg: ApprovalDistributionMessage::DistributeAssignment(_, _)
				}
			);

			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;
			overseer
		},
	)
}

/// Test peer view updates have higher priority than normal messages.
#[test]
fn test_peer_view_is_prioritized_when_unread_messages_in_the_queue() {
	let num_approval_distro_workers = 4;

	test_harness(
		num_approval_distro_workers,
		prio_right,
		true,
		|mut overseer, mut approval_voting_work_provider, mut rx_approval_distribution_workers| async move {
			let validator_index = ValidatorIndex(17);
			let approvals = vec![
				IndirectSignedApprovalVoteV2 {
					block_hash: H256::random(),
					candidate_indices: 1.into(),
					validator: validator_index,
					signature: dummy_signature(),
				},
				IndirectSignedApprovalVoteV2 {
					block_hash: H256::random(),
					candidate_indices: 2.into(),
					validator: validator_index,
					signature: dummy_signature(),
				},
			];
			let expected_msg = polkadot_node_network_protocol::ValidationProtocols::V3(
				polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(
					approvals.clone(),
				),
			);
			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::NetworkBridgeUpdate(
					polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
						PeerId::random(),
						expected_msg.clone(),
					),
				),
			)
			.await;

			overseer_message(
				&mut overseer,
				ApprovalVotingParallelMessage::NetworkBridgeUpdate(
					polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerViewChange(
						PeerId::random(),
						View::default(),
					),
				),
			)
			.await;

			for (index, rx_approval_distribution_worker) in
				rx_approval_distribution_workers.iter_mut().enumerate()
			{
				assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
					FromOrchestra::Communication {
						msg: ApprovalDistributionMessage::NetworkBridgeUpdate(
							polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerViewChange(
								_,
								_,
							),
						)
					} => {
					}
				);
				if index == validator_index.0 as usize % num_approval_distro_workers {
					assert_matches!(rx_approval_distribution_worker.next().await.unwrap(),
						FromOrchestra::Communication {
							msg: ApprovalDistributionMessage::NetworkBridgeUpdate(
								polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
									_,
									msg,
								),
							)
						} => {
							assert_eq!(msg, expected_msg);
						}
					);
				} else {
					assert!(rx_approval_distribution_worker
						.next()
						.timeout(Duration::from_millis(200))
						.await
						.is_none());
				}
			}

			assert!(approval_voting_work_provider
				.recv()
				.timeout(Duration::from_millis(200))
				.await
				.is_none());

			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;
			overseer
		},
	)
}

// Test validator_index_for_msg with empty messages.
#[test]
fn test_validator_index_with_empty_message() {
	let result = validator_index_for_msg(polkadot_node_network_protocol::ValidationProtocols::V3(
		polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(vec![]),
	));

	assert_eq!(result, (None, Some(vec![])));

	let result = validator_index_for_msg(polkadot_node_network_protocol::ValidationProtocols::V3(
		polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(vec![]),
	));

	assert_eq!(result, (None, Some(vec![])));
}

// Test validator_index_for_msg when all the messages are originating from the same validator.
#[test]
fn test_validator_index_with_all_messages_from_the_same_validator() {
	let validator_index = ValidatorIndex(3);
	let v3_assignment = polkadot_node_network_protocol::ValidationProtocols::V3(
		polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(vec![
			(
				fake_assignment_cert_v2(H256::random(), validator_index, CoreIndex(1).into()),
				1.into(),
			),
			(
				fake_assignment_cert_v2(H256::random(), validator_index, CoreIndex(3).into()),
				3.into(),
			),
		]),
	);
	let result = validator_index_for_msg(v3_assignment.clone());

	assert_eq!(result, (Some((validator_index, v3_assignment)), None));

	let v3_approval = polkadot_node_network_protocol::ValidationProtocols::V3(
		polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(vec![
			IndirectSignedApprovalVoteV2 {
				block_hash: H256::random(),
				candidate_indices: 1.into(),
				validator: validator_index,
				signature: dummy_signature(),
			},
			IndirectSignedApprovalVoteV2 {
				block_hash: H256::random(),
				candidate_indices: 1.into(),
				validator: validator_index,
				signature: dummy_signature(),
			},
		]),
	);
	let result = validator_index_for_msg(v3_approval.clone());

	assert_eq!(result, (Some((validator_index, v3_approval)), None));
}

// Test validator_index_for_msg when all the messages are originating from different validators,
// so the function should split them by validator index, so we can forward them separately to the
// worker they are assigned to.
#[test]
fn test_validator_index_with_messages_from_different_validators() {
	let first_validator_index = ValidatorIndex(3);
	let second_validator_index = ValidatorIndex(4);
	let assignments = vec![
		(
			fake_assignment_cert_v2(H256::random(), first_validator_index, CoreIndex(1).into()),
			1.into(),
		),
		(
			fake_assignment_cert_v2(H256::random(), second_validator_index, CoreIndex(3).into()),
			3.into(),
		),
	];

	let v3_assignment = polkadot_node_network_protocol::ValidationProtocols::V3(
		polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(
			assignments.clone(),
		),
	);
	let result = validator_index_for_msg(v3_assignment.clone());

	assert_matches!(result, (None, Some(_)));
	let messsages_split_by_validator = result.1.unwrap();
	assert_eq!(messsages_split_by_validator.len(), assignments.len());
	for (index, (validator_index, message)) in messsages_split_by_validator.into_iter().enumerate()
	{
		assert_eq!(validator_index, assignments[index].0.validator);
		assert_eq!(
			message,
			polkadot_node_network_protocol::ValidationProtocols::V3(
				polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(
					assignments.get(index).into_iter().cloned().collect(),
				),
			)
		);
	}

	let approvals = vec![
		IndirectSignedApprovalVoteV2 {
			block_hash: H256::random(),
			candidate_indices: 1.into(),
			validator: first_validator_index,
			signature: dummy_signature(),
		},
		IndirectSignedApprovalVoteV2 {
			block_hash: H256::random(),
			candidate_indices: 2.into(),
			validator: second_validator_index,
			signature: dummy_signature(),
		},
	];
	let v3_approvals = polkadot_node_network_protocol::ValidationProtocols::V3(
		polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(
			approvals.clone(),
		),
	);
	let result = validator_index_for_msg(v3_approvals.clone());

	assert_matches!(result, (None, Some(_)));
	let messsages_split_by_validator = result.1.unwrap();
	assert_eq!(messsages_split_by_validator.len(), approvals.len());
	for (index, (validator_index, message)) in messsages_split_by_validator.into_iter().enumerate()
	{
		assert_eq!(validator_index, approvals[index].validator);
		assert_eq!(
			message,
			polkadot_node_network_protocol::ValidationProtocols::V3(
				polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(
					approvals.get(index).into_iter().cloned().collect(),
				),
			)
		);
	}
}
