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
use polkadot_node_subsystem_test_helpers::mock::new_block_import_info;
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{
	vstaging::{CandidateEvent, CandidateReceiptV2},
	BlockNumber, CandidateCommitments, CoreIndex, GroupIndex, Hash, HeadData, Header,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt_v2_bad_sig, dummy_hash};
use std::collections::HashMap;

#[derive(Clone)]
pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
	// Map from generated candidate receipts vec![valid, invalid]
	pub candidate_receipts: HashMap<Hash, Vec<CandidateReceiptV2>>,
	// Map from generated candidate events
	pub candidate_events: HashMap<Hash, Vec<CandidateEvent>>,
	// Relay chain block headers
	pub block_headers: HashMap<Hash, Header>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		let config = config.clone();
		let test_authorities = config.generate_authorities();
		let block_infos: Vec<BlockInfo> =
			(1..=config.num_blocks).map(generate_block_info).collect();
		let candidate_receipts: HashMap<Hash, Vec<CandidateReceiptV2>> = block_infos
			.iter()
			.map(|block_info| {
				(
					block_info.hash,
					vec![
						make_valid_candidate_receipt(block_info.hash),
						make_invalid_candidate_receipt(block_info.hash),
					],
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
						.map(|receipt| make_valid_candidate_event(hash, receipt.clone()))
						.collect::<Vec<_>>(),
				)
			})
			.collect();
		let block_headers = block_infos.iter().map(generate_block_header).collect();

		Self {
			config,
			test_authorities,
			block_infos,
			candidate_receipts,
			candidate_events,
			block_headers,
		}
	}
}

fn make_valid_candidate_receipt(relay_parent: Hash) -> CandidateReceiptV2 {
	let mut candidate_receipt = dummy_candidate_receipt_v2_bad_sig(relay_parent, dummy_hash());
	candidate_receipt.commitments_hash = CandidateCommitments::default().hash();
	candidate_receipt
}

fn make_invalid_candidate_receipt(relay_parent: Hash) -> CandidateReceiptV2 {
	dummy_candidate_receipt_v2_bad_sig(relay_parent, Some(Default::default()))
}

fn make_valid_candidate_event(relay_parent: Hash, receipt: CandidateReceiptV2) -> CandidateEvent {
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

#[async_trait::async_trait]
impl HandleNetworkMessage for TestState {
	async fn handle(
		&self,
		message: NetworkMessage,
		_node_sender: &mut futures::channel::mpsc::UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		match message {
			_ => Some(message),
		}
	}
}
