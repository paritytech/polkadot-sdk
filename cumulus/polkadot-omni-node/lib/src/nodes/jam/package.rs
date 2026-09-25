// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Work-package assembly shared by the author and the foreign-package path.
//!
//! A block this collator authored and a block another collator authored are assembled into the
//! same [`WorkPackage`] shape: the para's service and authorizer settings, a `ParachainCandidate`
//! payload carrying only the validation-code hash, and the PoV as work-item extrinsic 0. The
//! author has the block and its proof; the foreign path rebuilds the same package from the
//! information a peer synced plus the locally re-executed proof.

use codec::Encode;
use cumulus_primitives_core::{ParachainBlockData, SchedulingProof};
use jam_interface::{JamChainSource, ServiceId, VersionedParameters, WorkPackage, WorkPackageHash};
use jam_types::{
	Authorization, CodeHash, ExtrinsicSpec, RefineContext, UnsignedGas, WorkItem, WorkPayload,
};
use parachain_service_core::{authorizer::Authorizer, candidate::ParachainCandidate};
use sp_additional_data::AdditionalData;
use sp_runtime::traits::Block as BlockT;
use sp_trie::CompactProof;
use std::time::Duration;

use super::LOG_TARGET;

/// How long to wait before retrying a JAM read that failed at startup.
const RETRY_DELAY: Duration = Duration::from_secs(6);

/// The parachain service settings every work package of this para is built with.
///
/// Read once at startup from JAM and shared by the authoring path and the foreign-package
/// acceptor, so both assemble byte-identical items.
#[derive(Clone, Debug)]
pub(crate) struct PackageParams {
	/// The parachain service id.
	pub(crate) service_id: ServiceId,
	/// The service's code hash at the anchor, as JAM holds it.
	pub(crate) service_code_hash: CodeHash,
	/// The refine gas limit from the JAM chain parameters.
	pub(crate) refine_gas_limit: UnsignedGas,
	/// The accumulate gas limit from the JAM chain parameters.
	pub(crate) accumulate_gas_limit: UnsignedGas,
}

/// Read the chain parameters and the parachain service's code hash, retrying until JAM answers.
///
/// Extracted from the collation task's startup reads: it blocks until the service is registered
/// and JAM's parameters are readable, because neither has a sane default and a collator that
/// guessed would author packages no core accepts.
pub(crate) async fn read_package_params<
	Jam: JamChainSource + jam_interface::JamStateSource + ?Sized,
>(
	jam: &Jam,
	service_id: ServiceId,
) -> PackageParams {
	let (refine_gas_limit, accumulate_gas_limit) = loop {
		match jam.parameters().await {
			Ok(VersionedParameters::V1(parameters)) => {
				break (parameters.max_refine_gas, parameters.max_accumulate_gas);
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?error,
					"Unable to fetch JAM chain parameters; retrying.",
				);
				tokio::time::sleep(RETRY_DELAY).await;
			},
		}
	};

	let service_code_hash = loop {
		let result = match jam.best_block().await {
			Ok(best) => jam.service_info(best.header_hash, service_id).await,
			Err(error) => Err(error),
		};
		match result {
			Ok(Some(service)) => {
				tracing::info!(
					target: LOG_TARGET,
					service_id,
					code_hash = ?service.code_hash,
					balance = service.balance,
					"Found the parachain service on JAM.",
				);
				break service.code_hash;
			},
			Ok(None) => {
				tracing::info!(
					target: LOG_TARGET,
					service_id,
					"Parachain service not registered on JAM yet; waiting.",
				);
				tokio::time::sleep(RETRY_DELAY).await;
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					service_id,
					?error,
					"Unable to read the parachain service info; retrying.",
				);
				tokio::time::sleep(RETRY_DELAY).await;
			},
		}
	};

	PackageParams { service_id, service_code_hash, refine_gas_limit, accumulate_gas_limit }
}

/// Assemble the work package for `context`, carrying `spec` as its single extrinsic.
///
/// The payload's `ParachainCandidate` keeps only the `validation_code_hash` — the parachain
/// service still reads that — and no PoV. The PoV travels as work-item extrinsic 0 instead: CE 133
/// caps the first message at 200 KiB, while extrinsics ride the bulk channel.
pub(crate) fn work_package(
	spec: ExtrinsicSpec,
	validation_code_hash: [u8; 32],
	params: &PackageParams,
	authorizer: Authorizer,
	context: RefineContext,
) -> WorkPackage {
	let payload = ParachainCandidate {
		validation_code_hash: parachain_service_core::types::ValidationCodeHash(
			validation_code_hash.into(),
		),
	}
	.encode();

	let work_item = WorkItem {
		service: params.service_id,
		code_hash: params.service_code_hash,
		payload: WorkPayload(payload),
		refine_gas_limit: params.refine_gas_limit,
		accumulate_gas_limit: params.accumulate_gas_limit,
		import_segments: Default::default(),
		extrinsics: vec![spec].try_into().expect("a single extrinsic always fits; qed"),
		export_count: 0,
	};

	WorkPackage {
		authorization: Authorization::default(),
		auth_code_host: params.service_id,
		authorizer,
		context,
		items: vec![work_item].try_into().expect("a single work item always fits; qed"),
	}
}

/// The extrinsic spec naming `pov`: its `blake2b-256` hash and length.
pub(crate) fn extrinsic_spec(pov: &[u8]) -> ExtrinsicSpec {
	ExtrinsicSpec { hash: jam_std_common::hash_raw(pov).into(), len: pov.len() as u32 }
}

/// The extrinsic spec of the PoV built from `blocks`, `proof`, `parent_header` and
/// `additional_data`.
pub(crate) fn pov_spec<Block: BlockT>(
	blocks: &[Block],
	proof: &CompactProof,
	parent_header: &Block::Header,
	additional_data: &AdditionalData,
) -> ExtrinsicSpec {
	extrinsic_spec(&build_pov(blocks, proof, parent_header, additional_data))
}

/// The PoV: a V4 [`cumulus_primitives_core::ParachainBlockData`] carrying the SCALE-encoded parent
/// header of `blocks[0]` and the additional-data map assembled at build time.
///
/// The scheduling proof is empty — JAM has no relay-chain scheduling. The PoV is not
/// zstd-compressed; JIP-2 is silent on compression and the service refuses compressed PoVs.
pub(crate) fn build_pov<Block: BlockT>(
	blocks: &[Block],
	proof: &CompactProof,
	parent_header: &Block::Header,
	additional_data: &AdditionalData,
) -> Vec<u8> {
	ParachainBlockData::new_with_parent_header(
		blocks.to_vec(),
		proof.clone(),
		SchedulingProof::empty(),
		blocks.iter().map(|_| Some(additional_data.clone())).collect(),
		parent_header.encode(),
	)
	.encode()
}

/// The hash JAM keys a work package by: blake2b-256 over its encoding.
///
/// polkajam derives it inside its bundle builder, which phase 5a no longer uses; a test pins the
/// two against each other so the status subscriptions keep naming the package the node sees.
pub(crate) fn work_package_hash(package: &WorkPackage) -> WorkPackageHash {
	WorkPackageHash::from(sp_crypto_hashing::blake2_256(&jam_codec::Encode::encode(package)))
}
