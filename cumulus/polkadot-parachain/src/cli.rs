// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::common::NodeExtraArgs;
use clap::{Command, CommandFactory, FromArgMatches};
use sc_cli::SubstrateCli;
use std::path::PathBuf;

/// Sub-commands supported by the collator.
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management CLI utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Remove the whole chain.
	PurgeChain(cumulus_client_cli::PurgeChainCmd),

	/// Export the genesis state of the parachain.
	#[command(alias = "export-genesis-state")]
	ExportGenesisHead(cumulus_client_cli::ExportGenesisHeadCommand),

	/// Export the genesis wasm of the parachain.
	ExportGenesisWasm(cumulus_client_cli::ExportGenesisWasmCommand),

	/// Sub-commands concerned with benchmarking.
	/// The pallet benchmarking moved to the `pallet` sub-command.
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),
}

#[derive(Debug, clap::Parser)]
#[command(
	propagate_version = true,
	args_conflicts_with_subcommands = true,
	subcommand_negates_reqs = true,
	after_help = crate::examples(Self::executable_name())
)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[command(flatten)]
	pub run: cumulus_client_cli::RunCmd,

	/// EXPERIMENTAL: Use slot-based collator which can handle elastic scaling.
	///
	/// Use with care, this flag is unstable and subject to change.
	#[arg(long)]
	pub experimental_use_slot_based: bool,

	/// Disable automatic hardware benchmarks.
	///
	/// By default these benchmarks are automatically ran at startup and measure
	/// the CPU speed, the memory bandwidth and the disk speed.
	///
	/// The results are then printed out in the logs, and also sent as part of
	/// telemetry, if telemetry is enabled.
	#[arg(long)]
	pub no_hardware_benchmarks: bool,

	/// Export all `PoVs` build by this collator to the given folder.
	///
	/// This is useful for debugging issues that are occurring while validating these `PoVs` on the
	/// relay chain.
	#[arg(long)]
	pub export_pov_to_path: Option<PathBuf>,

	/// Relay chain arguments
	#[arg(raw = true)]
	pub relay_chain_args: Vec<String>,
}

impl Cli {
	pub(crate) fn node_extra_args(&self) -> NodeExtraArgs {
		NodeExtraArgs {
			use_slot_based_consensus: self.experimental_use_slot_based,
			export_pov: self.export_pov_to_path.clone(),
		}
	}
}

#[derive(Debug)]
pub struct RelayChainCli {
	/// The actual relay chain cli object.
	pub base: polkadot_cli::RunCmd,

	/// Optional chain id that should be passed to the relay chain.
	pub chain_id: Option<String>,

	/// The base path that should be used by the relay chain.
	pub base_path: Option<PathBuf>,
}

impl RelayChainCli {
	fn polkadot_cmd() -> Command {
		let help_template = color_print::cformat!(
			"The arguments that are passed to the relay chain node. \n\
			\n\
			<bold><underline>RELAY_CHAIN_ARGS:</></> \n\
			{{options}}",
		);

		polkadot_cli::RunCmd::command()
			.no_binary_name(true)
			.help_template(help_template)
	}

	/// Parse the relay chain CLI parameters using the parachain `Configuration`.
	pub fn new<'a>(
		para_config: &sc_service::Configuration,
		relay_chain_args: impl Iterator<Item = &'a String>,
	) -> Self {
		let polkadot_cmd = Self::polkadot_cmd();
		let matches = polkadot_cmd.get_matches_from(relay_chain_args);
		let base = FromArgMatches::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

		let extension = crate::chain_spec::Extensions::try_get(&*para_config.chain_spec);
		let chain_id = extension.map(|e| e.relay_chain.clone());

		let base_path = para_config.base_path.path().join("polkadot");
		Self { base, chain_id, base_path: Some(base_path) }
	}
}
