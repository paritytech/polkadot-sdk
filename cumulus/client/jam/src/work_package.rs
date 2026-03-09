// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Error, JamClient};
use jam_codec::Encode;
use jam_std_common::hash_raw;
use jam_types::{
	AuthConfig, Authorization, Authorizer, BoundedVec, CodeHash, CoreIndex, HeaderHash,
	RefineContext, ServiceId, UnsignedGas, VecSet, WorkItem, WorkPackage, WorkPackageHash,
};

/// Builds JAM work packages from parachain data.
///
/// Fetches the required chain context (anchor, state root, beefy root, finalized block)
/// from a JAM node and assembles a `WorkPackage` ready for submission.
pub struct WorkPackageBuilder {
	service_id: ServiceId,
	service_code_hash: CodeHash,
	authorizer_code_hash: CodeHash,
	auth_code_host: ServiceId,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
	core: CoreIndex,
}

impl WorkPackageBuilder {
	pub fn new(service_id: ServiceId, service_code_hash: CodeHash) -> Self {
		Self {
			service_id,
			service_code_hash,
			authorizer_code_hash: CodeHash::default(),
			auth_code_host: 0,
			refine_gas_limit: jam_types::max_refine_gas() / 10,
			accumulate_gas_limit: jam_types::max_accumulate_gas() / 10,
			core: 0,
		}
	}

	pub fn authorizer(mut self, code_hash: CodeHash, auth_code_host: ServiceId) -> Self {
		self.authorizer_code_hash = code_hash;
		self.auth_code_host = auth_code_host;
		self
	}

	pub fn core(mut self, core: CoreIndex) -> Self {
		self.core = core;
		self
	}

	pub fn gas_limits(mut self, refine: UnsignedGas, accumulate: UnsignedGas) -> Self {
		self.refine_gas_limit = refine;
		self.accumulate_gas_limit = accumulate;
		self
	}

	/// Build a work package by fetching chain context from the JAM node.
	///
	/// `payload` is the SCALE-encoded work item payload (e.g. a parachain instruction).
	pub async fn build(
		&self,
		client: &JamClient,
		payload: Vec<u8>,
	) -> Result<BuiltWorkPackage, Error> {
		let best_block = client.best_block().await?;
		let finalized_block = client.finalized_block().await?;
		let state_root = client.state_root(best_block.header_hash).await?;
		let beefy_root = client.beefy_root(best_block.header_hash).await?;

		let context = RefineContext {
			anchor: best_block.header_hash,
			state_root,
			beefy_root,
			lookup_anchor: finalized_block.header_hash,
			lookup_anchor_slot: finalized_block.slot,
			prerequisites: VecSet::new(),
		};

		let work_item = WorkItem {
			service: self.service_id,
			code_hash: self.service_code_hash,
			payload: payload.into(),
			refine_gas_limit: self.refine_gas_limit,
			accumulate_gas_limit: self.accumulate_gas_limit,
			import_segments: BoundedVec::new(),
			extrinsics: BoundedVec::new(),
			export_count: 0,
		};

		let work_package = WorkPackage {
			authorization: Authorization::new(),
			auth_code_host: self.auth_code_host,
			authorizer: Authorizer {
				code_hash: self.authorizer_code_hash,
				config: AuthConfig::new(),
			},
			context,
			items: vec![work_item]
				.try_into()
				.map_err(|_| Error::Submission("too many work items".into()))?,
		};

		let encoded = work_package.encode();
		let hash = WorkPackageHash(hash_raw(&encoded));

		Ok(BuiltWorkPackage {
			package: work_package,
			encoded: encoded.into(),
			hash,
			core: self.core,
			anchor: best_block.header_hash,
		})
	}
}

/// A fully constructed work package ready for submission.
pub struct BuiltWorkPackage {
	pub package: WorkPackage,
	pub encoded: bytes::Bytes,
	pub hash: WorkPackageHash,
	pub core: CoreIndex,
	pub anchor: HeaderHash,
}
