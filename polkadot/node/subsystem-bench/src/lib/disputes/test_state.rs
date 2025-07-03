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
	vstaging::{CandidateEvent, CandidateReceiptV2},
	AuthorityDiscoveryId, BlockNumber, CandidateCommitments, CandidateHash, CoreIndex, GroupIndex,
	Hash, HeadData, Header, InvalidDisputeStatementKind, SessionIndex, ValidDisputeStatementKind,
	ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt_v2_bad_sig, dummy_hash};
use sp_keystore::KeystorePtr;
use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
	// Generated candidate receipts
	pub candidate_receipts: HashMap<Hash, Vec<CandidateReceiptV2>>,
	// Generated candidate events
	pub candidate_events: HashMap<Hash, Vec<CandidateEvent>>,
	// Generated dispute requests
	pub dispute_requests: HashMap<CandidateHash, DisputeRequest>,
	// Relay chain block headers
	pub block_headers: HashMap<Hash, Header>,
	// Map from candidate hash to authorities that have received a dispute request
	pub requests_tracker: Arc<Mutex<HashMap<CandidateHash, HashSet<AuthorityDiscoveryId>>>>,
}

impl TestState {
	pub fn new(config: &TestConfiguration, options: &DisputesOptions) -> Self {
		let config = config.clone();
		let test_authorities = config.generate_authorities();
		let block_infos: Vec<BlockInfo> =
			(1..=config.num_blocks).map(generate_block_info).collect();
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
		let dispute_requests = candidate_receipts
			.iter()
			.flat_map(|(_, receipts)| {
				receipts.iter().map(|receipt| {
					let valid = issue_explicit_statement(
						test_authorities.keyring.local_keystore(),
						test_authorities.validator_public[1].clone(),
						receipt.hash(),
						1,
						true,
					);
					let invalid = issue_explicit_statement(
						test_authorities.keyring.local_keystore(),
						test_authorities.validator_public[3].clone(),
						receipt.hash(),
						1,
						false,
					);

					(
						receipt.hash(),
						DisputeRequest(UncheckedDisputeMessage {
							candidate_receipt: receipt.clone(),
							session_index: 1,
							valid_vote: ValidDisputeVote {
								validator_index: ValidatorIndex(1),
								signature: valid.validator_signature().clone(),
								kind: ValidDisputeStatementKind::Explicit,
							},
							invalid_vote: InvalidDisputeVote {
								validator_index: ValidatorIndex(3),
								signature: invalid.validator_signature().clone(),
								kind: InvalidDisputeStatementKind::Explicit,
							},
						}),
					)
				})
			})
			.collect();
		let block_headers = block_infos.iter().map(generate_block_header).collect();
		let requests_tracker = Arc::new(Mutex::new(HashMap::new()));

		Self {
			config,
			test_authorities,
			block_infos,
			candidate_receipts,
			candidate_events,
			dispute_requests,
			block_headers,
			requests_tracker,
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
			NetworkMessage::RequestFromNode(authority_id, Requests::DisputeSendingV1(req)) => {
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
				todo!("{:?}", message);
			},
		}
	}
}
