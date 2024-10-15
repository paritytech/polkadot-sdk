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

//! Contains the [`OverheadCmd`] as entry point for the CLI to execute
//! the *overhead* benchmarks.

use super::remark_builders::*;
use crate::{
	extrinsic::{
		bench::{Benchmark, BenchmarkParams as ExtrinsicBenchmarkParams},
		ExtrinsicBuilder,
	},
	overhead::{
		cmd::ChainType::{Parachain, Relaychain, Unknown},
		fake_runtime_api,
		template::TemplateData,
	},
	shared::{self, GenesisBuilderPolicy, HostInfoParams, WeightParams},
};
use clap::{Args, Parser};
use codec::{Decode, Encode};
use cumulus_client_parachain_inherent::MockValidationDataInherentDataProvider;
use fake_runtime_api::RuntimeApi as FakeRuntimeApi;
use frame_support::Deserialize;
use log::info;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sc_block_builder::BlockBuilderApi;
use sc_chain_spec::{ChainSpec, ChainSpecExtension, GenericChainSpec, GenesisBlockBuilder};
use sc_cli::{CliConfiguration, ImportParams, Result, SharedParams};
use sc_client_api::{execution_extensions::ExecutionExtensions, UsageProvider};
use sc_client_db::{BlocksPruning, DatabaseSettings, DatabaseSource};
use sc_executor::WasmExecutor;
use sc_service::{new_client, new_db_backend, BasePath, ClientConfig, TFullClient, TaskManager};
use serde::Serialize;
use sp_api::{ApiExt, CallApiAt, Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, Block as BlockT},
	DigestItem, OpaqueExtrinsic,
};
use sp_wasm_interface::HostFunctions;
use std::{fmt::Debug, path::PathBuf, sync::Arc};
use subxt::{ext::futures, Metadata};

const DEFAULT_PARA_ID: u32 = 100;

/// Benchmark the execution overhead per-block and per-extrinsic.
#[derive(Debug, Parser)]
pub struct OverheadCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub import_params: ImportParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub params: OverheadParams,
}

/// Configures the benchmark, the post-processing and weight generation.
#[derive(Debug, Default, Serialize, Clone, PartialEq, Args)]
pub struct OverheadParams {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub weight: WeightParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub bench: ExtrinsicBenchmarkParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub hostinfo: HostInfoParams,

	/// Add a header to the generated weight output file.
	///
	/// Good for adding LICENSE headers.
	#[arg(long, value_name = "PATH")]
	pub header: Option<PathBuf>,

	/// Enable the Trie cache.
	///
	/// This should only be used for performance analysis and not for final results.
	#[arg(long)]
	pub enable_trie_cache: bool,

	/// Optional runtime blob to use instead of the one from the genesis config.
	#[arg(long, value_name = "PATH")]
	pub runtime: Option<PathBuf>,

	/// The preset that we expect to find in the GenesisBuilder runtime API.
	///
	/// This can be useful when a runtime has a dedicated benchmarking preset instead of using the
	/// default one.
	#[arg(long, default_value = sp_genesis_builder::DEV_RUNTIME_PRESET)]
	pub genesis_builder_preset: String,

	/// How to construct the genesis state.
	///
	/// Uses `GenesisBuilderPolicy::Spec` by default and  `GenesisBuilderPolicy::Runtime` if
	/// `runtime` is set.
	#[arg(long, value_enum)]
	pub genesis_builder: Option<GenesisBuilderPolicy>,

	/// Inflate the genesis state by generating accounts.
	///
	/// Can be used together with `--account_type` to specify the type of accounts to generate.
	#[arg(long)]
	pub generate_num_accounts: Option<u64>,

	/// Parachain Id to use for parachains. If not specified, the benchmark code will choose
	/// a para-id and patch the state accordingly.
	#[arg(long)]
	pub para_id: Option<u32>,
}

/// Type of a benchmark.
#[derive(Serialize, Clone, PartialEq, Copy)]
pub(crate) enum BenchmarkType {
	/// Measure the per-extrinsic execution overhead.
	Extrinsic,
	/// Measure the per-block execution overhead.
	Block,
}

