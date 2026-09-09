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

use polkadot_sdk::*;

/// An overarching CLI command definition.
#[derive(Debug, clap::Parser)]
pub struct Cli {
	/// Possible subcommand with parameters.
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub run: sc_cli::RunCmd,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub mixnet_params: sc_cli::MixnetParams,

	/// Disable automatic hardware benchmarks.
	///
	/// By default these benchmarks are automatically ran at startup and measure
	/// the CPU speed, the memory bandwidth and the disk speed.
	///
	/// The results are then printed out in the logs, and also sent as part of
	/// telemetry, if telemetry is enabled.
	#[arg(long)]
	pub no_hardware_benchmarks: bool,

	/// Number of concurrent workers for statement validation from the network.
	///
	/// Only relevant when `--enable-statement-store` is used.
	#[arg(long, default_value_t = sc_statement_store::DEFAULT_NETWORK_WORKERS)]
	pub statement_network_workers: usize,

	/// Maximum statements per second per peer before rate limiting kicks in.
	///
	/// Uses a token bucket algorithm that allows short bursts up to this limit
	/// while enforcing the average rate over time.
	///
	/// Only relevant when `--enable-statement-store` is used.
	#[arg(long, default_value_t = sc_statement_store::DEFAULT_RATE_LIMIT)]
	pub statement_rate_limit: u32,

	/// Maximum number of statements the statement store can hold.
	///
	/// Once this limit is reached, lower-priority statements may be evicted.
	///
	/// Only relevant when `--enable-statement-store` is used.
	#[arg(long, default_value_t = sc_statement_store::DEFAULT_MAX_TOTAL_STATEMENTS)]
	pub statement_store_max_total_statements: usize,

	/// Maximum total data size (in bytes) the statement store can hold.
	///
	/// Once this limit is reached, lower-priority statements may be evicted.
	///
	/// Only relevant when `--enable-statement-store` is used.
	#[arg(long, default_value_t = sc_statement_store::DEFAULT_MAX_TOTAL_SIZE)]
	pub statement_store_max_total_size: usize,

	/// Number of seconds for which removed statements won't be allowed to be added back.
	///
	/// This prevents old statements from being re-propagated on the network.
	///
	/// Only relevant when `--enable-statement-store` is used.
	#[arg(long, default_value_t = sc_statement_store::DEFAULT_PURGE_AFTER_SEC)]
	pub statement_store_purge_after_sec: u64,

	/// Affinity topic advertised by this node. Repeatable; each value is a 32-byte hex
	/// hash.
	///
	/// Only relevant when `--enable-statement-store` is used.
	///
	/// Hidden: takes effect only on the experimental v2 DHT statement path.
	#[arg(long = "statement-affinity-topic", value_name = "TOPIC", hide = true)]
	pub statement_affinity_topics: Vec<sc_statement_store::Topic>,

	/// DHT replication factor (K): number of closest peers a statement is routed to.
	///
	/// Only relevant when `--enable-statement-store` is used.
	///
	/// Hidden: takes effect only on the experimental v2 DHT statement path.
	#[arg(
		long,
		value_name = "K",
		default_value_t = sc_statement_store::DEFAULT_REPLICATION_FACTOR,
		hide = true
	)]
	pub statement_replication_factor: std::num::NonZeroUsize,

	/// Number of peers to gossip a statement to within the affinity set.
	///
	/// Only relevant when `--enable-statement-store` is used.
	///
	/// Hidden: takes effect only on the experimental v2 DHT statement path.
	#[arg(
		long,
		value_name = "N",
		default_value_t = sc_statement_store::DEFAULT_GOSSIP_TARGET,
		hide = true
	)]
	pub statement_gossip_target: std::num::NonZeroUsize,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub storage_monitor: sc_storage_monitor::StorageMonitorParams,
}

/// Possible subcommands of the main binary.
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// The custom inspect subcommand for decoding blocks and extrinsics.
	#[command(
		name = "inspect",
		about = "Decode given block or extrinsic using current native runtime."
	)]
	Inspect(node_inspect::cli::InspectCmd),

	/// Sub-commands concerned with benchmarking.
	///
	/// The pallet benchmarking moved to the `pallet` sub-command.
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Verify a signature for a message, provided on STDIN, with a given (public or secret) key.
	Verify(sc_cli::VerifyCmd),

	/// Generate a seed that provides a vanity address.
	Vanity(sc_cli::VanityCmd),

	/// Sign a message, with a given (secret) key.
	Sign(sc_cli::SignCmd),

	/// Build a chain specification.
	/// DEPRECATED: `build-spec` command will be removed after 1/04/2026. Use `export-chain-spec`
	/// command instead.
	#[deprecated(
		note = "build-spec command will be removed after 1/04/2026. Use export-chain-spec command instead"
	)]
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Export the chain specification.
	ExportChainSpec(sc_cli::ExportChainSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn statement_affinity_topics_accumulate_repeated_flags() {
		use clap::Parser;
		let topic_a = "11".repeat(32);
		let topic_b = "22".repeat(32);
		let cli = Cli::parse_from([
			"substrate-node",
			"--statement-affinity-topic",
			&format!("0x{topic_a}"),
			"--statement-affinity-topic",
			&format!("0x{topic_b}"),
		]);
		assert_eq!(
			cli.statement_affinity_topics,
			vec![sc_statement_store::Topic([0x11; 32]), sc_statement_store::Topic([0x22; 32])]
		);
	}
}
