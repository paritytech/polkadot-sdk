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

//! A generic statement distribution subsystem mockup suitable to be used in benchmarks.

use crate::{statement::TestState, NODE_UNDER_TEST, SESSION_INDEX};
use bitvec::vec::BitVec;
use futures::FutureExt;
use polkadot_node_network_protocol::{
	request_response::{
		v2::{AttestedCandidateRequest, AttestedCandidateResponse},
		IncomingRequestReceiver, OutgoingRequest, Recipient, Requests,
	},
	v3::{self, BackedCandidateAcknowledgement, StatementFilter},
	UnifiedReputationChange, Versioned,
};
use polkadot_node_subsystem::{
	messages::{NetworkBridgeEvent, NetworkBridgeTxMessage, StatementDistributionMessage},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_primitives::{CompactStatement, SignedStatement, SigningContext, ValidatorIndex};
use sc_network::IfDisconnected;
use sp_application_crypto::Pair;
use std::sync::{atomic::Ordering, Arc};

const COST_INVALID_REQUEST: UnifiedReputationChange =
	UnifiedReputationChange::CostMajor("Peer sent unparsable request");
const LOG_TARGET: &str = "subsystem-bench::statement-distribution-mock";

pub struct MockStatementDistribution {
	/// Receiver for attested candidate requests.
	req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
	test_state: Arc<TestState>,
	index: usize,
}

impl MockStatementDistribution {
	pub fn new(
		req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
		test_state: Arc<TestState>,
		index: usize,
	) -> Self {
		Self { req_receiver, test_state, index }
	}
}

#[overseer::subsystem(StatementDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockStatementDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();
		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(StatementDistribution, prefix = self::overseer)]
impl MockStatementDistribution {
	async fn run<Context>(mut self, mut ctx: Context) {
		loop {
			tokio::select! {
				msg = ctx.recv() => match msg {
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) => return,
					Ok(FromOrchestra::Communication { msg }) =>
						match msg {
							StatementDistributionMessage::NetworkBridgeUpdate(
								NetworkBridgeEvent::PeerMessage(
									peer_id,
									Versioned::V3(v3::StatementDistributionMessage::Statement(
										relay_parent,
										statement,
									)),
								),
							) => {
								let candidate_hash = *statement.unchecked_payload().candidate_hash();
								let statements_sent_count = self
									.test_state
									.statements_tracker
									.get(&candidate_hash)
									.unwrap()
									.get(self.index)
									.unwrap()
									.as_ref();
								if statements_sent_count.load(Ordering::SeqCst) {
									continue
								} else {
									statements_sent_count.store(true, Ordering::SeqCst);
								}

								let group_statements = self.test_state.statements.get(&candidate_hash).unwrap();
								if !group_statements.iter().any(|s| s.unchecked_validator_index().0 == self.index as u32)
								{
									continue
								}

								let statement = CompactStatement::Valid(candidate_hash);
								let context =
									SigningContext { parent_hash: relay_parent, session_index: SESSION_INDEX };
								let payload = statement.signing_payload(&context);
								let pair = self.test_state.test_authorities.validator_pairs.get(self.index).unwrap();
								let signature = pair.sign(&payload[..]);
								let statement = SignedStatement::new(
									statement,
									ValidatorIndex(self.index as u32),
									signature,
									&context,
									&pair.public(),
								)
								.unwrap()
								.as_unchecked()
								.to_owned();

								ctx.send_message(NetworkBridgeTxMessage::SendValidationMessage(
									vec![peer_id],
									Versioned::V3(v3::StatementDistributionMessage::Statement(
										relay_parent,
										statement,
									))
									.into(),
								))
								.await;
							},
							StatementDistributionMessage::NetworkBridgeUpdate(
								NetworkBridgeEvent::PeerMessage(
									peer_id,
									Versioned::V3(v3::StatementDistributionMessage::BackedCandidateManifest(
										manifest,
									)),
								),
							) => {
								let backing_group =
									self.test_state.session_info.validator_groups.get(manifest.group_index).unwrap();
								let group_size = backing_group.len();
								let is_own_backing_group = backing_group.contains(&ValidatorIndex(NODE_UNDER_TEST));
								let mut seconded_in_group =
									BitVec::from_iter((0..group_size).map(|_| !is_own_backing_group));
								let mut validated_in_group = BitVec::from_iter((0..group_size).map(|_| false));

								if is_own_backing_group {
									let (req, pending_response) = OutgoingRequest::new(
										Recipient::Peer(peer_id),
										AttestedCandidateRequest {
											candidate_hash: manifest.candidate_hash,
											mask: StatementFilter::blank(group_size),
										},
									);
									let reqs = vec![Requests::AttestedCandidateV2(req)];
									ctx.send_message(NetworkBridgeTxMessage::SendRequests(
										reqs,
										IfDisconnected::TryConnect,
									))
									.await;

									let response = pending_response.await.unwrap();
									for statement in response.statements {
										let validator_index = statement.unchecked_validator_index();
										let position_in_group =
											backing_group.iter().position(|v| *v == validator_index).unwrap();
										match statement.unchecked_payload() {
											CompactStatement::Seconded(_) =>
												seconded_in_group.set(position_in_group, true),
											CompactStatement::Valid(_) =>
												validated_in_group.set(position_in_group, true),
										}
									}
								}

								let ack = BackedCandidateAcknowledgement {
									candidate_hash: manifest.candidate_hash,
									statement_knowledge: StatementFilter { seconded_in_group, validated_in_group },
								};

								ctx.send_message(NetworkBridgeTxMessage::SendValidationMessage(
									vec![peer_id],
									Versioned::V3(v3::StatementDistributionMessage::BackedCandidateKnown(ack)).into(),
								))
								.await;

								self.test_state
									.manifests_tracker
									.get(&manifest.candidate_hash)
									.unwrap()
									.as_ref()
									.store(true, Ordering::SeqCst);
							},
							StatementDistributionMessage::NetworkBridgeUpdate(
								NetworkBridgeEvent::PeerMessage(
									_peer_id,
									Versioned::V3(v3::StatementDistributionMessage::BackedCandidateKnown(ack)),
								),
							) => {
								self.test_state
									.manifests_tracker
									.get(&ack.candidate_hash)
									.unwrap()
									.as_ref()
									.store(true, Ordering::SeqCst);
							},
							msg => gum::debug!(target: LOG_TARGET, ?msg, "Unhandled message"),
						},
					err => gum::error!(target: LOG_TARGET, ?err, "recv error"),
				},
				req = self.req_receiver.recv(|| vec![COST_INVALID_REQUEST]) => {
					let req = req.expect("Receiver never fails");
					let candidate_receipt = self
						.test_state
						.commited_candidate_receipts
						.values()
						.flatten()
						.find(|v| v.hash() == req.payload.candidate_hash)
						.unwrap()
						.clone();
					let persisted_validation_data = self.test_state.pvd.clone();
					let statements = self.test_state.statements.get(&req.payload.candidate_hash).unwrap().clone();
					let res = AttestedCandidateResponse {
						candidate_receipt,
						persisted_validation_data,
						statements,
					};
					let _ = req.send_response(res);
				}
			}
		}
	}
}