/// Hostfunctions that are typically used by parachains.
pub type ParachainHostFunctions = (
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
	sp_io::SubstrateHostFunctions,
);

pub type BlockNumber = u32;

/// Typical block type using `OpaqueExtrinsic`.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Typical block type using `OpaqueExtrinsic`.
pub type OpaqueBlock = generic::Block<Header, OpaqueExtrinsic>;

/// Client type used throughout the benchmarking code.
type OverheadClient<Block, HF> = TFullClient<Block, FakeRuntimeApi, WasmExecutor<HF>>;

/// Creates inherent data for a given parachain ID.
///
/// This function constructs the inherent data required for block execution,
/// including the relay chain state and validation data. Not all of these
/// inherents are required for every chain. The runtime will pick the ones
/// it requires based on their identifier.
fn create_inherent_data<Client: UsageProvider<Block> + HeaderBackend<Block>, Block: BlockT>(
	client: &Arc<Client>,
	chain_type: &ChainType,
) -> InherentData {
	let genesis = client.usage_info().chain.best_hash;
	let header = client.header(genesis).unwrap().unwrap();

	let mut inherent_data = sp_inherents::InherentData::new();

	// Para inherent can only makes sense when we are handling a parachain.
	if let Parachain(para_id) = chain_type {
		let parachain_validation_data_provider = MockValidationDataInherentDataProvider::<()> {
			para_id: ParaId::from(*para_id),
			current_para_block_head: Some(header.encode().into()),
			relay_offset: 1,
			..Default::default()
		};
		let _ = futures::executor::block_on(
			parachain_validation_data_provider.provide_inherent_data(&mut inherent_data),
		);
	}

	// Parachain inherent that is used on relay chains to perform parachain validation.
	let para_inherent = polkadot_primitives::InherentData {
		bitfields: Vec::new(),
		backed_candidates: Vec::new(),
		disputes: Vec::new(),
		parent_header: header,
	};

	// Timestamp inherent that is very common in substrate chains.
	let timestamp = sp_timestamp::InherentDataProvider::new(std::time::Duration::default().into());

	let _ = futures::executor::block_on(timestamp.provide_inherent_data(&mut inherent_data));
	let _ =
		inherent_data.put_data(polkadot_primitives::PARACHAINS_INHERENT_IDENTIFIER, &para_inherent);

	inherent_data
}

#[derive(Deserialize, Serialize, Clone, ChainSpecExtension)]
pub struct ParachainExtension {
	/// The id of the Parachain.
	pub para_id: Option<u32>,
}

impl OverheadCmd {
	/// Make an educated guess what kind of chain we are dealing with.
	fn identify_chain(&self, metadata: &Metadata, para_id: Option<u32>) -> Result<ChainType> {
		let parachain_info_exists = metadata.pallet_by_name("ParachainInfo").is_some();
		let parachain_system_exists = metadata.pallet_by_name("ParachainSystem").is_some();
		let para_inherent_exists = metadata.pallet_by_name("ParaInherent").is_some();

		log::info!("Identifying chain type based on metadata.");
		log::info!("{} ParachainSystem", if parachain_system_exists { "✅" } else { "❌" });
		log::info!("{} ParachainInfo", if parachain_info_exists { "✅" } else { "❌" });
		log::info!("{} ParaInherent", if para_inherent_exists { "✅" } else { "❌" });

		if parachain_system_exists && parachain_info_exists {
			log::info!("Parachain Identified");
			Ok(Parachain(para_id.or(self.params.para_id).unwrap_or(DEFAULT_PARA_ID)))
		} else if para_inherent_exists {
			log::info!("Relaychain Identified");
			Ok(Relaychain)
		} else {
			log::info!("Found Custom chain");
			Ok(Unknown)
		}
	}
	fn chain_spec_from_path<HF: HostFunctions>(
		&self,
	) -> Result<(Option<Box<dyn ChainSpec>>, Option<u32>)> {
		let chain_spec = self
			.shared_params
			.chain
			.clone()
			.map(|path| {
				GenericChainSpec::<ParachainExtension, HF>::from_json_file(path.into())
					.map_err(|e| format!("Unable to load chain spec: {:?}", e))
			})
			.transpose()?;

		let para_id_from_chain_spec =
			chain_spec.as_ref().and_then(|spec| spec.extensions().para_id);
		Ok((chain_spec.map(|c| Box::new(c) as Box<_>), para_id_from_chain_spec))
	}

