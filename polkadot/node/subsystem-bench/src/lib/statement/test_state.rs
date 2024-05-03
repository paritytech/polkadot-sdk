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
use parity_scale_codec::Encode;
use polkadot_node_network_protocol::{
	request_response::{v2::AttestedCandidateResponse, Requests},
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
	Hash, Header, PersistedValidationData, SignedStatement, SigningContext, ValidatorIndex,
	ValidatorPair,
};
use polkadot_primitives_test_helpers::{
	dummy_committed_candidate_receipt, dummy_hash, dummy_head_data, dummy_pvd,
};
use sc_network::ProtocolName;
use sp_core::{Pair, H256};
use std::{
	collections::HashMap,
	sync::{
		atomic::{AtomicBool, Ordering},
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
	pub statements_tracker: HashMap<CandidateHash, HashMap<u32, Arc<AtomicBool>>>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		let mut test_state = Self {
			config: config.clone(),
			test_authorities: config.generate_authorities(),
			block_infos: Default::default(),
			candidate_receipts: Default::default(),
			commited_candidate_receipts: Default::default(),
			persisted_validation_data: dummy_pvd(dummy_head_data(), 0),
			block_headers: Default::default(),
			validator_pairs: Default::default(),
			statements_tracker: Default::default(),
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
			commited_candidate_receipt.descriptor.persisted_validation_data_hash =
				test_state.persisted_validation_data.hash();
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

			test_state.statements_tracker.insert(
				candidate_hash,
				HashMap::from_iter(
					(0..config.n_validators)
						.map(|index| (index as u32, Arc::new(AtomicBool::new(index <= 1)))),
				),
			);
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
			NetworkMessage::RequestFromNode(_authority_id, Requests::AttestedCandidateV2(req)) => {
				let payload = req.payload;
				let candidate_receipt = self
					.commited_candidate_receipts
					.values()
					.find(|v| v.hash() == payload.candidate_hash)
					.expect("Pregenerated")
					.clone();
				let persisted_validation_data = self.persisted_validation_data.clone();

				let res = AttestedCandidateResponse {
					candidate_receipt,
					persisted_validation_data,
					statements: vec![],
				};
				let _ = req.pending_response.send(Ok((res.encode(), ProtocolName::from(""))));
				None
			},
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
					.expect("Should exist");
				let candidate_hash = *statement.unchecked_payload().candidate_hash();

				let sent = self
					.statements_tracker
					.get(&candidate_hash)
					.expect("Pregenerated")
					.get(&(index as u32))
					.expect("Pregenerated")
					.as_ref();

				if sent.load(Ordering::SeqCst) {
					return None
				} else {
					sent.store(true, Ordering::SeqCst);
				}

				let statement = CompactStatement::Valid(candidate_hash);
				let context = SigningContext { parent_hash: relay_parent, session_index: 0 };
				let payload = statement.signing_payload(&context);
				let pair = self.validator_pairs.get(index).unwrap();
				let signature = pair.sign(&payload[..]);
				let statement = SignedStatement::new(
					statement,
					ValidatorIndex(index as u32),
					signature,
					&context,
					&pair.public(),
				)
				.unwrap()
				.as_unchecked()
				.to_owned();

				node_sender
					.start_send_unpin(NetworkMessage::MessageFromPeer(
						*self.test_authorities.peer_ids.get(index).expect("Must exist"),
						Versioned::V3(ValidationProtocol::StatementDistribution(
							StatementDistributionMessage::Statement(relay_parent, statement),
						)),
					))
					.unwrap();
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
				None
			},
			_ => Some(message),
		}
	}
}
