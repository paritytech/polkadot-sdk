// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! One-shot binary that sets on-chain statement allowances for deterministic benchmark accounts
//! via `Sudo(batch_all(set_storage(...)))`. Run once before repeatedly invoking
//! `statement-latency-bench`.

use anyhow::{anyhow, Context};
use clap::Parser;
use codec::Encode;
use sp_core::Pair;
use statement_latency_bench::{connect_to_endpoints, get_keypair, CustomConfig};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClient};
use log::info;
use sp_statement_store::{statement_allowance_key, StatementAllowance};
use std::str::FromStr;
use subxt::{
	config::DefaultExtrinsicParamsBuilder,
	ext::scale_value::{value, Value},
	OnlineClient,
};
use subxt_signer::{sr25519::Keypair as SubxtKeypair, SecretUri};

#[derive(Parser, Debug)]
#[command(name = "setup-allowances")]
#[command(about = "Set statement allowances for benchmark accounts", long_about = None)]
struct SetupArgs {
	/// Comma-separated list of RPC WebSocket endpoints (e.g., ws://node1:9944,ws://node2:9944).
	#[arg(long, value_delimiter = ',', required = true)]
	rpc_endpoints: Vec<String>,

	/// Sudo seed/SURI for setting statement allowances (e.g., "//Alice" or mnemonic phrase).
	#[arg(long, required = true)]
	sudo_seed: String,

	/// Number of deterministic benchmark accounts to provision.
	#[arg(long, default_value = "100")]
	num_clients: u32,

	/// Number of accounts per allowance-setting transaction.
	#[arg(long, default_value = "100")]
	allowance_batch_size: u32,

	/// Maximum number of statements allowed per account.
	#[arg(long, default_value_t = 100_000)]
	allowance_max_count: u32,

	/// Maximum total size of statements in bytes per account.
	#[arg(long, default_value_t = 1_000_000)]
	allowance_max_size: u32,

	/// Maximum number of calls in a single batch_all transaction.
	#[arg(long, default_value_t = 100)]
	max_batch_calls: usize,
}

