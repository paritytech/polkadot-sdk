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

use crate::common::{
	chain_spec::{Extensions, GenericChainSpec, LoadSpec},
	NodeExtraArgs,
};
use clap::{Command, CommandFactory, FromArgMatches};
use sc_chain_spec::ChainSpec;
use sc_cli::SubstrateCli;
use std::{fmt::Debug, path::PathBuf};

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

fn examples(executable_name: String) -> String {
	color_print::cformat!(
		r#"<bold><underline>Examples:</></>

   <bold>{0} --chain para.json --sync warp -- --chain relay.json --sync warp</>
        Launch a warp-syncing full node of a given para's chain-spec, and a given relay's chain-spec.

	<green><italic>The above approach is the most flexible, and the most forward-compatible way to spawn an omni-node.</></>

	You can find the chain-spec of some networks in:
	https://paritytech.github.io/chainspecs

   <bold>{0} --chain asset-hub-polkadot --sync warp -- --chain polkadot --sync warp</>
        Launch a warp-syncing full node of the <italic>Asset Hub</> parachain on the <italic>Polkadot</> Relay Chain.

   <bold>{0} --chain asset-hub-kusama --sync warp --relay-chain-rpc-url ws://rpc.example.com -- --chain kusama</>
        Launch a warp-syncing full node of the <italic>Asset Hub</> parachain on the <italic>Kusama</> Relay Chain.
        Uses <italic>ws://rpc.example.com</> as remote relay chain node.
 "#,
		executable_name,
	)
}

#[derive(Debug, clap::Parser)]
#[command(
	propagate_version = true,
	args_conflicts_with_subcommands = true,
	subcommand_negates_reqs = true,
	after_help = examples(Self::executable_name())
)]
pub struct Cli {
	#[arg(skip)]
	pub(crate) chain_spec_loader: Option<Box<dyn LoadSpec>>,

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

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		Self::executable_name()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"The command-line arguments provided first will be passed to the parachain node, \n\
			and the arguments provided after -- will be passed to the relay chain node. \n\
			\n\
			Example: \n\
			\n\
			{} [parachain-args] -- [relay-chain-args]",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/polkadot-sdk/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
		if let Some(chain_spec_loader) = &self.chain_spec_loader {
			return chain_spec_loader.load_spec(id);
		}

		Ok(Box::new(GenericChainSpec::from_json_file(id.into())?))
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

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String {
		Cli::impl_name()
	}

	fn impl_version() -> String {
		Cli::impl_version()
	}

	fn description() -> String {
		Cli::description()
	}

	fn author() -> String {
		Cli::author()
	}

	fn support_url() -> String {
		Cli::support_url()
	}

	fn copyright_start_year() -> i32 {
		Cli::copyright_start_year()
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name()].iter()).load_spec(id)
	}
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

		let extension = Extensions::try_get(&*para_config.chain_spec);
		let chain_id = extension.map(|e| e.relay_chain.clone());

		let base_path = para_config.base_path.path().join("polkadot");
		Self { base, chain_id, base_path: Some(base_path) }
	}
}
