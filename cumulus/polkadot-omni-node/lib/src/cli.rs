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

//! CLI options of the omni-node. See [`Command`].

use crate::{
	chain_spec::DiskChainSpecLoader,
	common::{
		chain_spec::{Extensions, LoadSpec},
		NodeExtraArgs,
	},
};
use clap::{Command, CommandFactory, FromArgMatches};
use sc_chain_spec::ChainSpec;
use sc_cli::{
	CliConfiguration, DefaultConfigurationValues, ImportParams, KeystoreParams, NetworkParams,
	RpcEndpoint, SharedParams, SubstrateCli,
};
use sc_service::{config::PrometheusConfig, BasePath};
use std::{fmt::Debug, marker::PhantomData, path::PathBuf};

/// Trait that can be used to customize some of the customer-facing info related to the node binary
/// that is being built using this library.
///
/// The related info is shown to the customer as part of logs or help messages.
/// It does not impact functionality.
pub trait CliConfig {
	/// The version of the resulting node binary.
	fn impl_version() -> String;

	/// The description of the resulting node binary.
	fn description(executable_name: String) -> String {
		format!(
			"The command-line arguments provided first will be passed to the parachain node, \n\
			and the arguments provided after -- will be passed to the relay chain node. \n\
			\n\
			Example: \n\
			\n\
			{} [parachain-args] -- [relay-chain-args]",
			executable_name
		)
	}

	/// The author of the resulting node binary.
	fn author() -> String;

	/// The support URL for the resulting node binary.
	fn support_url() -> String;

	/// The starting copyright year of the resulting node binary.
	fn copyright_start_year() -> u16;
}

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

/// CLI Options shipped with `polkadot-omni-node`.
#[derive(clap::Parser)]
#[command(
	propagate_version = true,
	args_conflicts_with_subcommands = true,
	subcommand_negates_reqs = true
)]
pub struct Cli<Config: CliConfig> {
	#[arg(skip)]
	pub(crate) chain_spec_loader: Option<Box<dyn LoadSpec>>,

	/// Possible subcommands. See [`Subcommand`].
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	/// The shared parameters with all cumulus-based parachain nodes.
	#[command(flatten)]
	pub run: cumulus_client_cli::RunCmd,

	/// Start a dev node that produces a block each `dev_block_time` ms.
	///
	/// This is a dev option, and it won't result in starting or connecting to a parachain network.
	/// The resulting node will work on its own, running the wasm blob and artificially producing
	/// a block each `dev_block_time` ms, as if it was part of a parachain.
	#[arg(long)]
	pub dev_block_time: Option<u64>,

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

	#[arg(skip)]
	pub(crate) _phantom: PhantomData<Config>,
}

impl<Config: CliConfig> Cli<Config> {
	pub(crate) fn node_extra_args(&self) -> NodeExtraArgs {
		NodeExtraArgs {
			use_slot_based_consensus: self.experimental_use_slot_based,
			export_pov: self.export_pov_to_path.clone(),
		}
	}
}

impl<Config: CliConfig> SubstrateCli for Cli<Config> {
	fn impl_name() -> String {
		Self::executable_name()
	}

	fn impl_version() -> String {
		Config::impl_version()
	}

	fn description() -> String {
		Config::description(Self::executable_name())
	}

	fn author() -> String {
		Config::author()
	}

	fn support_url() -> String {
		Config::support_url()
	}

	fn copyright_start_year() -> i32 {
		Config::copyright_start_year() as i32
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn ChainSpec>, String> {
		match &self.chain_spec_loader {
			Some(chain_spec_loader) => chain_spec_loader.load_spec(id),
			None => DiskChainSpecLoader.load_spec(id),
		}
	}
}

/// The relay chain CLI flags. These are passed in after a `--` at the end.
#[derive(Debug)]
pub struct RelayChainCli<Config: CliConfig> {
	/// The actual relay chain cli object.
	pub base: polkadot_cli::RunCmd,

	/// Optional chain id that should be passed to the relay chain.
	pub chain_id: Option<String>,

	/// The base path that should be used by the relay chain.
	pub base_path: Option<PathBuf>,

	_phantom: PhantomData<Config>,
}

impl<Config: CliConfig> RelayChainCli<Config> {
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
		Self { base, chain_id, base_path: Some(base_path), _phantom: Default::default() }
	}
}

impl<Config: CliConfig> SubstrateCli for RelayChainCli<Config> {
	fn impl_name() -> String {
		Cli::<Config>::impl_name()
	}

