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

use sc_cli::{CliConfiguration, DatabaseParams, PruningParams, Result, SharedParams};
use sc_client_api::{Backend as ClientBackend, StorageProvider, UsageProvider};
use sc_client_db::DbHash;
use sc_service::Configuration;
use sp_api::CallApiAt;
use sp_blockchain::HeaderBackend;
use sp_database::{ColumnId, Database};
use sp_runtime::traits::{Block as BlockT, HashingFor};
use sp_state_machine::Storage;
use sp_storage::{ChildInfo, ChildType, PrefixedStorageKey, StateVersion};

use clap::{Args, Parser};
use log::info;
use rand::prelude::*;
use serde::Serialize;
use sp_runtime::generic::BlockId;
use std::{fmt::Debug, path::PathBuf, sync::Arc};

use super::template::TemplateData;
use crate::shared::{new_rng, HostInfoParams, WeightParams};

/// Benchmark the storage speed of a chain snapshot.
#[derive(Debug, Parser)]
pub struct StorageCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub database_params: DatabaseParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub pruning_params: PruningParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub params: StorageParams,
}

/// Parameters for modifying the benchmark behaviour and the post processing of the results.
#[derive(Debug, Default, Serialize, Clone, PartialEq, Args)]
pub struct StorageParams {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub weight_params: WeightParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub hostinfo: HostInfoParams,

	/// Skip the `read` benchmark.
	#[arg(long)]
	pub skip_read: bool,

	/// Skip the `write` benchmark.
	#[arg(long)]
	pub skip_write: bool,

	/// Specify the Handlebars template to use for outputting benchmark results.
	#[arg(long)]
	pub template_path: Option<PathBuf>,

	/// Add a header to the generated weight output file.
	///
	/// Good for adding LICENSE headers.
	#[arg(long, value_name = "PATH")]
	pub header: Option<PathBuf>,

	/// Path to write the raw 'read' results in JSON format to. Can be a file or directory.
	#[arg(long)]
	pub json_read_path: Option<PathBuf>,

	/// Path to write the raw 'write' results in JSON format to. Can be a file or directory.
	#[arg(long)]
	pub json_write_path: Option<PathBuf>,

	/// Rounds of warmups before measuring.
	#[arg(long, default_value_t = 1)]
	pub warmups: u32,

	/// The `StateVersion` to use. Substrate `--dev` should use `V1` and Polkadot `V0`.
	/// Selecting the wrong version can corrupt the DB.
	#[arg(long, value_parser = clap::value_parser!(u8).range(0..=1))]
	pub state_version: u8,

	/// Trie cache size in bytes.
	///
	/// Providing `0` will disable the cache.
	#[arg(long, value_name = "Bytes", default_value_t = 67108864)]
	pub trie_cache_size: usize,

	/// Enable the Trie cache.
	///
	/// This should only be used for performance analysis and not for final results.
	#[arg(long)]
	pub enable_trie_cache: bool,

	/// Include child trees in benchmark.
	#[arg(long)]
	pub include_child_trees: bool,

	/// Disable PoV recorder.
	///
	/// The recorder has impact on performance when benchmarking with the TrieCache enabled.
	/// If the chain is recording a proof while building/importing a block, the pov recorder
	/// should be activated.
	///
	/// Hence, when generating weights for a parachain this should be activated and when generating
	/// weights for a standalone chain this should be deactivated.
	#[arg(long, default_value = "false")]
	pub disable_pov_recorder: bool,

	/// The batch size for the write benchmark.
	///
	/// Since the write size needs to also include the cost of computing the storage root, which is
	/// done once at the end of the block, the batch size is used to simulate multiple writes in a
	/// block.
	#[arg(long, default_value_t = 100_000)]
	pub batch_size: usize,
}