	/// Run the benchmark overhead command.
	pub fn run_with_extrinsic_builder<Block, ExtraHF>(
		&self,
		ext_builder_provider: Option<
			Box<
				dyn FnOnce(
					subxt::Metadata,
					H256,
					subxt::client::RuntimeVersion,
				) -> Box<dyn ExtrinsicBuilder>,
			>,
		>,
	) -> Result<()>
	where
		Block: BlockT<Extrinsic = OpaqueExtrinsic, Hash = H256>,
		ExtraHF: HostFunctions,
	{
		let (chain_spec, para_id_from_chain_spec) =
			self.chain_spec_from_path::<(ParachainHostFunctions, ExtraHF)>()?;
		let code_bytes = shared::genesis_state::get_code_bytes(&chain_spec, &self.params.runtime)?;

		let executor = WasmExecutor::<(ParachainHostFunctions, ExtraHF)>::builder()
			.with_allow_missing_host_functions(true)
			.build();

		let opaque_metadata = fetch_latest_metadata_from_blob(&executor, &code_bytes)?;
		let metadata = Metadata::decode(&mut (*opaque_metadata).as_slice())?;
		let chain_type = self.identify_chain(&metadata, para_id_from_chain_spec)?;

		let client = self.build_client_components::<Block, (ParachainHostFunctions, ExtraHF)>(
			chain_spec,
			&code_bytes,
			executor,
			&chain_type,
		)?;

		let inherent_data = create_inherent_data(&client, &chain_type);

		let ext_builder = {
			let genesis = client.usage_info().chain.best_hash;
			let version = client.runtime_api().version(genesis).unwrap();
			let runtime_version = subxt::client::RuntimeVersion {
				spec_version: version.spec_version,
				transaction_version: version.transaction_version,
			};

			match ext_builder_provider {
				Some(provider) => provider(metadata, genesis, runtime_version),
				None => {
					let genesis = subxt::utils::H256::from(genesis.to_fixed_bytes());
					Box::new(SubstrateRemarkBuilder::new(metadata, genesis, runtime_version))
					as Box<_>},
			}
		};

		self.run(
			"Overhead Benchmark".to_string(),
			client,
			inherent_data,
			Default::default(),
			&*ext_builder,
			chain_type.requires_proof_recording(),
		)
	}

	fn build_client_components<Block: BlockT, HF: HostFunctions>(
		&self,
		chain_spec: Option<Box<dyn ChainSpec>>,
		code_bytes: &Vec<u8>,
		executor: WasmExecutor<HF>,
		chain_type: &ChainType,
	) -> Result<Arc<OverheadClient<Block, HF>>> {
		let extensions = ExecutionExtensions::new(None, Arc::new(executor.clone()));

		let backend = new_db_backend(DatabaseSettings {
			trie_cache_maximum_size: self.trie_cache_maximum_size()?,
			state_pruning: None,
			blocks_pruning: BlocksPruning::KeepAll,
			source: DatabaseSource::RocksDb {
				cache_size: self.database_cache_size()?.unwrap_or(1024),
				path: BasePath::new_temp_dir()?.path().into(),
			},
		})?;

		let storage = shared::genesis_state::genesis_storage::<HF>(
			self.params.genesis_builder,
			&self.params.runtime,
			Some(&code_bytes),
			&self.params.genesis_builder_preset,
			&chain_spec,
		)?;

		let genesis_block_builder = GenesisBlockBuilder::new_with_storage(
			storage,
			true,
			backend.clone(),
			executor.clone(),
		)?;

		let tokio_runtime = sc_cli::build_runtime()?;
		let task_manager = TaskManager::new(tokio_runtime.handle().clone(), None)
			.map_err(|_| "Unable to build task manager")?;

		let client: Arc<OverheadClient<Block, HF>> = Arc::new(new_client(
			backend.clone(),
			executor,
			genesis_block_builder,
			Default::default(),
			Default::default(),
			extensions,
			Box::new(task_manager.spawn_handle()),
			None,
			None,
			ClientConfig {
				offchain_worker_enabled: false,
				offchain_indexing_api: false,
				wasm_runtime_overrides: None,
				no_genesis: false,
				wasm_runtime_substitutes: Default::default(),
				enable_import_proof_recording: chain_type.requires_proof_recording(),
			},
		)?);

		Ok(client)
	}