/// Set statement allowances for all deterministic benchmark accounts.
///
/// Storage items are grouped into inner `set_storage` calls of `batch_size` each to keep
/// individual call payloads small. The inner calls are then chunked into groups of at most
/// `max_batch_calls` and each group is submitted as a separate `Sudo(batch_all(...))` transaction
/// to stay within the runtime's batched calls limit.
#[allow(clippy::too_many_arguments)]
async fn set_allowances(
	rpc_url: &str,
	rpc_client: &WsClient,
	sudo_seed: &str,
	num_clients: u32,
	batch_size: u32,
	max_count: u32,
	max_size: u32,
	max_batch_calls: usize,
) -> Result<(), anyhow::Error> {
	let client = OnlineClient::<CustomConfig>::from_insecure_url(rpc_url).await?;

	let uri = SecretUri::from_str(sudo_seed).map_err(|e| anyhow!("Invalid sudo seed URI: {e}"))?;
	let sudo_key =
		SubxtKeypair::from_uri(&uri).map_err(|e| anyhow!("Failed to derive sudo keypair: {e}"))?;

	let allowance_value = StatementAllowance::new(max_count, max_size).encode();

	let storage_calls: Vec<Value> = (0..num_clients)
		.step_by(batch_size as usize)
		.map(|chunk_start| {
			let chunk_end = std::cmp::min(chunk_start + batch_size, num_clients);

			let items: Vec<Value> = (chunk_start..chunk_end)
				.map(|i| {
					let pub_key = get_keypair(i).public();
					let storage_key = statement_allowance_key(pub_key.as_ref() as &[u8]);

					let hex_key: String = storage_key.iter().map(|b| format!("{b:02x}")).collect();
					info!("Account {i}: pubkey={pub_key} storage_key=0x{hex_key}");

					Value::unnamed_composite([
						Value::from_bytes(storage_key),
						Value::from_bytes(allowance_value.clone()),
					])
				})
				.collect();

			value! { System(set_storage { items: items }) }
		})
		.collect();

	let num_inner_calls = storage_calls.len();
	info!(
		"Submitting {} set_storage calls for {} accounts (max_batch_calls={})",
		num_inner_calls, num_clients, max_batch_calls
	);

	use subxt::tx::TxStatus;
	for (chunk_idx, chunk) in storage_calls.chunks(max_batch_calls).enumerate() {
		let chunk_calls: Vec<Value> = chunk.to_vec();
		let batch_call = value! { Utility(batch_all { calls: chunk_calls }) };
		let tx = subxt::tx::dynamic("Sudo", "sudo", vec![batch_call]);
		let dp = DefaultExtrinsicParamsBuilder::<CustomConfig>::new().immortal().build();
		let extensions =
			(dp.0, dp.1, dp.2, dp.3, dp.4, dp.5, dp.6, dp.7, dp.8, (), (), (), (), (), (), ());

		let mut progress = client
			.tx()
			.create_signed(&tx, &sudo_key, extensions)
			.await?
			.submit_and_watch()
			.await?;

		while let Some(status) = progress.next().await.transpose()? {
			match status {
				TxStatus::InFinalizedBlock(tx_in_block) => {
					tx_in_block.wait_for_success().await?;
					info!(
						"Batch {}/{} finalized in block {:#?}",
						chunk_idx + 1,
						num_inner_calls.div_ceil(max_batch_calls),
						tx_in_block.block_hash()
					);
					break;
				},
				TxStatus::Error { message } |
				TxStatus::Invalid { message } |
				TxStatus::Dropped { message } => {
					return Err(anyhow!("Allowance tx batch {} failed: {message}", chunk_idx + 1));
				},
				_ => continue,
			}
		}
	}

	// Verify that allowances were actually written to storage.
	let finalized_hash: String = rpc_client
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.context("Failed to get finalized head")?;

	for i in 0..num_clients {
		let pub_key = get_keypair(i).public();
		let storage_key = statement_allowance_key(pub_key.as_ref() as &[u8]);
		let hex_key: String = storage_key.iter().map(|b| format!("{b:02x}")).collect();

		// Check at best block.
		let result_best: Option<String> = rpc_client
			.request("state_getStorage", rpc_params![format!("0x{hex_key}")])
			.await
			.with_context(|| format!("Failed to verify allowance for account {i} at best"))?;

		// Check at finalized block.
		let result_finalized: Option<String> = rpc_client
			.request("state_getStorage", rpc_params![format!("0x{hex_key}"), &finalized_hash])
			.await
			.with_context(|| format!("Failed to verify allowance for account {i} at finalized"))?;

		info!(
			"Account {i}: allowance at best={:?}, at finalized={:?}",
			result_best, result_finalized
		);

		if result_finalized.is_none() {
			return Err(anyhow!(
				"Account {i}: allowance NOT found at finalized block {finalized_hash}"
			));
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let args = SetupArgs::parse();

	if args.rpc_endpoints.is_empty() {
		return Err(anyhow!(
			"At least one RPC endpoint must be provided. Example: --rpc-endpoints ws://localhost:9944"
		));
	}

	info!(
		"Setting up allowances for {} accounts (batch_size={}, max_count={}, max_size={}, max_batch_calls={})",
		args.num_clients, args.allowance_batch_size, args.allowance_max_count,
		args.allowance_max_size, args.max_batch_calls
	);

	let rpc_clients = connect_to_endpoints(&args.rpc_endpoints).await?;

	set_allowances(
		&args.rpc_endpoints[0],
		&rpc_clients[0],
		&args.sudo_seed,
		args.num_clients,
		args.allowance_batch_size,
		args.allowance_max_count,
		args.allowance_max_size,
		args.max_batch_calls,
	)
	.await?;

	info!("Allowances set successfully for {} accounts", args.num_clients);
	Ok(())
}
