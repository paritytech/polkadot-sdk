// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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
//! The Ethereum JSON-RPC server.
use clap::Parser;
use pallet_revive_eth_rpc::{
	client::{connect, Client, SubstrateBlockNumber},
	BlockInfoProvider, BlockInfoProviderImpl, DBReceiptProvider, ReceiptProvider,
};
use sc_cli::SharedParams;
use std::sync::Arc;

// Parsed command instructions from the command line
#[derive(Parser, Debug)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	pub node_rpc_url: String,

	/// Specifies the block number to start indexing from, going backwards from the current block.
	/// If not provided, only new blocks will be indexed
	#[clap(long)]
	pub oldest_block: Option<SubstrateBlockNumber>,

	/// The database used to store Ethereum transaction hashes.
	#[clap(long, env = "DATABASE_URL")]
	pub database_url: String,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,
}

/// Initialize the logger
#[cfg(not(test))]
fn init_logger(params: &SharedParams) -> anyhow::Result<()> {
	let mut logger = sc_cli::LoggerBuilder::new(params.log_filters().join(","));
	logger
		.with_log_reloading(params.enable_log_reloading)
		.with_detailed_output(params.detailed_log_output);

	if let Some(tracing_targets) = &params.tracing_targets {
		let tracing_receiver = params.tracing_receiver.into();
		logger.with_profiling(tracing_receiver, tracing_targets);
	}

	if params.disable_log_color {
		logger.with_colors(false);
	}

	logger.init()?;
	Ok(())
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
	let CliCommand {
		node_rpc_url, database_url, shared_params: _shared_params, oldest_block, ..
	} = CliCommand::parse();

	#[cfg(not(test))]
	init_logger(&_shared_params)?;

	let (api, rpc_client, rpc) = connect(&node_rpc_url).await?;
	let block_provider: Arc<dyn BlockInfoProvider> =
		Arc::new(BlockInfoProviderImpl::new(0, api.clone(), rpc.clone()));
	let receipt_provider: Arc<dyn ReceiptProvider> =
		Arc::new(DBReceiptProvider::new(&database_url, false, block_provider.clone()).await?);

	let client = Client::new(api, rpc_client, rpc, block_provider, receipt_provider).await?;
	client.subscribe_and_cache_receipts(oldest_block).await?;

	Ok(())
}