	/// Measure the per-block and per-extrinsic execution overhead.
	///
	/// Writes the results to console and into two instances of the
	/// `weights.hbs` template, one for each benchmark.
	pub fn run<Block, C>(
		&self,
		chain_name: String,
		client: Arc<C>,
		inherent_data: sp_inherents::InherentData,
		digest_items: Vec<DigestItem>,
		ext_builder: &dyn ExtrinsicBuilder,
		should_record_proof: bool,
	) -> Result<()>
	where
		Block: BlockT<Extrinsic = OpaqueExtrinsic>,
		C: ProvideRuntimeApi<Block>
			+ CallApiAt<Block>
			+ UsageProvider<Block>
			+ sp_blockchain::HeaderBackend<Block>,
		C::Api: ApiExt<Block> + BlockBuilderApi<Block>,
	{
		if ext_builder.pallet() != "system" || ext_builder.extrinsic() != "remark" {
			return Err(format!("The extrinsic builder is required to build `System::Remark` extrinsics but builds `{}` extrinsics instead", ext_builder.name()).into());
		}

		let bench = Benchmark::new(
			client,
			self.params.bench.clone(),
			inherent_data,
			digest_items,
			should_record_proof,
		);

		// per-block execution overhead
		{
			let (stats, proof_size) = bench.bench_block()?;
			info!("Per-block execution overhead [ns]:\n{:?}", stats);
			let template = TemplateData::new(
				BenchmarkType::Block,
				&chain_name,
				&self.params,
				&stats,
				proof_size,
			)?;
			template.write(&self.params.weight.weight_path)?;
		}
		// per-extrinsic execution overhead
		{
			let (stats, proof_size) = bench.bench_extrinsic(ext_builder)?;
			info!("Per-extrinsic execution overhead [ns]:\n{:?}", stats);
			let template = TemplateData::new(
				BenchmarkType::Extrinsic,
				&chain_name,
				&self.params,
				&stats,
				proof_size,
			)?;
			template.write(&self.params.weight.weight_path)?;
		}

		Ok(())
	}
}

impl BenchmarkType {
	/// Short name of the benchmark type.
	pub(crate) fn short_name(&self) -> &'static str {
		match self {
			Self::Extrinsic => "extrinsic",
			Self::Block => "block",
		}
	}

	/// Long name of the benchmark type.
	pub(crate) fn long_name(&self) -> &'static str {
		match self {
			Self::Extrinsic => "ExtrinsicBase",
			Self::Block => "BlockExecution",
		}
	}
}

#[derive(Clone)]
enum ChainType {
	Parachain(u32),
	Relaychain,
	Unknown,
}

impl ChainType {
	fn requires_proof_recording(&self) -> bool {
		match self {
			Parachain(_) => true,
			Relaychain => false,
			Unknown => false,
		}
	}
}

// Boilerplate
impl CliConfiguration for OverheadCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}

	fn import_params(&self) -> Option<&ImportParams> {
		Some(&self.import_params)
	}

	fn trie_cache_maximum_size(&self) -> Result<Option<usize>> {
		if self.params.enable_trie_cache {
			Ok(self.import_params().map(|x| x.trie_cache_maximum_size()).unwrap_or_default())
		} else {
			Ok(None)
		}
	}

	fn base_path(&self) -> Result<Option<BasePath>> {
		Ok(Some(BasePath::new_temp_dir()?))
	}
}
