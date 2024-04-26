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
	configuration::{TestAuthorities, TestConfiguration},
	network::{HandleNetworkMessage, NetworkMessage},
};
use colored::Colorize;
use futures::SinkExt;
use itertools::Itertools;
use polkadot_node_network_protocol::{
	v3::{BackedCandidateAcknowledgement, StatementDistributionMessage, ValidationProtocol},
	Versioned,
};
use polkadot_node_primitives::{AvailableData, BlockData, PoV};
use polkadot_node_subsystem_test_helpers::{
	derive_erasure_chunks_with_proofs_and_root, mock::new_block_import_info,
};
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{
	BlockNumber, CandidateHash, CandidateReceipt, CommittedCandidateReceipt, CompactStatement,
	Hash, HeadData, Header, PersistedValidationData, SignedStatement, SigningContext,
	ValidatorIndex, ValidatorPair,
};
use polkadot_primitives_test_helpers::{dummy_committed_candidate_receipt, dummy_hash};
use sp_core::{Pair, H256};
use std::{
	collections::HashMap,
	sync::{
		atomic::{AtomicBool, AtomicU32, Ordering},
		Arc,
	},
};

const LOG_TARGET: &str = "subsystem-bench::statement::test_state";

#[derive(Clone)]
pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
	// Map from generated candidate receipts
	pub candidate_receipts: HashMap<H256, Vec<CandidateReceipt>>,
	// Map from generated commited candidate receipts
	pub commited_candidate_receipts: HashMap<H256, CommittedCandidateReceipt>,
	// TODO
	pub persisted_validation_data: PersistedValidationData,
	// Relay chain block headers
	pub block_headers: HashMap<H256, Header>,
	// TODO
	pub validator_pairs: Vec<ValidatorPair>,
	// TODO
	pub seconded_tracker: HashMap<CandidateHash, HashMap<u32, Arc<AtomicBool>>>,
	pub seconded_count: HashMap<CandidateHash, Arc<AtomicU32>>,
	pub statements_count: HashMap<CandidateHash, Arc<AtomicU32>>,
	pub known_count: HashMap<CandidateHash, Arc<AtomicU32>>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		let mut test_state = Self {
			config: config.clone(),
			test_authorities: config.generate_authorities(),
			block_infos: Default::default(),
			candidate_receipts: Default::default(),
			commited_candidate_receipts: Default::default(),
			persisted_validation_data: PersistedValidationData {
				parent_head: HeadData(vec![7, 8, 9]),
				relay_parent_number: Default::default(),
				max_pov_size: 1024,
				relay_parent_storage_root: Default::default(),
			},
			block_headers: Default::default(),
			validator_pairs: Default::default(),
			seconded_tracker: Default::default(),
			seconded_count: Default::default(),
			statements_count: Default::default(),
			known_count: Default::default(),
		};

		// For each unique pov we create a candidate receipt.
		let pov_sizes = Vec::from(config.pov_sizes());
		let mut commited_candidate_receipt_templates: Vec<CommittedCandidateReceipt> =
			Default::default();
		let mut pov_size_to_candidate: HashMap<usize, usize> = Default::default();
		for (index, pov_size) in pov_sizes.iter().cloned().unique().enumerate() {
			gum::info!(target: LOG_TARGET, index, pov_size, "{}", "Generating template candidate".bright_blue());

			let mut commited_candidate_receipt = dummy_committed_candidate_receipt(dummy_hash());
			let pov = PoV { block_data: BlockData(vec![index as u8; pov_size]) };

			let new_available_data = AvailableData {
				validation_data: test_state.persisted_validation_data.clone(),
				pov: Arc::new(pov),
			};

			let (_, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				config.n_validators,
				&new_available_data,
				|_, _| {},
			);

			commited_candidate_receipt.descriptor.erasure_root = erasure_root;
			commited_candidate_receipt_templates.push(commited_candidate_receipt);
			pov_size_to_candidate.insert(pov_size, index);
		}

		test_state.block_infos = (1..=config.num_blocks)
			.map(|block_num| {
				let relay_block_hash = Hash::repeat_byte(block_num as u8);
				new_block_import_info(relay_block_hash, block_num as BlockNumber)
			})
			.collect();

		test_state.block_headers = test_state
			.block_infos
			.iter()
			.map(|info| {
				(
					info.hash,
					Header {
						digest: Default::default(),
						number: info.number,
						parent_hash: info.parent_hash,
						extrinsics_root: Default::default(),
						state_root: Default::default(),
					},
				)
			})
			.collect::<HashMap<_, _>>();

		for (index, info) in test_state.block_infos.iter().enumerate() {
			let pov_size = pov_sizes.get(index % pov_sizes.len()).expect("This is a cycle; qed");
			let candidate_index =
				*pov_size_to_candidate.get(pov_size).expect("pov_size always exists; qed");
			let mut receipt = commited_candidate_receipt_templates[candidate_index].clone();
			receipt.descriptor.relay_parent = info.hash;
			gum::debug!(target: LOG_TARGET, candidate_hash = ?receipt.hash(), "new candidate");

			let descriptor = receipt.descriptor.clone();
			let commitments_hash = receipt.commitments.hash();
			let candidate_hash = receipt.hash();
			test_state.commited_candidate_receipts.insert(info.hash, receipt);
			test_state
				.candidate_receipts
				.insert(info.hash, vec![CandidateReceipt { descriptor, commitments_hash }]);

			test_state.seconded_tracker.insert(
				candidate_hash,
				HashMap::from_iter(
					(1..=config.max_validators_per_core)
						.map(|index| (index as u32, Arc::new(AtomicBool::new(false)))),
				),
			);
			test_state.seconded_count.insert(candidate_hash, Arc::new(AtomicU32::new(1))); // one seconded in node under test
			test_state.statements_count.insert(candidate_hash, Arc::new(AtomicU32::new(0)));
			test_state.known_count.insert(candidate_hash, Arc::new(AtomicU32::new(0)));
		}

		test_state.validator_pairs = test_state
			.test_authorities
			.key_seeds
			.iter()
			.map(|seed| ValidatorPair::from_string_with_seed(seed, None).unwrap().0)
			.collect();

		test_state
	}
}

