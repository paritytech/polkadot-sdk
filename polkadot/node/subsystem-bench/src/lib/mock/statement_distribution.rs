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

//! A generic statement distribution subsystem mockup suitable to be used in benchmarks.

use crate::mock::runtime_api::session_info_for_peers;
use futures::FutureExt;
use itertools::Itertools;
use polkadot_node_network_protocol::{
	request_response::{
		v2::{AttestedCandidateRequest, AttestedCandidateResponse},
		IncomingRequestReceiver,
	},
	UnifiedReputationChange,
};
use polkadot_node_primitives::{AvailableData, BlockData, PoV};
use polkadot_node_subsystem::{
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_test_helpers::{
	derive_erasure_chunks_with_proofs_and_root, mock::new_block_import_info,
};
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{
	vstaging::{CandidateReceiptV2, CommittedCandidateReceiptV2, MutateDescriptorV2},
	BlockNumber, CandidateHash, CompactStatement, CoreIndex, Hash, Id, PersistedValidationData,
	SignedStatement, SigningContext, UncheckedSigned, ValidatorIndex, ValidatorPair,
};
use polkadot_primitives_test_helpers::{
	dummy_committed_candidate_receipt_v2, dummy_hash, dummy_head_data, dummy_pvd,
};
use sp_application_crypto::Pair;
use sp_core::H256;
use std::{collections::HashMap, sync::Arc};

use crate::configuration::TestConfiguration;

const LOG_TARGET: &str = "subsystem-bench::statement-distribution-mock";
const COST_INVALID_REQUEST: UnifiedReputationChange =
	UnifiedReputationChange::CostMajor("Peer sent unparsable request");
const SESSION_INDEX: u32 = 0;

pub struct MockStatementDistribution {
	/// Receiver for attested candidate requests.
	req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
	// Map from generated commited candidate receipts
	pub commited_candidate_receipts: HashMap<H256, Vec<CommittedCandidateReceiptV2>>,
	// PersistedValidationData, we use one for all candidates
	pub pvd: PersistedValidationData,
	// Pregenerated statements
	pub statements: HashMap<CandidateHash, Vec<UncheckedSigned<CompactStatement>>>,
}

impl MockStatementDistribution {
	pub fn new(
		req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
		config: &TestConfiguration,
	) -> Self {
		let pvd = dummy_pvd(dummy_head_data(), 0);
		let mut statements: HashMap<CandidateHash, Vec<UncheckedSigned<CompactStatement>>> =
			Default::default();
		let mut candidate_receipts: HashMap<H256, Vec<CandidateReceiptV2>> = Default::default();
		let mut commited_candidate_receipts: HashMap<H256, Vec<CommittedCandidateReceiptV2>> =
			Default::default();
		let test_authorities = config.generate_authorities();
		let session_info = session_info_for_peers(config, &test_authorities);

		// For each unique pov we create a candidate receipt.
		let pov_sizes = Vec::from(config.pov_sizes()); // For n_cores
		let pov_size_to_candidate = generate_pov_size_to_candidate(&pov_sizes);
		let receipt_templates =
			generate_receipt_templates(&pov_size_to_candidate, config.n_validators, &pvd);
		let block_infos: Vec<_> = (1..=config.num_blocks).map(generate_block_info).collect();

		for block_info in block_infos.iter() {
			for core_idx in 0..config.n_cores {
				let pov_size = pov_sizes.get(core_idx).expect("This is a cycle; qed");
				let candidate_index =
					*pov_size_to_candidate.get(pov_size).expect("pov_size always exists; qed");
				let mut receipt = receipt_templates[candidate_index].clone();
				receipt.descriptor.set_para_id(Id::new(core_idx as u32 + 1));
				receipt.descriptor.set_relay_parent(block_info.hash);
				receipt.descriptor.set_core_index(CoreIndex(core_idx as u32));
				receipt.descriptor.set_session_index(SESSION_INDEX);

				candidate_receipts.entry(block_info.hash).or_default().push(CandidateReceiptV2 {
					descriptor: receipt.descriptor.clone(),
					commitments_hash: receipt.commitments.hash(),
				});
				commited_candidate_receipts.entry(block_info.hash).or_default().push(receipt);
			}
		}

		let groups = session_info.validator_groups.clone();

		for block_info in block_infos.iter() {
			for (index, group) in groups.iter().enumerate() {
				let candidate =
					candidate_receipts.get(&block_info.hash).unwrap().get(index).unwrap();
				let group_statements = group
					.iter()
					.map(|&v| {
						sign_statement(
							CompactStatement::Seconded(candidate.hash()),
							block_info.hash,
							v,
							test_authorities.validator_pairs.get(v.0 as usize).unwrap(),
						)
					})
					.collect_vec();
				statements.insert(candidate.hash(), group_statements);
			}
		}

		Self { req_receiver, commited_candidate_receipts, pvd, statements }
	}
}

fn generate_block_info(block_num: usize) -> BlockInfo {
	new_block_import_info(Hash::repeat_byte(block_num as u8), block_num as BlockNumber)
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
) -> Vec<CommittedCandidateReceiptV2> {
	pov_size_to_candidate
		.iter()
		.map(|(&pov_size, &index)| {
			let mut receipt = dummy_committed_candidate_receipt_v2(dummy_hash());
			let (_, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				n_validators,
				&AvailableData {
					validation_data: pvd.clone(),
					pov: Arc::new(PoV { block_data: BlockData(vec![index as u8; pov_size]) }),
				},
				|_, _| {},
			);
			receipt.descriptor.set_persisted_validation_data_hash(pvd.hash());
			receipt.descriptor.set_erasure_root(erasure_root);
			receipt
		})
		.collect()
}

fn sign_statement(
	statement: CompactStatement,
	relay_parent: H256,
	validator_index: ValidatorIndex,
	pair: &ValidatorPair,
) -> UncheckedSigned<CompactStatement> {
	let context = SigningContext { parent_hash: relay_parent, session_index: SESSION_INDEX };
	let payload = statement.signing_payload(&context);

	SignedStatement::new(
		statement,
		validator_index,
		pair.sign(&payload[..]),
		&context,
		&pair.public(),
	)
	.unwrap()
	.as_unchecked()
	.to_owned()
}

#[overseer::subsystem(StatementDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockStatementDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();
		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(StatementDistribution, prefix = self::overseer)]
impl MockStatementDistribution {
	async fn run<Context>(mut self, mut ctx: Context) {
		loop {
			tokio::select! {
				msg = ctx.recv() => match msg {
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) => return,
					Ok(FromOrchestra::Communication { msg }) =>
						println!("🚨🚨🚨 Received message: {:?}", msg),
					err => println!("🚨🚨🚨 recv error: {:?}", err),
				},
				req = self.req_receiver.recv(|| vec![COST_INVALID_REQUEST]) => {
					let req = req.expect("Receiver never fails");
					println!("🚨🚨🚨 Received candidate request: {:?}", req);
					let candidate_receipt = self
						.commited_candidate_receipts
						.values()
						.flatten()
						.find(|v| v.hash() == req.payload.candidate_hash)
						.unwrap()
						.clone();
					let persisted_validation_data = self.pvd.clone();
					let statements = self.statements.get(&req.payload.candidate_hash).unwrap().clone();
					let res = AttestedCandidateResponse {
						candidate_receipt,
						persisted_validation_data,
						statements,
					};
					let _ = req.send_response(res);
				}
			}
		}
	}
}
