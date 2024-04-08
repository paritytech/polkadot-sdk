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

use crate::{
	approval::{ApprovalsOptions, BlockTestData, CandidateTestData},
	configuration::TestAuthorities,
};
use itertools::Itertools;
use parity_scale_codec::{Decode, Encode};
use polkadot_node_network_protocol::v3 as protocol_v3;
use polkadot_primitives::{CandidateIndex, Hash, ValidatorIndex};
use sc_network::PeerId;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct TestMessageInfo {
	/// The actual message
	pub msg: protocol_v3::ApprovalDistributionMessage,
	/// The list of peers that would sends this message in a real topology.
	/// It includes both the peers that would send the message because of the topology
	/// or because of randomly choosing so.
	pub sent_by: Vec<ValidatorIndex>,
	/// The tranche at which this message should be sent.
	pub tranche: u32,
	/// The block hash this message refers to.
	pub block_hash: Hash,
}

impl std::hash::Hash for TestMessageInfo {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		match &self.msg {
			protocol_v3::ApprovalDistributionMessage::Assignments(assignments) => {
				for (assignment, candidates) in assignments {
					(assignment.block_hash, assignment.validator).hash(state);
					candidates.hash(state);
				}
			},
			protocol_v3::ApprovalDistributionMessage::Approvals(approvals) => {
				for approval in approvals {
					(approval.block_hash, approval.validator).hash(state);
					approval.candidate_indices.hash(state);
				}
			},
		};
	}
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
/// A list of messages that depend of each-other, approvals cover one of the assignments and
/// vice-versa.
pub struct MessagesBundle {
	pub assignments: Vec<TestMessageInfo>,
	pub approvals: Vec<TestMessageInfo>,
}

impl MessagesBundle {
	/// The tranche when this bundle can be sent correctly, so no assignments or approvals will be
	/// from the future.
	pub fn tranche_to_send(&self) -> u32 {
		self.assignments
			.iter()
			.chain(self.approvals.iter())
			.max_by(|a, b| a.tranche.cmp(&b.tranche))
			.unwrap()
			.tranche
	}

	/// The min tranche in the bundle.
	pub fn min_tranche(&self) -> u32 {
		self.assignments
			.iter()
			.chain(self.approvals.iter())
			.min_by(|a, b| a.tranche.cmp(&b.tranche))
			.unwrap()
			.tranche
	}

	/// Tells if the bundle is needed for sending.
	/// We either send it because we need more assignments and approvals to approve the candidates
	/// or because we configured the test to send messages until a given tranche.
	pub fn should_send(
		&self,
		candidates_test_data: &HashMap<(Hash, CandidateIndex), CandidateTestData>,
		options: &ApprovalsOptions,
	) -> bool {
		self.needed_for_approval(candidates_test_data) ||
			(!options.stop_when_approved &&
				self.min_tranche() <= options.last_considered_tranche)
	}

	/// Tells if the bundle is needed because we need more messages to approve the candidates.
	pub fn needed_for_approval(
		&self,
		candidates_test_data: &HashMap<(Hash, CandidateIndex), CandidateTestData>,
	) -> bool {
		self.assignments
			.iter()
			.any(|message| message.needed_for_approval(candidates_test_data))
	}

	/// Mark the assignments in the bundle as sent.
	pub fn record_sent_assignment(
		&self,
		candidates_test_data: &mut HashMap<(Hash, CandidateIndex), CandidateTestData>,
	) {
		self.assignments
			.iter()
			.for_each(|assignment| assignment.record_sent_assignment(candidates_test_data));
	}
}

impl TestMessageInfo {
	/// Tells if the message is an approval.
	fn is_approval(&self) -> bool {
		match self.msg {
			protocol_v3::ApprovalDistributionMessage::Assignments(_) => false,
			protocol_v3::ApprovalDistributionMessage::Approvals(_) => true,
		}
	}

	/// Records an approval.
	/// We use this to check after all messages have been processed that we didn't loose any
	/// message.
	pub fn record_vote(&self, state: &BlockTestData) {
		if self.is_approval() {
			match &self.msg {
				protocol_v3::ApprovalDistributionMessage::Assignments(_) => todo!(),
				protocol_v3::ApprovalDistributionMessage::Approvals(approvals) =>
					for approval in approvals {
						for candidate_index in approval.candidate_indices.iter_ones() {
							state
								.votes
								.get(approval.validator.0 as usize)
								.unwrap()
								.get(candidate_index)
								.unwrap()
								.store(true, std::sync::atomic::Ordering::SeqCst);
						}
					},
			}
		}
	}