	fn impl_version() -> String {
		Cli::<Config>::impl_version()
	}

	fn description() -> String {
		Cli::<Config>::description()
	}

	fn author() -> String {
		Cli::<Config>::author()
	}

	fn support_url() -> String {
		Cli::<Config>::support_url()
	}

	fn copyright_start_year() -> i32 {
		Cli::<Config>::copyright_start_year()
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([Self::executable_name()].iter()).load_spec(id)
	}
}

impl<Config: CliConfig> DefaultConfigurationValues for RelayChainCli<Config> {
	fn p2p_listen_port() -> u16 {
		30334
	}

	fn rpc_listen_port() -> u16 {
		9945
	}

	fn prometheus_listen_port() -> u16 {
		9616
	}
}

impl<Config: CliConfig> CliConfiguration<Self> for RelayChainCli<Config> {
	fn shared_params(&self) -> &SharedParams {
		self.base.base.shared_params()
	}

	fn import_params(&self) -> Option<&ImportParams> {
		self.base.base.import_params()
	}

	fn network_params(&self) -> Option<&NetworkParams> {
		self.base.base.network_params()
	}

	fn keystore_params(&self) -> Option<&KeystoreParams> {
		self.base.base.keystore_params()
	}

	fn base_path(&self) -> sc_cli::Result<Option<BasePath>> {
		Ok(self
			.shared_params()
			.base_path()?
			.or_else(|| self.base_path.clone().map(Into::into)))
	}

	fn rpc_addr(&self, default_listen_port: u16) -> sc_cli::Result<Option<Vec<RpcEndpoint>>> {
		self.base.base.rpc_addr(default_listen_port)
	}

	fn prometheus_config(
		&self,
		default_listen_port: u16,
		chain_spec: &Box<dyn ChainSpec>,
	) -> sc_cli::Result<Option<PrometheusConfig>> {
		self.base.base.prometheus_config(default_listen_port, chain_spec)
	}

	fn init<F>(
		&self,
		_support_url: &String,
		_impl_version: &String,
		_logger_hook: F,
	) -> sc_cli::Result<()>
	where
		F: FnOnce(&mut sc_cli::LoggerBuilder),
	{
		unreachable!("PolkadotCli is never initialized; qed");
	}

	fn chain_id(&self, is_dev: bool) -> sc_cli::Result<String> {
		let chain_id = self.base.base.chain_id(is_dev)?;

		Ok(if chain_id.is_empty() { self.chain_id.clone().unwrap_or_default() } else { chain_id })
	}

	fn role(&self, is_dev: bool) -> sc_cli::Result<sc_service::Role> {
		self.base.base.role(is_dev)
	}

	fn transaction_pool(
		&self,
		is_dev: bool,
	) -> sc_cli::Result<sc_service::config::TransactionPoolOptions> {
		self.base.base.transaction_pool(is_dev)
	}

	fn trie_cache_maximum_size(&self) -> sc_cli::Result<Option<usize>> {
		self.base.base.trie_cache_maximum_size()
	}

	fn rpc_methods(&self) -> sc_cli::Result<sc_service::config::RpcMethods> {
		self.base.base.rpc_methods()
	}

	fn rpc_max_connections(&self) -> sc_cli::Result<u32> {
		self.base.base.rpc_max_connections()
	}

	fn rpc_cors(&self, is_dev: bool) -> sc_cli::Result<Option<Vec<String>>> {
		self.base.base.rpc_cors(is_dev)
	}

	fn default_heap_pages(&self) -> sc_cli::Result<Option<u64>> {
		self.base.base.default_heap_pages()
	}

	fn force_authoring(&self) -> sc_cli::Result<bool> {
		self.base.base.force_authoring()
	}

	fn disable_grandpa(&self) -> sc_cli::Result<bool> {
		self.base.base.disable_grandpa()
	}

	fn max_runtime_instances(&self) -> sc_cli::Result<Option<usize>> {
		self.base.base.max_runtime_instances()
	}

	fn announce_block(&self) -> sc_cli::Result<bool> {
		self.base.base.announce_block()
	}

	fn telemetry_endpoints(
		&self,
		chain_spec: &Box<dyn ChainSpec>,
	) -> sc_cli::Result<Option<sc_telemetry::TelemetryEndpoints>> {
		self.base.base.telemetry_endpoints(chain_spec)
	}

	fn node_name(&self) -> sc_cli::Result<String> {
		self.base.base.node_name()
	}
}
