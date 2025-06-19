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
use polkadot_node_subsystem_test_helpers::mock::generate_block_info;
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{vstaging::CandidateReceiptV2, CandidateCommitments};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt_v2_bad_sig, dummy_hash};
use sp_core::H256;
use std::collections::HashMap;

#[derive(Clone)]
pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
	// Map from generated candidate receipts (valid, invalid)
	pub candidate_receipts: HashMap<H256, (CandidateReceiptV2, CandidateReceiptV2)>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		let config = config.clone();
		let test_authorities = config.generate_authorities();
		let block_infos: Vec<BlockInfo> =
			(1..=config.num_blocks).map(generate_block_info).collect();
		let candidate_receipts = block_infos
			.iter()
			.map(|block_info| {
				(
					block_info.hash,
					(
						make_valid_candidate_receipt(block_info.hash),
						make_invalid_candidate_receipt(block_info.hash),
					),
				)
			})
			.collect();

		Self { config, test_authorities, block_infos, candidate_receipts }
	}
}

fn make_valid_candidate_receipt(relay_parent: H256) -> CandidateReceiptV2 {
	let mut candidate_receipt = dummy_candidate_receipt_v2_bad_sig(relay_parent, dummy_hash());
	candidate_receipt.commitments_hash = CandidateCommitments::default().hash();
	candidate_receipt
}

fn make_invalid_candidate_receipt(relay_parent: H256) -> CandidateReceiptV2 {
	dummy_candidate_receipt_v2_bad_sig(Default::default(), Some(Default::default()))
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
