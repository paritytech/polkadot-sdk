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
use log::{debug, info};
use sc_statement_store::{
	subxt_client::{get_account_nonce, submit_extrinsic, CustomConfig},
	test_utils::{create_uniform_allowance_items, get_keypair},
};
use sp_core::Pair;
use sp_statement_store::{statement_allowance_key, StatementAllowance};
use std::str::FromStr;
use subxt::{
	ext::scale_value::{value, Value},
	OnlineClient,
};
use subxt_signer::{sr25519::Keypair as SubxtKeypair, SecretUri};

#[derive(Parser, Debug)]
#[command(name = "setup-allowances")]
#[command(about = "Set statement allowances for benchmark accounts", long_about = None)]
struct SetupArgs {
	/// RPC WebSocket endpoint (e.g., ws://node1:9944).
	#[arg(long, required = true)]
	rpc_endpoint: String,

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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let args = SetupArgs::parse();

	info!(
		"Setting up allowances for {} accounts (batch_size={}, max_count={}, max_size={}, max_batch_calls={})",
		args.num_clients,
		args.allowance_batch_size,
		args.allowance_max_count,
		args.allowance_max_size,
		args.max_batch_calls
	);

	let client = OnlineClient::<CustomConfig>::from_insecure_url_with_config(
		CustomConfig::default(),
		&args.rpc_endpoint,
	)
	.await?;

	let uri =
		SecretUri::from_str(&args.sudo_seed).map_err(|e| anyhow!("Invalid sudo seed URI: {e}"))?;
	let sudo_key =
		SubxtKeypair::from_uri(&uri).map_err(|e| anyhow!("Failed to derive sudo keypair: {e}"))?;

	let allowance = StatementAllowance::new(args.allowance_max_count, args.allowance_max_size);
	let raw_items = create_uniform_allowance_items(args.num_clients, allowance);

	// Group raw storage items into set_storage calls, one per allowance_batch_size chunk
	let storage_calls: Vec<Value> = raw_items
		.chunks(args.allowance_batch_size as usize)
		.map(|chunk| {
			let items: Vec<Value> = chunk
				.iter()
				.map(|(key, val)| {
					Value::unnamed_composite([
						Value::from_bytes(key.clone()),
						Value::from_bytes(val.clone()),
					])
				})
				.collect();
			value! { System(set_storage { items: items }) }
		})
		.collect();

	let num_inner_calls = storage_calls.len();
	info!(
		"Submitting {} set_storage calls for {} accounts (max_batch_calls={})",
		num_inner_calls, args.num_clients, args.max_batch_calls
	);

	let sudo_account_id =
		<SubxtKeypair as subxt::transactions::Signer<CustomConfig>>::account_id(&sudo_key);
	let mut nonce = get_account_nonce(&client, &sudo_account_id).await?;

	for (chunk_idx, chunk) in storage_calls.chunks(args.max_batch_calls).enumerate() {
		let chunk_calls: Vec<Value> = chunk.to_vec();
		let batch_call = value! { Utility(batch_all { calls: chunk_calls }) };
		let tx = subxt::tx::dynamic("Sudo", "sudo", vec![batch_call]);

		submit_extrinsic(&client, &tx, &sudo_key, nonce).await?;
		nonce += 1;

		info!(
			"Batch {}/{} finalized",
			chunk_idx + 1,
			num_inner_calls.div_ceil(args.max_batch_calls),
		);
	}

	// Verify that allowances were actually written to storage at the latest finalized block
	let at_finalized = client.at_current_block().await.context("Failed to get finalized block")?;

	for i in 0..args.num_clients {
		let pub_key = get_keypair(i).public();
		let storage_key = statement_allowance_key(pub_key.as_ref() as &[u8]);

		match at_finalized.storage().fetch_raw(storage_key.to_vec()).await {
			Ok(value) => {
				debug!("Account {i}: allowance at finalized ({} bytes)", value.len());
			},
			Err(e) => {
				return Err(anyhow!("Account {i}: allowance NOT found at finalized block: {e}"));
			},
		}
	}

	info!("Allowances set successfully for {} accounts", args.num_clients);
	Ok(())
}
