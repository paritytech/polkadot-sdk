// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Local-only soak of statement gossip between collators of two binary versions. Two collators
//! run the default `polkadot-parachain`, two run the binary named by `OLD_PARACHAIN_COMMAND`.
//! Statements flow at a steady rate to every collator in one-minute rounds, each round must reach
//! every live collator, and every tenth round one collator restarts and has to catch up on the
//! round it missed. `MIXED_VERSION_SOAK_SECS` sets the duration, one hour by default.
//!
//! Collator names are well-known accounts because their Aura keys are the genesis authority set
//! of the People chain spec.

use super::common::{
	assert_statements_match, base_dir, collator_args, create_chain_spec_with_allowances,
	launch_network_with_commands, submit_statement, subscribe_topic, wait_for_first_block,
};
use codec::Encode;
use log::info;
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::sr25519;
use sp_statement_store::{SubmitResult, Topic};
use std::time::{Duration, Instant};
use zombienet_sdk::NetworkNode;

const OLD_COMMAND_ENV: &str = "OLD_PARACHAIN_COMMAND";
const SOAK_SECS_ENV: &str = "MIXED_VERSION_SOAK_SECS";
const DEFAULT_SOAK_SECS: u64 = 3600;
const NEW_COLLATORS: [&str; 2] = ["alice", "bob"];
const OLD_COLLATORS: [&str; 2] = ["charlie", "dave"];
const LOG_FILTER: &str = "info,statement-store=info,statement-gossip=debug";

const PARTICIPANTS: u32 = 2_000;
const STATEMENTS_PER_SECOND: usize = 50;
const SUBMIT_TICK: Duration = Duration::from_millis(100);
const ROUND_SECS: u64 = 60;
const PAYLOAD_SIZE: usize = 128;
const RESTART_EVERY_ROUNDS: usize = 10;
const RESTART_DOWNTIME: Duration = Duration::from_secs(30);
const VERIFY_TIMEOUT_SECS: u64 = 60;
const CATCH_UP_TIMEOUT_SECS: u64 = 180;

fn round_topic(round: usize) -> Topic {
	let mut topic = [0u8; 32];
	topic[..8].copy_from_slice(&(round as u64).to_le_bytes());
	topic.into()
}

struct Load {
	keypairs: Vec<sr25519::Pair>,
	seq: u32,
}

impl Load {
	fn new() -> Self {
		let keypairs = (0..PARTICIPANTS).map(get_keypair).collect();
		Self { keypairs, seq: 0 }
	}

	fn next_statement(&mut self, round: usize, topic: Topic) -> sp_statement_store::Statement {
		let keypair = &self.keypairs[self.seq as usize % self.keypairs.len()];
		self.seq += 1;
		let mut payload = vec![0u8; PAYLOAD_SIZE];
		payload[..8].copy_from_slice(&(round as u64).to_le_bytes());
		payload[8..12].copy_from_slice(&self.seq.to_le_bytes());
		create_test_statement(keypair, &[topic], None, payload, u32::MAX, self.seq)
	}
}

struct RoundReport {
	expected: Vec<Vec<u8>>,
	submit_time: Duration,
	verify_time: Duration,
}

/// Streams one round of statements round-robin to `active`, then expects the whole round on each.
async fn run_round(
	round: usize,
	active: &[&NetworkNode],
	load: &mut Load,
) -> Result<RoundReport, anyhow::Error> {
	let topic = round_topic(round);
	// The subscription lives only as long as the RPC client that opened it.
	let mut targets = Vec::with_capacity(active.len());
	for node in active {
		let rpc = node.rpc().await?;
		let subscription = subscribe_topic(&rpc, topic).await?;
		targets.push((node.name(), rpc, subscription));
	}

	let total = STATEMENTS_PER_SECOND * ROUND_SECS as usize;
	let per_tick = STATEMENTS_PER_SECOND * SUBMIT_TICK.as_millis() as usize / 1000;
	let mut expected = Vec::with_capacity(total);
	let mut ticker = tokio::time::interval(SUBMIT_TICK);
	let started = Instant::now();
	while expected.len() < total {
		ticker.tick().await;
		for _ in 0..per_tick.min(total - expected.len()) {
			let statement = load.next_statement(round, topic);
			let (name, rpc, _) = &targets[expected.len() % targets.len()];
			let result = submit_statement(rpc, &statement).await?;
			assert_eq!(result, SubmitResult::New, "round {round}: {name} rejected");
			expected.push(statement.encode());
		}
	}
	let submit_time = started.elapsed();

	let verify_started = Instant::now();
	for (name, _, subscription) in targets.iter_mut() {
		assert_statements_match(subscription, &expected, VERIFY_TIMEOUT_SECS, name).await?;
	}
	Ok(RoundReport { expected, submit_time, verify_time: verify_started.elapsed() })
}

