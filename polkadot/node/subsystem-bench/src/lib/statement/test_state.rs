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

use crate::configuration::{TestAuthorities, TestConfiguration};
use colored::Colorize;
use itertools::Itertools;
use polkadot_node_primitives::{AvailableData, BlockData, PoV};
use polkadot_node_subsystem_test_helpers::{
	derive_erasure_chunks_with_proofs_and_root, mock::new_block_import_info,
};
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{
	BlockNumber, CandidateReceipt, CommittedCandidateReceipt, Hash, HeadData, Header,
	PersistedValidationData,
};
use polkadot_primitives_test_helpers::{dummy_committed_candidate_receipt, dummy_hash};
use sp_core::H256;
use std::{collections::HashMap, sync::Arc};

const LOG_TARGET: &str = "subsystem-bench::statement::test_state";

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
	pub persisted_validation_data: PersistedValidationData,
	// Relay chain block headers
	pub block_headers: HashMap<H256, Header>,
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

		// Generate all candidates
		let candidates_count = config.n_cores * config.num_blocks;
		gum::info!(target: LOG_TARGET,"{}", format!("Pre-generating {} candidates.", candidates_count).bright_blue());
		let candidates = (0..candidates_count)
			.map(|index| {
				let pov_size =
					pov_sizes.get(index % pov_sizes.len()).expect("This is a cycle; qed");
				let candidate_index =
					*pov_size_to_candidate.get(&pov_size).expect("pov_size always exists; qed");
				let mut candidate_receipt =
					commited_candidate_receipt_templates[candidate_index].clone();

				// Make it unique.
				candidate_receipt.descriptor.relay_parent = Hash::from_low_u64_be(index as u64);

				gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_receipt.hash(), "new candidate");

				candidate_receipt
			})
			.collect::<Vec<_>>();

		for info in test_state.block_infos.iter() {
			for _ in 0..config.n_cores {
				let receipt = candidates
					.get(config.num_blocks * config.n_cores % candidates.len())
					.expect("Cycle");
				test_state
					.commited_candidate_receipts
					.entry(info.hash)
					.or_default()
					.push(receipt.clone());
				test_state.candidate_receipts.entry(info.hash).or_default().push(
					CandidateReceipt {
						descriptor: receipt.descriptor.clone(),
						commitments_hash: receipt.commitments.hash(),
					},
				);
			}
		}

		test_state
	}
}
