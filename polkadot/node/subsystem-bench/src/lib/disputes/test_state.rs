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
	disputes::DisputesOptions,
	network::{HandleNetworkMessage, NetworkMessage},
	NODE_UNDER_TEST,
};
use codec::Encode;
use polkadot_node_network_protocol::request_response::{
	v1::{DisputeRequest, DisputeResponse},
	ProtocolName, Requests,
};
use polkadot_node_primitives::{
	InvalidDisputeVote, SignedDisputeStatement, UncheckedDisputeMessage, ValidDisputeVote,
};
use polkadot_node_subsystem_test_helpers::mock::new_block_import_info;
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{
	AuthorityDiscoveryId, BlockNumber, CandidateCommitments, CandidateEvent, CandidateHash,
	CandidateReceiptV2, CoreIndex, GroupIndex, Hash, HeadData, Header, InvalidDisputeStatementKind,
	SessionIndex, ValidDisputeStatementKind, ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt_v2_bad_sig, dummy_hash};
use rand::{seq::SliceRandom, thread_rng, Rng};
use sp_keystore::KeystorePtr;
use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Disputes Options
	pub options: DisputesOptions,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
	// Generated candidate receipts
	pub candidate_receipts: HashMap<Hash, Vec<CandidateReceiptV2>>,
	// Generated candidate events
	pub candidate_events: HashMap<Hash, Vec<CandidateEvent>>,
	// Generated dispute requests
	pub dispute_requests: HashMap<CandidateHash, Vec<(u32, DisputeRequest)>>,
	// Relay chain block headers
	pub block_headers: HashMap<Hash, Header>,
	// Map from candidate hash to authorities that have received a dispute request
	pub requests_tracker: Arc<Mutex<HashMap<CandidateHash, HashSet<AuthorityDiscoveryId>>>>,
}

impl TestState {
	pub fn new(config: &TestConfiguration, options: &DisputesOptions) -> Self {
		let config = config.clone();
		let options = options.clone();
		let test_authorities = config.generate_authorities();
		let block_infos: Vec<BlockInfo> =
			(1..=config.num_blocks).map(generate_block_info).collect();
		// Generate candidate receipts for each dispute
		let candidate_receipts: HashMap<Hash, Vec<CandidateReceiptV2>> = block_infos
			.iter()
			.map(|block_info| {
				(
					block_info.hash,
					(0..options.n_disputes)
						.map(|_| make_candidate_receipt(block_info.hash))
						.collect(),
				)
			})
			.collect();

		let candidate_events = candidate_receipts
			.iter()
			.map(|(&hash, receipts)| {
				(
					hash,
					receipts
						.iter()
						.map(|receipt| make_candidate_backed_event(receipt.clone()))
						.collect::<Vec<_>>(),
				)
			})
			.collect();

		let misbehaving: HashSet<u32> = HashSet::from_iter(options.misbehaving_validators.clone());
		assert!(!misbehaving.is_empty(), "At least one misbehaving validator must be specified");
		assert!(
			misbehaving
				.iter()
				.all(|&i| i != NODE_UNDER_TEST && i < config.n_validators as u32),
			"Misbehaving validators should be within validators range. Index {NODE_UNDER_TEST} is reserved for the node under test"
		);
		let mut rng = thread_rng();
		let misbehaving_indices = (0..options.n_disputes)
			.map(|_| {
				*misbehaving
					.iter()
					.nth(rng.gen_range(0..misbehaving.len()))
					.expect("At least one misbehaving validator")
			})
			.collect::<Vec<_>>();
		let mut available_indices: Vec<u32> =
			(1..config.n_validators as u32).filter(|i| !misbehaving.contains(i)).collect();
		let validator_indices_per_candidate: Vec<Vec<u32>> = (0..options.n_disputes)
			.map(|_| {
				available_indices.shuffle(&mut rng);
				available_indices
					.iter()
					.take(options.votes_per_candidate as usize)
					.cloned()
					.collect::<Vec<_>>()
			})
			.collect();

		let dispute_requests: HashMap<_, _> = candidate_receipts
			.iter()
			.flat_map(|(_, receipts)| {
				itertools::izip!(
					receipts.iter(),
					validator_indices_per_candidate.iter(),
					misbehaving_indices.iter()
				)
				.map(|(receipt, validator_indices, &misbehaving_index)| {
					let requests = validator_indices
						.iter()
						.map(|&validator_index| {
							let statements = vec![
								(
									issue_explicit_statement(
										test_authorities.keyring.local_keystore(),
										test_authorities.validator_public[validator_index as usize]
											.clone(),
										receipt.hash(),
										1,
										options.concluded_valid,
									),
									ValidatorIndex(validator_index),
								),
								(
									issue_explicit_statement(
										test_authorities.keyring.local_keystore(),
										test_authorities.validator_public
											[misbehaving_index as usize]
											.clone(),
										receipt.hash(),
										1,
										!options.concluded_valid, /* votes against the
										                           * supermajority */
									),
									ValidatorIndex(misbehaving_index),
								),
							];

							let valid = statements
								.iter()
								.find(|(s, _)| s.statement().indicates_validity())
								.expect("One statement generates as valid");
							let invalid = statements
								.iter()
								.find(|(s, _)| s.statement().indicates_invalidity())
								.expect("One statement generates as invalid");

							let request = DisputeRequest(UncheckedDisputeMessage {
								candidate_receipt: receipt.clone(),
								session_index: 1,
								valid_vote: ValidDisputeVote {
									validator_index: valid.1,
									signature: valid.0.validator_signature().clone(),
									kind: ValidDisputeStatementKind::Explicit,
								},
								invalid_vote: InvalidDisputeVote {
									validator_index: invalid.1,
									signature: invalid.0.validator_signature().clone(),
									kind: InvalidDisputeStatementKind::Explicit,
								},
							});

							(validator_index, request)
						})
						.collect::<Vec<_>>();

					(receipt.hash(), requests)
				})
			})
			.collect();
		let block_headers = block_infos.iter().map(generate_block_header).collect();
		let requests_tracker = Arc::new(Mutex::new(HashMap::new()));

		Self {
			config,
			options,
			test_authorities,
			block_infos,
			candidate_receipts,
			candidate_events,
			dispute_requests,
			block_headers,
			requests_tracker,
		}
	}