/// Waits for a restarted collator and expects the round it sat out to arrive from its peers.
async fn assert_caught_up(
	target: &NetworkNode,
	round: usize,
	expected: &[Vec<u8>],
) -> Result<(), anyhow::Error> {
	target.wait_until_is_up(120u64).await?;
	let rpc = target.rpc().await?;
	let mut subscription = subscribe_topic(&rpc, round_topic(round)).await?;
	let started = Instant::now();
	assert_statements_match(&mut subscription, expected, CATCH_UP_TIMEOUT_SECS, target.name())
		.await?;
	info!(
		"Round {round}: {} caught up on the missed round in {:.1}s",
		target.name(),
		started.elapsed().as_secs_f64()
	);
	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_mixed_version() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	// GitHub Actions sets `CI=true`; the soak needs an hour and a second binary, so it stays local.
	if std::env::var_os("CI").is_some() {
		info!("Local-only soak, skipped in CI");
		return Ok(());
	}
	let old_command = std::env::var(OLD_COMMAND_ENV)
		.map_err(|_| anyhow::anyhow!("{OLD_COMMAND_ENV} must name the old polkadot-parachain"))?;
	let soak_secs = std::env::var(SOAK_SECS_ENV)
		.ok()
		.map(|v| v.parse::<u64>())
		.transpose()
		.map_err(|e| anyhow::anyhow!("{SOAK_SECS_ENV}: {e}"))?
		.unwrap_or(DEFAULT_SOAK_SECS);
	let rounds = (soak_secs / ROUND_SECS).max(1) as usize;

	let chain_spec_path = create_chain_spec_with_allowances(PARTICIPANTS, &base_dir()?)?;
	let args = collator_args(PARTICIPANTS, LOG_FILTER);
	let collators = [
		(NEW_COLLATORS[0], None),
		(NEW_COLLATORS[1], None),
		(OLD_COLLATORS[0], Some(old_command.as_str())),
		(OLD_COLLATORS[1], Some(old_command.as_str())),
	];
	let network = launch_network_with_commands(&collators, &chain_spec_path, args, &[]).await?;

	let alice = network.get_node(NEW_COLLATORS[0])?;
	let bob = network.get_node(NEW_COLLATORS[1])?;
	let charlie = network.get_node(OLD_COLLATORS[0])?;
	let dave = network.get_node(OLD_COLLATORS[1])?;
	let all = [alice, bob, charlie, dave];
	// Alternates an old and a new collator so both versions restart and catch up.
	let restart_cycle = [dave, bob, charlie, alice];
	wait_for_first_block(&all, 300).await?;

	info!(
		"Soak: {rounds} rounds of {ROUND_SECS}s at {STATEMENTS_PER_SECOND}/s, restart every \
		 {RESTART_EVERY_ROUNDS} rounds with {}s downtime",
		RESTART_DOWNTIME.as_secs()
	);
	let mut load = Load::new();
	let mut restarts = 0usize;
	let mut max_verify = Duration::ZERO;
	let mut total_verify = Duration::ZERO;
	let mut total_statements = 0usize;

	for round in 0..rounds {
		let restarting = (round > 0 && round % RESTART_EVERY_ROUNDS == 0)
			.then(|| restart_cycle[restarts % restart_cycle.len()]);
		let active: Vec<&NetworkNode> = all
			.iter()
			.copied()
			.filter(|node| restarting.is_none_or(|r| r.name() != node.name()))
			.collect();

		let report = match restarting {
			Some(target) => {
				info!(
					"Round {round}: restarting {} with {}s downtime",
					target.name(),
					RESTART_DOWNTIME.as_secs()
				);
				let (restart, report) = tokio::join!(
					target.restart(Some(RESTART_DOWNTIME)),
					run_round(round, &active, &mut load)
				);
				restart?;
				report?
			},
			None => run_round(round, &active, &mut load).await?,
		};
		total_statements += report.expected.len();
		total_verify += report.verify_time;
		max_verify = max_verify.max(report.verify_time);
		info!(
			"Round {round}: {} statements submitted in {:.1}s, verified on {} nodes in {:.1}s",
			report.expected.len(),
			report.submit_time.as_secs_f64(),
			active.len(),
			report.verify_time.as_secs_f64()
		);

		if let Some(target) = restarting {
			assert_caught_up(target, round, &report.expected).await?;
			restarts += 1;
		}
	}

	info!(
		"Soak done: {rounds} rounds, {total_statements} statements, {restarts} restarts, verify \
		 avg {:.1}s max {:.1}s",
		total_verify.as_secs_f64() / rounds as f64,
		max_verify.as_secs_f64()
	);
	Ok(())
}
