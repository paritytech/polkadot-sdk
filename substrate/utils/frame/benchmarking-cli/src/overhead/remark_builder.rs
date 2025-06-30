// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::extrinsic::ExtrinsicBuilder;
use codec::Decode;
use sc_client_api::UsageProvider;
use sp_api::{ApiExt, Core, Metadata, ProvideRuntimeApi};
use sp_runtime::{traits::Block as BlockT, OpaqueExtrinsic};
use std::sync::Arc;
use subxt::{
	client::RuntimeVersion as SubxtRuntimeVersion,
	config::substrate::SubstrateExtrinsicParamsBuilder, Config, OfflineClient, SubstrateConfig,
};

pub type SubstrateRemarkBuilder = DynamicRemarkBuilder<SubstrateConfig>;

/// Remark builder that can be used to build simple extrinsics for
/// FRAME-based runtimes.
pub struct DynamicRemarkBuilder<C: Config> {
	offline_client: OfflineClient<C>,
}

impl<C: Config<Hash = subxt::utils::H256>> DynamicRemarkBuilder<C> {
	/// Initializes a new remark builder from a client.
	///
	/// This will first fetch metadata and runtime version from the runtime and then
	/// construct an offline client that provides the extrinsics.
	pub fn new_from_client<Client, Block>(client: Arc<Client>) -> sc_cli::Result<Self>
	where
		Block: BlockT<Hash = sp_core::H256>,
		Client: UsageProvider<Block> + ProvideRuntimeApi<Block>,
		Client::Api: Metadata<Block> + Core<Block>,
	{
		let genesis = client.usage_info().chain.best_hash;
		let api = client.runtime_api();

		let Ok(Some(metadata_api_version)) = api.api_version::<dyn Metadata<Block>>(genesis) else {
			return Err("Unable to fetch metadata runtime API version.".to_string().into());
		};

		log::debug!("Found metadata API version {}.", metadata_api_version);
		let opaque_metadata = if metadata_api_version > 1 {
			let Ok(supported_metadata_versions) = api.metadata_versions(genesis) else {
				return Err("Unable to fetch metadata versions".to_string().into());
			};

			let latest = supported_metadata_versions
				.into_iter()
				// TODO: Subxt doesn't support V16 metadata until v0.42.0, so don't try
				// to fetch it here until we update to that version.
				.filter(|v| *v != u32::MAX && *v < 16)
				.max()
				.ok_or("No stable metadata versions supported".to_string())?;

			api.metadata_at_version(genesis, latest)
				.map_err(|e| format!("Unable to fetch metadata: {:?}", e))?
				.ok_or("Unable to decode metadata".to_string())?
		} else {
			// Fall back to using the non-versioned metadata API.
			api.metadata(genesis)
				.map_err(|e| format!("Unable to fetch metadata: {:?}", e))?
		};

		let version = api.version(genesis).unwrap();
		let runtime_version = SubxtRuntimeVersion {
			spec_version: version.spec_version,
			transaction_version: version.transaction_version,
		};
		let metadata = subxt::Metadata::decode(&mut (*opaque_metadata).as_slice())?;
		let genesis = subxt::utils::H256::from(genesis.to_fixed_bytes());

		Ok(Self { offline_client: OfflineClient::new(genesis, runtime_version, metadata) })
	}
}

impl<C: Config> DynamicRemarkBuilder<C> {
	/// Constructs a new remark builder.
	pub fn new(
		metadata: subxt::Metadata,
		genesis_hash: C::Hash,
		runtime_version: SubxtRuntimeVersion,
	) -> Self {
		Self { offline_client: OfflineClient::new(genesis_hash, runtime_version, metadata) }
	}
}

impl ExtrinsicBuilder for DynamicRemarkBuilder<SubstrateConfig> {
	fn pallet(&self) -> &str {
		"system"
	}

	fn extrinsic(&self) -> &str {
		"remark"
	}

	fn build(&self, nonce: u32) -> std::result::Result<OpaqueExtrinsic, &'static str> {
		let signer = subxt_signer::sr25519::dev::alice();
		let dynamic_tx = subxt::dynamic::tx("System", "remark", vec![Vec::<u8>::new()]);

		let params = SubstrateExtrinsicParamsBuilder::new().nonce(nonce.into()).build();

		// Default transaction parameters assume a nonce of 0.
		let transaction = self
			.offline_client
			.tx()
			.create_partial_offline(&dynamic_tx, params)
			.unwrap()
			.sign(&signer);
		let mut encoded = transaction.into_encoded();

		OpaqueExtrinsic::from_bytes(&mut encoded).map_err(|_| "Unable to construct OpaqueExtrinsic")
	}
}
