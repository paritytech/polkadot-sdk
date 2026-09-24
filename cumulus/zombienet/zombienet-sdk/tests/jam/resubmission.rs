// This file is part of Cumulus.

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

#![cfg(feature = "jam")]

//! A collator whose first work-package submission never reaches JAM must resend the identical
//! package and still move the head.
//!
//! The test pins the JAM ordinary node's RPC port, pre-allocates a second port for
//! [`ProxyServer`], and starts the proxy before the network so `alice`'s `--jam-rpc-urls` can name
//! it before `spawn_fn` builds the whole network in one call. The proxy swallows the first
//! `submitWorkPackage` of every distinct work-package hash and forwards every later one
//! byte-exact. If the collator gives up instead of resending, or re-signs the package so its hash
//! changes, the head never reaches the target; if it resends identically, it does. The assertions
//! read the proxy's own ledger, the collator's log and the accumulated head together, so they say
//! which of those happened.

use anyhow::anyhow;
use cumulus_jam_zombienet_tests::{
	para::{JAM_SLOT, PARACHAIN_SERVICE_ID},
	proxy::ProxyServer,
	rpc::JamRpc,
};
use std::time::Duration;
use tokio::time::Instant;
use zombienet_sdk::{LocalFileSystem, Network};

const RESEND_MARKER: &str = "Resending the identical work package; it has not appeared on chain.";
const APPEARED_MARKER: &str = "The work package appeared on chain in a JAM block.";
const HOLD_MARKER: &str =
	"Holding this parachain slot: an overdue work package is being resent instead of a new block \
	 being built.";
const BUILT_MARKER: &str = "Built and imported a parachain block.";
/// The collator line when it abandons a package. Tolerated: only logged, never asserted on.
const GIVE_UP_MARKER: &str = "Giving up on a work package.";

/// The accumulated head the run has to reach. Every head needs its package reported, and every
/// reported package had its first submission dropped, so reaching this is only possible through
/// resends.
const HEAD_TARGET: u64 = 5;
/// Loose, but a collator that never resends has to time out here rather than pass.
const HEAD_BUDGET: Duration = Duration::from_secs(10 * 60);

/// One collator behind a proxy that drops the first submission of every package; the head can
/// only reach the target by resending.
#[tokio::test(flavor = "multi_thread")]
async fn resends_make_the_head_advance() -> Result<(), anyhow::Error> {
	const TEST: &str = "resends_make_the_head_advance";
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let jam_or_port = free_port()?;
	let proxy_port = free_port()?;

	let jam = crate::jam::setup(TEST, &[crate::jam::para(0, 0, &["alice"])])?
		.with_ordinary_rpc_port(jam_or_port)
		.with_jam_rpc_url("alice", format!("ws://127.0.0.1:{proxy_port}"));
	let config = jam
		.jamchain()
		.with_parachain(|p| jam.parachain(p, 0))
		.with_global_settings(|g| g.with_base_dir(jam.base_dir()))
		.build()
		.map_err(|e| {
			anyhow!(
				"config errs: {}",
				e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
			)
		})?;

	// The proxy has to be serving before the network comes up: `alice`'s `--jam-rpc-urls` already
	// names it, and its `jam-init` retries for ~15s, so it only has to be there by then.
	let proxy_task = tokio::spawn(async move {
		ProxyServer::serve_when_ready(
			&format!("ws://127.0.0.1:{jam_or_port}"),
			&format!("127.0.0.1:{proxy_port}"),
			Duration::from_secs(90),
		)
		.await
	});

	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;
	let proxy = proxy_task.await??;

	let jam_url = crate::jam::jam_rpc_url(&network)?;
	let jam_rpc = JamRpc::wait_ready(&jam_url, Instant::now() + Duration::from_secs(120)).await?;

	let result = resend_then_advance(&network, &proxy, &jam_rpc)
		.await
		.map_err(|error| anyhow::anyhow!("{error}\n\n{}", proxy.describe()));
	if let Err(error) = network.destroy().await {
		log::warn!("tearing down the JAM network failed: {error}");
	}
	proxy.shutdown().await;
	result
}

/// Reserve a free loopback port by binding and immediately dropping the listener.
fn free_port() -> anyhow::Result<u16> {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
	Ok(listener.local_addr()?.port())
}

