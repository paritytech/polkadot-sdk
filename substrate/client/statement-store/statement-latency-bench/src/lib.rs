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

//! Shared types and helpers used by the `statement-latency-bench` binary. Accounts are
//! the same deterministic `//StatementClient//{idx}` set that `setup-allowances`
//! provisions.

use anyhow::{anyhow, Context};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use log::debug;
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair};
use sp_crypto_hashing::blake2_256;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundStats {
	pub round: usize,
	pub send_duration_secs: f64,
	pub receive_duration_secs: f64,
	pub full_latency_secs: f64,
	pub sent_count: u32,
	pub received_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
	pub min: f64,
	pub avg: f64,
	pub max: f64,
}

pub fn parse_messages_pattern(pattern: &str) -> Result<Vec<(usize, usize)>, anyhow::Error> {
	pattern
		.split(',')
		.map(|part| {
			let part = part.trim();
			let (count_str, size_str) = part
				.split_once(':')
				.ok_or_else(|| anyhow!("Invalid pattern '{part}'. Expected 'count:size'"))?;

			let count = count_str
				.parse::<usize>()
				.with_context(|| format!("Invalid count '{count_str}' in pattern '{part}'"))?;
			let size = size_str
				.parse::<usize>()
				.with_context(|| format!("Invalid size '{size_str}' in pattern '{part}'"))?;

			Ok((count, size))
		})
		.collect()
}

pub fn messages_per_client(pattern: &[(usize, usize)]) -> usize {
	pattern.iter().map(|(count, _)| count).sum()
}

pub fn calc_stats(values: impl Iterator<Item = f64>) -> Stats {
	let values: Vec<_> = values.collect();
	let min = values.iter().copied().fold(f64::INFINITY, f64::min);
	let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
	let avg = values.iter().sum::<f64>() / values.len() as f64;
	Stats { min, avg, max }
}

pub fn is_leader(client_id: u32) -> bool {
	client_id == 0
}

pub fn generate_topic(test_run_id: u64, client_id: u32, round: usize, msg_idx: u32) -> [u8; 32] {
	let topic_str = format!("{test_run_id}-{client_id}-{round}-{msg_idx}");
	blake2_256(topic_str.as_bytes())
}

/// Generate a deterministic keypair for a given client index
pub fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementClient//{idx}"), None)
		.expect("Derivation path is always valid; qed")
}

pub async fn connect_to_endpoints(
	endpoints: &[String],
) -> Result<Vec<Arc<WsClient>>, anyhow::Error> {
	let mut clients = Vec::with_capacity(endpoints.len());

	for endpoint in endpoints {
		let client = WsClientBuilder::default()
			.max_concurrent_requests(10000)
			.build(endpoint)
			.await
			.with_context(|| format!("Failed to connect to {endpoint}"))?;
		clients.push(Arc::new(client));
		debug!("Connected to {}", endpoint);
	}

	Ok(clients)
}
