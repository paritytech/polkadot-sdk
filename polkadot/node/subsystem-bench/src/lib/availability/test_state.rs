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
	environment::GENESIS_HASH,
	mock::runtime_api::node_features_with_chunk_mapping_enabled,
};
use bitvec::bitvec;
use codec::Encode;
use colored::Colorize;
use itertools::Itertools;
use polkadot_node_network_protocol::{
	request_response::{v2::ChunkFetchingRequest, ReqProtocolNames},
	Versioned, VersionedValidationProtocol,
};
use polkadot_node_primitives::{AvailableData, BlockData, ErasureChunk, PoV};
use polkadot_node_subsystem_test_helpers::{
	derive_erasure_chunks_with_proofs_and_root, mock::new_block_import_info,
};
use polkadot_node_subsystem_util::availability_chunks::availability_chunk_indices;
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{
	vstaging::{CandidateReceiptV2 as CandidateReceipt, MutateDescriptorV2},
	AvailabilityBitfield, BlockNumber, CandidateHash, ChunkIndex, CoreIndex, Hash, HeadData,
	Header, PersistedValidationData, Signed, SigningContext, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sp_core::H256;
use std::{collections::HashMap, iter::Cycle, sync::Arc};

const LOG_TARGET: &str = "subsystem-bench::availability::test_state";

#[derive(Clone)]
pub struct TestState {
	// Full test configuration
	pub config: TestConfiguration,
	// A cycle iterator on all PoV sizes used in the test.
	pub pov_sizes: Cycle<std::vec::IntoIter<usize>>,
	// Generated candidate receipts to be used in the test
	pub candidates: Cycle<std::vec::IntoIter<CandidateReceipt>>,
	// Map from pov size to candidate index
	pub pov_size_to_candidate: HashMap<usize, usize>,
	// Map from generated candidate hashes to candidate index in `available_data` and `chunks`.
	pub candidate_hashes: HashMap<CandidateHash, usize>,
	// Map from candidate hash to occupied core index.
	pub candidate_hash_to_core_index: HashMap<CandidateHash, CoreIndex>,
	// Per candidate index receipts.
	pub candidate_receipt_templates: Vec<CandidateReceipt>,
	// Per candidate index `AvailableData`
	pub available_data: Vec<AvailableData>,
	// Per candidate index chunks
	pub chunks: Vec<Vec<ErasureChunk>>,
	// Per-core ValidatorIndex -> ChunkIndex mapping
	pub chunk_indices: Vec<Vec<ChunkIndex>>,
	// Per relay chain block - candidate backed by our backing group
	pub backed_candidates: Vec<CandidateReceipt>,
	// Request protcol names
	pub req_protocol_names: ReqProtocolNames,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
	// Chung fetching requests for backed candidates
	pub chunk_fetching_requests: Vec<Vec<Vec<u8>>>,
	// Pregenerated signed availability bitfields
	pub signed_bitfields: HashMap<H256, Vec<VersionedValidationProtocol>>,
	// Relay chain block headers
	pub block_headers: HashMap<H256, Header>,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Map from generated candidate receipts
	pub candidate_receipts: HashMap<H256, Vec<CandidateReceipt>>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		let mut test_state = Self {
			available_data: Default::default(),
			candidate_receipt_templates: Default::default(),
			chunks: Default::default(),
			pov_size_to_candidate: Default::default(),
			pov_sizes: Vec::from(config.pov_sizes()).into_iter().cycle(),
			candidate_hashes: HashMap::new(),
			candidates: Vec::new().into_iter().cycle(),
			backed_candidates: Vec::new(),
			config: config.clone(),
			block_infos: Default::default(),
			chunk_fetching_requests: Default::default(),
			signed_bitfields: Default::default(),
			candidate_receipts: Default::default(),
			block_headers: Default::default(),
			test_authorities: config.generate_authorities(),
			req_protocol_names: ReqProtocolNames::new(GENESIS_HASH, None),
			chunk_indices: Default::default(),
			candidate_hash_to_core_index: Default::default(),
		};

		// we use it for all candidates.
		let persisted_validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		test_state.chunk_indices = (0..config.n_cores)
			.map(|core_index| {
				availability_chunk_indices(
					Some(&node_features_with_chunk_mapping_enabled()),
					config.n_validators,
					CoreIndex(core_index as u32),
				)
				.unwrap()
			})
			.collect();

		// For each unique pov we create a candidate receipt.
		for (index, pov_size) in config.pov_sizes().iter().cloned().unique().enumerate() {
			gum::info!(target: LOG_TARGET, index, pov_size, "{}", "Generating template candidate".bright_blue());

			let mut candidate_receipt = dummy_candidate_receipt(dummy_hash());
			let pov = PoV { block_data: BlockData(vec![index as u8; pov_size]) };

			let new_available_data = AvailableData {
				validation_data: persisted_validation_data.clone(),
				pov: Arc::new(pov),
			};

			let (new_chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				config.n_validators,
				&new_available_data,
				|_, _| {},
			);

			candidate_receipt.descriptor.erasure_root = erasure_root;

			test_state.chunks.push(new_chunks);
			test_state.available_data.push(new_available_data);
			test_state.pov_size_to_candidate.insert(pov_size, index);
			test_state.candidate_receipt_templates.push(CandidateReceipt {
				descriptor: candidate_receipt.descriptor.into(),
				commitments_hash: candidate_receipt.commitments_hash,
			});
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
		test_state.candidates = (0..candidates_count)
			.map(|index| {
				let pov_size = test_state.pov_sizes.next().expect("This is a cycle; qed");
				let candidate_index = *test_state
					.pov_size_to_candidate
					.get(&pov_size)
					.expect("pov_size always exists; qed");
				let mut candidate_receipt =
					test_state.candidate_receipt_templates[candidate_index].clone();

				// Make it unique.
				candidate_receipt
					.descriptor
					.set_relay_parent(Hash::from_low_u64_be(index as u64));
				// Store the new candidate in the state
				test_state.candidate_hashes.insert(candidate_receipt.hash(), candidate_index);

				let core_index = (index % config.n_cores) as u32;
				test_state
					.candidate_hash_to_core_index
					.insert(candidate_receipt.hash(), core_index.into());

				gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_receipt.hash(), "new candidate");

				candidate_receipt
			})
			.collect::<Vec<_>>()
			.into_iter()
			.cycle();

		// Prepare per block candidates.
		// Genesis block is always finalized, so we start at 1.
		for info in test_state.block_infos.iter() {
			for _ in 0..config.n_cores {
				let receipt = test_state.candidates.next().expect("Cycle iterator");
				test_state.candidate_receipts.entry(info.hash).or_default().push(receipt);
			}

			// First candidate is our backed candidate.
			test_state.backed_candidates.push(
				test_state
					.candidate_receipts
					.get(&info.hash)
					.expect("just inserted above")
					.first()
					.expect("just inserted above")
					.clone(),
			);
		}

		test_state.chunk_fetching_requests = test_state
			.backed_candidates
			.iter()
			.map(|candidate| {
				(0..config.n_validators)
					.map(|index| {
						ChunkFetchingRequest {
							candidate_hash: candidate.hash(),
							index: ValidatorIndex(index as u32),
						}
						.encode()
					})
					.collect::<Vec<_>>()
			})
			.collect::<Vec<_>>();

		test_state.signed_bitfields = test_state
			.block_infos
			.iter()
			.map(|block_info| {
				let signing_context =
					SigningContext { session_index: 0, parent_hash: block_info.hash };
				let messages = (0..config.n_validators)
					.map(|index| {
						let validator_public = test_state
							.test_authorities
							.validator_public
							.get(index)
							.expect("All validator keys are known");

						// Node has all the chunks in the world.
						let payload: AvailabilityBitfield =
							AvailabilityBitfield(bitvec![u8, bitvec::order::Lsb0; 1u8; 32]);
						let signed_bitfield = Signed::<AvailabilityBitfield>::sign(
							&test_state.test_authorities.keyring.keystore(),
							payload,
							&signing_context,
							ValidatorIndex(index as u32),
							validator_public,
						)
						.ok()
						.flatten()
						.expect("should be signed");

						peer_bitfield_message_v2(block_info.hash, signed_bitfield)
					})
					.collect::<Vec<_>>();

				(block_info.hash, messages)
			})
			.collect();

		gum::info!(target: LOG_TARGET, "{}","Created test environment.".bright_blue());

		test_state
	}
}

fn peer_bitfield_message_v2(
	relay_hash: H256,
	signed_bitfield: Signed<AvailabilityBitfield>,
) -> VersionedValidationProtocol {
	let bitfield = polkadot_node_network_protocol::v2::BitfieldDistributionMessage::Bitfield(
		relay_hash,
		signed_bitfield.into(),
	);

	Versioned::V2(polkadot_node_network_protocol::v2::ValidationProtocol::BitfieldDistribution(
		bitfield,
	))
}