	pub fn invalid_candidates(&self) -> Vec<Hash> {
		if self.options.concluded_valid {
			vec![]
		} else {
			// every disputed candidate should be invalid
			self.candidate_receipts
				.values()
				.flat_map(|receipts| receipts.iter().map(|r| r.hash().0))
				.collect::<Vec<_>>()
		}
	}
}

fn make_candidate_receipt(relay_parent: Hash) -> CandidateReceiptV2 {
	let mut candidate_receipt = dummy_candidate_receipt_v2_bad_sig(relay_parent, dummy_hash());
	candidate_receipt.commitments_hash = CandidateCommitments::default().hash();
	candidate_receipt
}

fn make_candidate_backed_event(receipt: CandidateReceiptV2) -> CandidateEvent {
	CandidateEvent::CandidateBacked(
		receipt,
		HeadData::default(),
		CoreIndex::default(),
		GroupIndex::default(),
	)
}

fn generate_block_info(block_num: usize) -> BlockInfo {
	new_block_import_info(Hash::repeat_byte(block_num as u8), block_num as BlockNumber)
}

fn generate_block_header(info: &BlockInfo) -> (Hash, Header) {
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

fn issue_explicit_statement(
	keystore: KeystorePtr,
	public: ValidatorId,
	candidate_hash: CandidateHash,
	session: SessionIndex,
	valid: bool,
) -> SignedDisputeStatement {
	SignedDisputeStatement::sign_explicit(&keystore, valid, candidate_hash, session, public)
		.unwrap()
		.unwrap()
}

#[async_trait::async_trait]
impl HandleNetworkMessage for TestState {
	async fn handle(
		&self,
		message: NetworkMessage,
		_node_sender: &mut futures::channel::mpsc::UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		match message {
			NetworkMessage::RequestFromNode(authority_id, requests) => {
				let Requests::DisputeSendingV1(req) = *requests else {
					todo!("Wrong requests type in message: {:?}", requests);
				};
				let mut tracker = self.requests_tracker.lock().unwrap();
				tracker
					.entry(req.payload.0.candidate_receipt.hash())
					.or_default()
					.insert(authority_id);
				drop(tracker);

				let _ = req
					.pending_response
					.send(Ok(((DisputeResponse::Confirmed).encode(), ProtocolName::from(""))));
				None
			},
			_ => {
				todo!("Wrong message type: {:?}", message);
			},
		}
	}
}