impl HandleNetworkMessage for TestState {
	fn handle(
		&self,
		message: NetworkMessage,
		node_sender: &mut futures::channel::mpsc::UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		match message {
			NetworkMessage::MessageFromNode(
				authority_id,
				Versioned::V3(ValidationProtocol::StatementDistribution(
					StatementDistributionMessage::Statement(relay_parent, statement),
				)),
			) => {
				let index = self
					.test_authorities
					.validator_authority_id
					.iter()
					.position(|v| v == &authority_id)
					.expect("Should exist") as u32;
				let candidate_hash = *statement.unchecked_payload().candidate_hash();
				self.statements_count
					.get(&candidate_hash)
					.expect("Pregenerated")
					.as_ref()
					.fetch_add(1, Ordering::SeqCst);

				let known = self
					.seconded_tracker
					.get(&candidate_hash)
					.expect("Pregenerated")
					.get(&index)
					.expect("Pregenerated")
					.as_ref();

				if known.load(Ordering::SeqCst) {
					return None
				} else {
					known.store(true, Ordering::SeqCst);
				}
				let statement = CompactStatement::Seconded(candidate_hash);
				let context = SigningContext { parent_hash: relay_parent, session_index: 0 };
				let payload = statement.signing_payload(&context);
				let pair = self.validator_pairs.get(index as usize).expect("Must exist");
				let signature = pair.sign(&payload[..]);
				let statement = SignedStatement::new(
					statement,
					ValidatorIndex(index),
					signature,
					&context,
					&pair.public(),
				)
				.unwrap();
				let unchecked = statement.as_unchecked().clone();

				node_sender
					.start_send_unpin(NetworkMessage::MessageFromPeer(
						*self.test_authorities.peer_ids.get(index as usize).expect("Must exist"),
						Versioned::V3(ValidationProtocol::StatementDistribution(
							StatementDistributionMessage::Statement(relay_parent, unchecked),
						)),
					))
					.unwrap();
				self.seconded_count
					.get(&candidate_hash)
					.expect("Pregenerated")
					.as_ref()
					.fetch_add(1, Ordering::SeqCst);
				None
			},
			NetworkMessage::MessageFromNode(
				authority_id,
				Versioned::V3(ValidationProtocol::StatementDistribution(
					StatementDistributionMessage::BackedCandidateManifest(manifest),
				)),
			) => {
				let index = self
					.test_authorities
					.validator_authority_id
					.iter()
					.position(|v| v == &authority_id)
					.expect("Should exist");
				let ack = BackedCandidateAcknowledgement {
					candidate_hash: manifest.candidate_hash,
					statement_knowledge: manifest.statement_knowledge,
				};
				node_sender
					.start_send_unpin(NetworkMessage::MessageFromPeer(
						*self.test_authorities.peer_ids.get(index).expect("Must exist"),
						Versioned::V3(ValidationProtocol::StatementDistribution(
							StatementDistributionMessage::BackedCandidateKnown(ack),
						)),
					))
					.unwrap();
				self.known_count
					.get(&manifest.candidate_hash)
					.expect("Pregenerated")
					.as_ref()
					.fetch_add(1, Ordering::SeqCst);
				None
			},
			_ => Some(message),
		}
	}
}