	/// Mark the assignments in the message as sent.
	pub fn record_sent_assignment(
		&self,
		candidates_test_data: &mut HashMap<(Hash, CandidateIndex), CandidateTestData>,
	) {
		match &self.msg {
			protocol_v3::ApprovalDistributionMessage::Assignments(assignments) => {
				for (assignment, candidate_indices) in assignments {
					for candidate_index in candidate_indices.iter_ones() {
						let candidate_test_data = candidates_test_data
							.get_mut(&(assignment.block_hash, candidate_index as CandidateIndex))
							.unwrap();
						candidate_test_data.mark_sent_assignment(self.tranche)
					}
				}
			},
			protocol_v3::ApprovalDistributionMessage::Approvals(_approvals) => todo!(),
		}
	}

	/// Returns a list of candidates indices in this message
	pub fn candidate_indices(&self) -> HashSet<usize> {
		let mut unique_candidate_indices = HashSet::new();
		match &self.msg {
			protocol_v3::ApprovalDistributionMessage::Assignments(assignments) =>
				for (_assignment, candidate_indices) in assignments {
					for candidate_index in candidate_indices.iter_ones() {
						unique_candidate_indices.insert(candidate_index);
					}
				},
			protocol_v3::ApprovalDistributionMessage::Approvals(approvals) =>
				for approval in approvals {
					for candidate_index in approval.candidate_indices.iter_ones() {
						unique_candidate_indices.insert(candidate_index);
					}
				},
		}
		unique_candidate_indices
	}

	/// Marks this message as no-shows if the number of configured no-shows is above the registered
	/// no-shows.
	/// Returns true if the message is a no-show.
	pub fn no_show_if_required(
		&self,
		assignments: &[TestMessageInfo],
		candidates_test_data: &mut HashMap<(Hash, CandidateIndex), CandidateTestData>,
	) -> bool {
		let mut should_no_show = false;
		if self.is_approval() {
			let covered_candidates = assignments
				.iter()
				.map(|assignment| (assignment, assignment.candidate_indices()))
				.collect_vec();

			match &self.msg {
				protocol_v3::ApprovalDistributionMessage::Assignments(_) => todo!(),
				protocol_v3::ApprovalDistributionMessage::Approvals(approvals) => {
					assert_eq!(approvals.len(), 1);

					for approval in approvals {
						should_no_show = should_no_show ||
							approval.candidate_indices.iter_ones().all(|candidate_index| {
								let candidate_test_data = candidates_test_data
									.get_mut(&(
										approval.block_hash,
										candidate_index as CandidateIndex,
									))
									.unwrap();
								let assignment = covered_candidates
									.iter()
									.find(|(_assignment, candidates)| {
										candidates.contains(&candidate_index)
									})
									.unwrap();
								candidate_test_data.should_no_show(assignment.0.tranche)
							});

						if should_no_show {
							for candidate_index in approval.candidate_indices.iter_ones() {
								let candidate_test_data = candidates_test_data
									.get_mut(&(
										approval.block_hash,
										candidate_index as CandidateIndex,
									))
									.unwrap();
								let assignment = covered_candidates
									.iter()
									.find(|(_assignment, candidates)| {
										candidates.contains(&candidate_index)
									})
									.unwrap();
								candidate_test_data.record_no_show(assignment.0.tranche)
							}
						}
					}
				},
			}
		}
		should_no_show
	}

	/// Tells if a message is needed for approval
	pub fn needed_for_approval(
		&self,
		candidates_test_data: &HashMap<(Hash, CandidateIndex), CandidateTestData>,
	) -> bool {
		match &self.msg {
			protocol_v3::ApprovalDistributionMessage::Assignments(assignments) =>
				assignments.iter().any(|(assignment, candidate_indices)| {
					candidate_indices.iter_ones().any(|candidate_index| {
						candidates_test_data
							.get(&(assignment.block_hash, candidate_index as CandidateIndex))
							.map(|data| data.should_send_tranche(self.tranche))
							.unwrap_or_default()
					})
				}),
			protocol_v3::ApprovalDistributionMessage::Approvals(approvals) =>
				approvals.iter().any(|approval| {
					approval.candidate_indices.iter_ones().any(|candidate_index| {
						candidates_test_data
							.get(&(approval.block_hash, candidate_index as CandidateIndex))
							.map(|data| data.should_send_tranche(self.tranche))
							.unwrap_or_default()
					})
				}),
		}
	}

	/// Splits a message into multiple messages based on what peers should send this message.
	/// It build a HashMap of messages that should be sent by each peer.
	pub fn split_by_peer_id(
		self,
		authorities: &TestAuthorities,
	) -> HashMap<(ValidatorIndex, PeerId), Vec<TestMessageInfo>> {
		let mut result: HashMap<(ValidatorIndex, PeerId), Vec<TestMessageInfo>> = HashMap::new();

		for validator_index in &self.sent_by {
			let peer = authorities.peer_ids.get(validator_index.0 as usize).unwrap();
			result.entry((*validator_index, *peer)).or_default().push(TestMessageInfo {
				msg: self.msg.clone(),
				sent_by: Default::default(),
				tranche: self.tranche,
				block_hash: self.block_hash,
			});
		}
		result
	}
}