/// Assert the head reached the target through resends, and that every log witness agrees.
async fn resend_then_advance(
	network: &Network<LocalFileSystem>,
	proxy: &ProxyServer,
	jam_rpc: &JamRpc,
) -> anyhow::Result<()> {
	// How long after its first submission a package may still have been submitted only once. A
	// collator resends at anchor+2, one JAM_SLOT at a time, so anything older than this that was
	// never submitted twice is a package the collator abandoned instead of resending.
	let resend_grace = JAM_SLOT * 4;

	super::wait_for_jam_head(jam_rpc, PARACHAIN_SERVICE_ID, 0, HEAD_TARGET, HEAD_BUDGET).await?;

	let attempts = proxy.snapshot();
	anyhow::ensure!(
		!attempts.is_empty(),
		"the head reached {HEAD_TARGET}, but the proxy saw no submitWorkPackage at all: alice is \
		 talking to the network directly, not through the proxy"
	);

	let forwarded: Vec<String> = attempts
		.iter()
		.filter(|attempt| attempt.count >= 2)
		.map(|attempt| hex::encode(&attempt.hash[..8]))
		.collect();
	anyhow::ensure!(
		forwarded.len() as u64 >= HEAD_TARGET,
		"the head reached {HEAD_TARGET} through the proxy, but the proxy forwarded only {} \
		 distinct work package(s); every accumulated head's package must have been submitted at \
		 least twice",
		forwarded.len()
	);

	for attempt in &attempts {
		if attempt.count == 1 {
			let age = attempt.first_seen.elapsed();
			anyhow::ensure!(
				age < resend_grace,
				"the proxy saw work package 0x{}… once and never again {age:?} after its first \
				 submission; the collator must resend the identical bytes, not re-sign a new \
				 package",
				hex::encode(&attempt.hash[..8]),
			);
		}
	}

	anyhow::ensure!(
		proxy.upstream_errors() == 0,
		"the proxy's upstream rejected {} forwarded submission(s)",
		proxy.upstream_errors()
	);

	let log = network.get_node("alice")?.logs().await?;
	let lines: Vec<String> = log.lines().map(str::to_string).collect();

	let resends = lines.iter().filter(|line| line.contains(RESEND_MARKER)).count();
	anyhow::ensure!(
		resends >= forwarded.len(),
		"the proxy forwarded {} distinct work package(s), but alice's log records only {resends} \
		 resend(s)",
		forwarded.len()
	);
	for prefix in &forwarded {
		anyhow::ensure!(
			lines.iter().any(|line| line.contains(RESEND_MARKER) && line.contains(prefix)),
			"the proxy forwarded work package 0x{prefix}…, but alice's log never records resending \
			 it"
		);
	}

	let appeared = lines.iter().filter(|line| line.contains(APPEARED_MARKER)).count();
	anyhow::ensure!(
		appeared as u64 >= HEAD_TARGET,
		"alice's log records only {appeared} package(s) appearing on chain, but JAM accumulated \
		 {HEAD_TARGET} heads"
	);

	let holds = lines.iter().filter(|line| line.contains(HOLD_MARKER)).count();
	anyhow::ensure!(
		holds >= 1,
		"alice never held a parachain slot for an outstanding resend, yet the proxy dropped its \
		 first submission of every package"
	);

	for prefix in &forwarded {
		if let Some(between) = built_between_resend_and_report(&lines, prefix) {
			anyhow::ensure!(
				between <= 1,
				"work package 0x{prefix}… had {between} `{BUILT_MARKER}` line(s) between its resend \
				 and its appearance on chain; while a package is outstanding the builder authors at \
				 most one block past it"
			);
		}
	}

	let given_up = lines.iter().filter(|line| line.contains(GIVE_UP_MARKER)).count();
	if given_up > 0 {
		log::warn!(
			"alice gave up on {given_up} work package(s); tolerated (the head still advanced), see \
			 the collator log for the reasons"
		);
	}

	Ok(())
}

/// How many authored-block lines fall strictly between work package `hash`'s first resend line and
/// its first appeared-on-chain line after it.
///
/// `None` when either line is missing, which is a forwarded package that was given up before it
/// appeared; the caller tolerates those.
fn built_between_resend_and_report(lines: &[String], hash: &str) -> Option<usize> {
	let resend = lines
		.iter()
		.position(|line| line.contains(RESEND_MARKER) && line.contains(hash))?;
	let appeared = lines
		.iter()
		.enumerate()
		.skip(resend + 1)
		.find(|(_, line)| line.contains(APPEARED_MARKER) && line.contains(hash))
		.map(|(index, _)| index)?;
	Some(
		lines[resend + 1..appeared]
			.iter()
			.filter(|line| line.contains(BUILT_MARKER))
			.count(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	const HASH_A: &str = "0011223344556677";
	const HASH_B: &str = "8899aabbccddeeff";

	fn resend(hash: &str) -> String {
		format!("wp_hash=0x{hash}... {RESEND_MARKER}")
	}
	fn appeared(hash: &str) -> String {
		format!("wp_hash=0x{hash}... {APPEARED_MARKER}")
	}
	fn built() -> String {
		format!("block_number=1 {BUILT_MARKER}")
	}

	#[test]
	fn built_between_resend_and_report_counts_only_the_gap() {
		// The built line before the first resend and the one after the last appearance exist to
		// prove the helper counts neither.
		let lines = vec![
			built(),
			resend(HASH_A),
			built(),
			appeared(HASH_A),
			resend(HASH_B),
			built(),
			built(),
			appeared(HASH_B),
			built(),
		];

		assert_eq!(built_between_resend_and_report(&lines, HASH_A), Some(1));
		assert_eq!(built_between_resend_and_report(&lines, HASH_B), Some(2));
	}

	#[test]
	fn built_between_resend_and_report_is_none_without_an_appearance() {
		// A forwarded package given up before it appeared has a resend but no appeared-on-chain
		// line, and must not be mistaken for a zero-gap success.
		let lines = vec![resend(HASH_A), built()];
		assert_eq!(built_between_resend_and_report(&lines, HASH_A), None);

		// A hash that was never resent has no gap to measure at all.
		assert_eq!(built_between_resend_and_report(&[], HASH_A), None);
	}
}