impl StorageCmd {
	/// Calls into the Read and Write benchmarking functions.
	/// Processes the output and writes it into files and stdout.
	pub fn run<Block, BA, C>(
		&self,
		cfg: Configuration,
		client: Arc<C>,
		db: (Arc<dyn Database<DbHash>>, ColumnId),
		storage: Arc<dyn Storage<HashingFor<Block>>>,
		shared_trie_cache: Option<sp_trie::cache::SharedTrieCache<HashingFor<Block>>>,
	) -> Result<()>
	where
		BA: ClientBackend<Block>,
		Block: BlockT<Hash = DbHash>,
		C: UsageProvider<Block>
			+ StorageProvider<Block, BA>
			+ HeaderBackend<Block>
			+ CallApiAt<Block>,
	{
		let mut template = TemplateData::new(&cfg, &self.params)?;

		let block_id = BlockId::<Block>::Number(client.usage_info().chain.best_number);
		template.set_block_number(block_id.to_string());

		if !self.params.skip_read {
			self.bench_warmup(&client)?;
			let record = self.bench_read(client.clone(), shared_trie_cache.clone())?;
			if let Some(path) = &self.params.json_read_path {
				record.save_json(&cfg, path, "read")?;
			}
			let stats = record.calculate_stats()?;
			info!("Time summary [ns]:\n{:?}\nValue size summary:\n{:?}", stats.0, stats.1);
			template.set_stats(Some(stats), None)?;
		}

		if !self.params.skip_write {
			self.bench_warmup(&client)?;
			let record = self.bench_write(client, db, storage, shared_trie_cache)?;
			if let Some(path) = &self.params.json_write_path {
				record.save_json(&cfg, path, "write")?;
			}
			let stats = record.calculate_stats()?;
			info!("Time summary [ns]:\n{:?}\nValue size summary:\n{:?}", stats.0, stats.1);
			template.set_stats(None, Some(stats))?;
		}

		template.write(&self.params.weight_params.weight_path, &self.params.template_path)
	}

	/// Returns the specified state version.
	pub(crate) fn state_version(&self) -> StateVersion {
		match self.params.state_version {
			0 => StateVersion::V0,
			1 => StateVersion::V1,
			_ => unreachable!("Clap set to only allow 0 and 1"),
		}
	}

	/// Returns Some if child node and None if regular
	pub(crate) fn is_child_key(&self, key: Vec<u8>) -> Option<ChildInfo> {
		if let Some((ChildType::ParentKeyId, storage_key)) =
			ChildType::from_prefixed_key(&PrefixedStorageKey::new(key))
		{
			return Some(ChildInfo::new_default(storage_key))
		}
		None
	}

	/// Run some rounds of the (read) benchmark as warmup.
	/// See `frame_benchmarking_cli::storage::read::bench_read` for detailed comments.
	fn bench_warmup<B, BA, C>(&self, client: &Arc<C>) -> Result<()>
	where
		C: UsageProvider<B> + StorageProvider<B, BA>,
		B: BlockT + Debug,
		BA: ClientBackend<B>,
	{
		let hash = client.usage_info().chain.best_hash;
		let mut keys: Vec<_> = client.storage_keys(hash, None, None)?.collect();
		let (mut rng, _) = new_rng(None);
		keys.shuffle(&mut rng);

		for i in 0..self.params.warmups {
			info!("Warmup round {}/{}", i + 1, self.params.warmups);
			let mut child_nodes = Vec::new();

			for key in keys.as_slice() {
				let _ = client
					.storage(hash, &key)
					.expect("Checked above to exist")
					.ok_or("Value unexpectedly empty");

				if let Some(info) = self
					.params
					.include_child_trees
					.then(|| self.is_child_key(key.clone().0))
					.flatten()
				{
					// child tree key
					for ck in client.child_storage_keys(hash, info.clone(), None, None)? {
						child_nodes.push((ck.clone(), info.clone()));
					}
				}
			}
			for (key, info) in child_nodes.as_slice() {
				client
					.child_storage(hash, info, key)
					.expect("Checked above to exist")
					.ok_or("Value unexpectedly empty")?;
			}
		}

		Ok(())
	}
}

// Boilerplate
impl CliConfiguration for StorageCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}

	fn database_params(&self) -> Option<&DatabaseParams> {
		Some(&self.database_params)
	}

	fn pruning_params(&self) -> Option<&PruningParams> {
		Some(&self.pruning_params)
	}

	fn trie_cache_maximum_size(&self) -> Result<Option<usize>> {
		if self.params.enable_trie_cache && self.params.trie_cache_size > 0 {
			Ok(Some(self.params.trie_cache_size))
		} else {
			Ok(None)
		}
	}
}
