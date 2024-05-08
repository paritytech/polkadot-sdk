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
use bitvec::order::Lsb0;
use itertools::Itertools;
use parity_scale_codec::Encode;
use polkadot_node_network_protocol::{
	request_response::{v2::AttestedCandidateResponse, Requests},
	v3::{
		BackedCandidateAcknowledgement, StatementDistributionMessage, StatementFilter,
		ValidationProtocol,
	},
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
	pub commited_candidate_receipts: HashMap<H256, Vec<CommittedCandidateReceipt>>,
	// TODO
	pub pvd: PersistedValidationData,
	// Relay chain block headers
	pub block_headers: HashMap<H256, Header>,
	// TODO
	pub validator_pairs: Vec<ValidatorPair>,
	// TODO
	pub statements_tracker: HashMap<CandidateHash, HashMap<u32, Arc<AtomicBool>>>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		let mut state = Self {
			config: config.clone(),
			test_authorities: config.generate_authorities(),
			block_infos: (1..=config.num_blocks).map(generate_block_info).collect(),
			candidate_receipts: Default::default(),
			commited_candidate_receipts: Default::default(),
			pvd: dummy_pvd(dummy_head_data(), 0),
			block_headers: Default::default(),
			validator_pairs: Default::default(),
			statements_tracker: Default::default(),
		};

		state.block_headers = state.block_infos.iter().map(generate_block_header).collect();
		state.validator_pairs =
			state.test_authorities.key_seeds.iter().map(generate_pair).collect();

		// For each unique pov we create a candidate receipt.
		let pov_sizes = Vec::from(config.pov_sizes()); // For n_cores
		let pov_size_to_candidate = generate_pov_size_to_candidate(&pov_sizes);
		let receipt_templates =
			generate_receipt_templates(&pov_size_to_candidate, config.n_validators, &state.pvd);

		for block_info in state.block_infos.iter() {
			for core_idx in 0..config.n_cores {
				let pov_size = pov_sizes.get(core_idx).expect("This is a cycle; qed");
				let candidate_index =
					*pov_size_to_candidate.get(pov_size).expect("pov_size always exists; qed");
				let mut receipt = receipt_templates[candidate_index].clone();
				receipt.descriptor.relay_parent = block_info.hash;

				state.candidate_receipts.entry(block_info.hash).or_default().push(
					CandidateReceipt {
						descriptor: receipt.descriptor.clone(),
						commitments_hash: receipt.commitments.hash(),
					},
				);
				state.statements_tracker.entry(receipt.hash()).or_default().extend(
					(0..config.n_validators)
						.map(|index| (index as u32, Arc::new(AtomicBool::new(index <= 1))))
						.collect::<HashMap<_, _>>(),
				);
				state
					.commited_candidate_receipts
					.entry(block_info.hash)
					.or_default()
					.push(receipt);
			}
		}

		state
	}
}

fn generate_block_info(block_num: usize) -> BlockInfo {
	new_block_import_info(Hash::repeat_byte(block_num as u8), block_num as BlockNumber)
}

fn generate_block_header(info: &BlockInfo) -> (H256, Header) {
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
}

fn generate_pov_size_to_candidate(pov_sizes: &[usize]) -> HashMap<usize, usize> {
	pov_sizes
		.iter()
		.cloned()
		.unique()
		.enumerate()
		.map(|(index, pov_size)| (pov_size, index))
		.collect()
}

fn generate_receipt_templates(
	pov_size_to_candidate: &HashMap<usize, usize>,
	n_validators: usize,
	pvd: &PersistedValidationData,
) -> Vec<CommittedCandidateReceipt> {
	pov_size_to_candidate
		.iter()
		.map(|(&pov_size, &index)| {
			let mut receipt = dummy_committed_candidate_receipt(dummy_hash());
			let (_, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				n_validators,
				&AvailableData {
					validation_data: pvd.clone(),
					pov: Arc::new(PoV { block_data: BlockData(vec![index as u8; pov_size]) }),
				},
				|_, _| {},
			);
			receipt.descriptor.persisted_validation_data_hash = pvd.hash();
			receipt.descriptor.erasure_root = erasure_root;
			receipt
		})
		.collect()
}

#[allow(clippy::ptr_arg)]
fn generate_pair(seed: &String) -> ValidatorPair {
	ValidatorPair::from_string_with_seed(seed, None).unwrap().0
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
					.flatten()
					.find(|v| v.hash() == payload.candidate_hash)
					.expect("Pregenerated")
					.clone();
				let persisted_validation_data = self.pvd.clone();
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
				gum::debug!("ValidatorIndex({}) received {:?}", index, statement);

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

				if index > self.config.max_validators_per_core - 1 {
					return None
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
					.start_send(NetworkMessage::MessageFromPeer(
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
					statement_knowledge: StatementFilter {
						seconded_in_group: bitvec::bitvec![u8, Lsb0; 0,1,0,0,0],
						validated_in_group: bitvec::bitvec![u8, Lsb0; 0,0,1,0,0],
					},
				};
				gum::debug!("ValidatorIndex({}) sends {:?}", index, ack);
				node_sender
					.start_send(NetworkMessage::MessageFromPeer(
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
