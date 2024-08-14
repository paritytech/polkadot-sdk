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

#[cfg(feature = "runtime-benchmarks")]
use crate::service::Block;
use crate::{
	chain_spec::{self, GenericChainSpec},
	cli::{Cli, RelayChainCli, Subcommand},
	common::NodeExtraArgs,
	fake_runtime_api::{
		asset_hub_polkadot_aura::RuntimeApi as AssetHubPolkadotRuntimeApi,
		aura::RuntimeApi as AuraRuntimeApi,
	},
	service::{new_aura_node_spec, DynNodeSpec, ShellNode},
};
#[cfg(feature = "runtime-benchmarks")]
use cumulus_client_service::storage_proof_size::HostFunctions as ReclaimHostFunctions;
use cumulus_primitives_core::ParaId;
use frame_benchmarking_cli::{BenchmarkCmd, SUBSTRATE_REFERENCE_HARDWARE};
use log::info;
use parachains_common::{AssetHubPolkadotAuraId, AuraId};
use sc_cli::{
	ChainSpec, CliConfiguration, DefaultConfigurationValues, ImportParams, KeystoreParams,
	NetworkParams, Result, SharedParams, SubstrateCli,
};
use sc_service::config::{BasePath, PrometheusConfig};
use sp_runtime::traits::AccountIdConversion;
#[cfg(feature = "runtime-benchmarks")]
use sp_runtime::traits::HashingFor;
use std::{net::SocketAddr, path::PathBuf};

/// The choice of consensus for the parachain omni-node.
#[derive(PartialEq, Eq, Debug, Default)]
pub enum Consensus {
	/// Aura consensus.
	#[default]
	Aura,
	/// Use the relay chain consensus.
	// TODO: atm this is just a demonstration, not really reach-able. We can add it to the CLI,
	// env, or the chain spec. Or, just don't, and when we properly refactor this mess we will
	// re-introduce it.
	#[allow(unused)]
	Relay,
}

/// Helper enum that is used for better distinction of different parachain/runtime configuration
/// (it is based/calculated on ChainSpec's ID attribute)
#[derive(Debug, PartialEq)]
enum Runtime {
	/// None of the system-chain runtimes, rather the node will act agnostic to the runtime ie. be
	/// an omni-node, and simply run a node with the given consensus algorithm.
	Omni(Consensus),
	Shell,
	Seedling,
	AssetHubPolkadot,
	AssetHub,
	Penpal(ParaId),
	ContractsRococo,
	Collectives,
	Glutton,
	BridgeHub(chain_spec::bridge_hubs::BridgeHubRuntimeType),
	Coretime(chain_spec::coretime::CoretimeRuntimeType),
	People(chain_spec::people::PeopleRuntimeType),
}

trait RuntimeResolver {
	fn runtime(&self) -> Result<Runtime>;
}

impl RuntimeResolver for dyn ChainSpec {
	fn runtime(&self) -> Result<Runtime> {
		Ok(runtime(self.id()))
	}
}

/// Implementation, that can resolve [`Runtime`] from any json configuration file
impl RuntimeResolver for PathBuf {
	fn runtime(&self) -> Result<Runtime> {
		#[derive(Debug, serde::Deserialize)]
		struct EmptyChainSpecWithId {
			id: String,
		}

		let file = std::fs::File::open(self)?;
		let reader = std::io::BufReader::new(file);
		let chain_spec: EmptyChainSpecWithId =
			serde_json::from_reader(reader).map_err(|e| sc_cli::Error::Application(Box::new(e)))?;

		Ok(runtime(&chain_spec.id))
	}
}

fn runtime(id: &str) -> Runtime {
	let id = id.replace('_', "-");
	let (_, id, para_id) = extract_parachain_id(&id);

	if id.starts_with("shell") {
		Runtime::Shell
	} else if id.starts_with("seedling") {
		Runtime::Seedling
	} else if id.starts_with("asset-hub-polkadot") | id.starts_with("statemint") {
		Runtime::AssetHubPolkadot
	} else if id.starts_with("asset-hub-kusama") |
		id.starts_with("statemine") |
		id.starts_with("asset-hub-rococo") |
		id.starts_with("rockmine") |
		id.starts_with("asset-hub-westend") |
		id.starts_with("westmint")
	{
		Runtime::AssetHub
	} else if id.starts_with("penpal") {
		Runtime::Penpal(para_id.unwrap_or(ParaId::new(0)))
	} else if id.starts_with("contracts-rococo") {
		Runtime::ContractsRococo
	} else if id.starts_with("collectives-polkadot") || id.starts_with("collectives-westend") {
		Runtime::Collectives
	} else if id.starts_with(chain_spec::bridge_hubs::BridgeHubRuntimeType::ID_PREFIX) {
		Runtime::BridgeHub(
			id.parse::<chain_spec::bridge_hubs::BridgeHubRuntimeType>()
				.expect("Invalid value"),
		)
	} else if id.starts_with(chain_spec::coretime::CoretimeRuntimeType::ID_PREFIX) {
		Runtime::Coretime(
			id.parse::<chain_spec::coretime::CoretimeRuntimeType>().expect("Invalid value"),
		)
	} else if id.starts_with("glutton") {
		Runtime::Glutton
	} else if id.starts_with(chain_spec::people::PeopleRuntimeType::ID_PREFIX) {
		Runtime::People(id.parse::<chain_spec::people::PeopleRuntimeType>().expect("Invalid value"))
	} else {
		log::warn!(
			"No specific runtime was recognized for ChainSpec's id: '{}', \
			so Runtime::Omni(Consensus::Aura) will be used",
			id
		);
		Runtime::Omni(Consensus::Aura)
	}
}

fn load_spec(id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
	let (id, _, para_id) = extract_parachain_id(id);
	Ok(match id {
		// - Default-like
		"staging" =>
			Box::new(chain_spec::rococo_parachain::staging_rococo_parachain_local_config()),
		"tick" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/tick.json")[..],
		)?),
		"trick" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/trick.json")[..],
		)?),
		"track" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/track.json")[..],
		)?),

		// -- Starters
		"shell" => Box::new(chain_spec::shell::get_shell_chain_spec()),
		"seedling" => Box::new(chain_spec::seedling::get_seedling_chain_spec()),

		// -- Asset Hub Polkadot
		"asset-hub-polkadot" | "statemint" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/asset-hub-polkadot.json")[..],
		)?),

		// -- Asset Hub Kusama
		"asset-hub-kusama" | "statemine" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/asset-hub-kusama.json")[..],
		)?),

		// -- Asset Hub Rococo
		"asset-hub-rococo-dev" =>
			Box::new(chain_spec::asset_hubs::asset_hub_rococo_development_config()),
		"asset-hub-rococo-local" =>
			Box::new(chain_spec::asset_hubs::asset_hub_rococo_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"asset-hub-rococo-genesis" =>
			Box::new(chain_spec::asset_hubs::asset_hub_rococo_genesis_config()),
		"asset-hub-rococo" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/asset-hub-rococo.json")[..],
		)?),

		// -- Asset Hub Westend
		"asset-hub-westend-dev" | "westmint-dev" =>
			Box::new(chain_spec::asset_hubs::asset_hub_westend_development_config()),
		"asset-hub-westend-local" | "westmint-local" =>
			Box::new(chain_spec::asset_hubs::asset_hub_westend_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"asset-hub-westend-genesis" | "westmint-genesis" =>
			Box::new(chain_spec::asset_hubs::asset_hub_westend_config()),
		// the shell-based chain spec as used for syncing
		"asset-hub-westend" | "westmint" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/asset-hub-westend.json")[..],
		)?),

		// -- Polkadot Collectives
		"collectives-polkadot" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/collectives-polkadot.json")[..],
		)?),

		// -- Westend Collectives
		"collectives-westend-dev" =>
			Box::new(chain_spec::collectives::collectives_westend_development_config()),
		"collectives-westend-local" =>
			Box::new(chain_spec::collectives::collectives_westend_local_config()),
		"collectives-westend" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/collectives-westend.json")[..],
		)?),

		// -- Contracts on Rococo
		"contracts-rococo-dev" =>
			Box::new(chain_spec::contracts::contracts_rococo_development_config()),
		"contracts-rococo-local" =>
			Box::new(chain_spec::contracts::contracts_rococo_local_config()),
		"contracts-rococo-genesis" => Box::new(chain_spec::contracts::contracts_rococo_config()),
		"contracts-rococo" => Box::new(GenericChainSpec::from_json_bytes(
			&include_bytes!("../chain-specs/contracts-rococo.json")[..],
		)?),

		// -- BridgeHub
		bridge_like_id
			if bridge_like_id
				.starts_with(chain_spec::bridge_hubs::BridgeHubRuntimeType::ID_PREFIX) =>
			bridge_like_id
				.parse::<chain_spec::bridge_hubs::BridgeHubRuntimeType>()
				.expect("invalid value")
				.load_config()?,

		// -- Coretime
		coretime_like_id
			if coretime_like_id
				.starts_with(chain_spec::coretime::CoretimeRuntimeType::ID_PREFIX) =>
			coretime_like_id
				.parse::<chain_spec::coretime::CoretimeRuntimeType>()
				.expect("invalid value")
				.load_config()?,

		// -- Penpal
		"penpal-rococo" => Box::new(chain_spec::penpal::get_penpal_chain_spec(
			para_id.expect("Must specify parachain id"),
			"rococo-local",
		)),
		"penpal-westend" => Box::new(chain_spec::penpal::get_penpal_chain_spec(
			para_id.expect("Must specify parachain id"),
			"westend-local",
		)),

		// -- Glutton Westend
		"glutton-westend-dev" => Box::new(chain_spec::glutton::glutton_westend_development_config(
			para_id.expect("Must specify parachain id"),
		)),
		"glutton-westend-local" => Box::new(chain_spec::glutton::glutton_westend_local_config(
			para_id.expect("Must specify parachain id"),
		)),
		// the chain spec as used for generating the upgrade genesis values
		"glutton-westend-genesis" => Box::new(chain_spec::glutton::glutton_westend_config(
			para_id.expect("Must specify parachain id"),
		)),

		// -- People
		people_like_id
			if people_like_id.starts_with(chain_spec::people::PeopleRuntimeType::ID_PREFIX) =>
			people_like_id
				.parse::<chain_spec::people::PeopleRuntimeType>()
				.expect("invalid value")
				.load_config()?,

		// -- Fallback (generic chainspec)
		"" => {
			log::warn!("No ChainSpec.id specified, so using default one, based on rococo-parachain runtime");
			Box::new(chain_spec::rococo_parachain::rococo_parachain_local_config())
		},

		// -- Loading a specific spec from disk
		path => Box::new(GenericChainSpec::from_json_file(path.into())?),
	})
}

/// Extracts the normalized chain id and parachain id from the input chain id.
/// (H/T to Phala for the idea)
/// E.g. "penpal-kusama-2004" yields ("penpal-kusama", Some(2004))
fn extract_parachain_id(id: &str) -> (&str, &str, Option<ParaId>) {
	let para_prefixes = [
		// Penpal
		"penpal-rococo-",
		"penpal-westend-",
		"penpal-kusama-",
		"penpal-polkadot-",
		// Glutton Kusama
		"glutton-kusama-dev-",
		"glutton-kusama-local-",
		"glutton-kusama-genesis-",
		// Glutton Westend
		"glutton-westend-dev-",
		"glutton-westend-local-",
		"glutton-westend-genesis-",
	];

	for para_prefix in para_prefixes {
		if let Some(suffix) = id.strip_prefix(para_prefix) {
			let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
			return (&id[..para_prefix.len() - 1], id, Some(para_id.into()))
		}
	}

	(id, id, None)
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
		load_spec(id)
	}
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

fn new_node_spec(
	config: &sc_service::Configuration,
	extra_args: &NodeExtraArgs,
) -> std::result::Result<Box<dyn DynNodeSpec>, sc_cli::Error> {
	Ok(match config.chain_spec.runtime()? {
		Runtime::AssetHubPolkadot =>
			new_aura_node_spec::<AssetHubPolkadotRuntimeApi, AssetHubPolkadotAuraId>(extra_args),
		Runtime::AssetHub |
		Runtime::BridgeHub(_) |
		Runtime::Collectives |
		Runtime::Coretime(_) |
		Runtime::People(_) |
		Runtime::ContractsRococo |
		Runtime::Glutton |
		Runtime::Penpal(_) => new_aura_node_spec::<AuraRuntimeApi, AuraId>(extra_args),
		Runtime::Shell | Runtime::Seedling => Box::new(ShellNode),
		Runtime::Omni(consensus) => match consensus {
			Consensus::Aura => new_aura_node_spec::<AuraRuntimeApi, AuraId>(extra_args),
			Consensus::Relay => Box::new(ShellNode),
		},
	})
}

/// Parse command line arguments into service configuration.
pub fn run() -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node = new_node_spec(&config, &cli.node_extra_args())?;
				node.prepare_check_block_cmd(config, cmd)
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node = new_node_spec(&config, &cli.node_extra_args())?;
				node.prepare_export_blocks_cmd(config, cmd)
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node = new_node_spec(&config, &cli.node_extra_args())?;
				node.prepare_export_state_cmd(config, cmd)
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node = new_node_spec(&config, &cli.node_extra_args())?;
				node.prepare_import_blocks_cmd(config, cmd)
			})
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let node = new_node_spec(&config, &cli.node_extra_args())?;
				node.prepare_revert_cmd(config, cmd)
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let polkadot_cli = RelayChainCli::new(runner.config(), cli.relay_chain_args.iter());

			runner.sync_run(|config| {
				let polkadot_config = SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|err| format!("Relay chain argument error: {}", err))?;

				cmd.run(config, polkadot_config)
			})
		},
		Some(Subcommand::ExportGenesisHead(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let node = new_node_spec(&config, &cli.node_extra_args())?;
				node.run_export_genesis_head_cmd(config, cmd)
			})
		},
		Some(Subcommand::ExportGenesisWasm(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|_config| {
				let spec = cli.load_spec(&cmd.shared_params.chain.clone().unwrap_or_default())?;
				cmd.run(&*spec)
			})
		},
		Some(Subcommand::Benchmark(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			// Switch on the concrete benchmark sub-command-
			match cmd {
				#[cfg(feature = "runtime-benchmarks")]
				BenchmarkCmd::Pallet(cmd) => runner.sync_run(|config| {
					cmd.run_with_spec::<HashingFor<Block>, ReclaimHostFunctions>(Some(
						config.chain_spec,
					))
				}),
				BenchmarkCmd::Block(cmd) => runner.sync_run(|config| {
					let node = new_node_spec(&config, &cli.node_extra_args())?;
					node.run_benchmark_block_cmd(config, cmd)
				}),
				#[cfg(feature = "runtime-benchmarks")]
				BenchmarkCmd::Storage(cmd) => runner.sync_run(|config| {
					let node = new_node_spec(&config, &cli.node_extra_args())?;
					node.run_benchmark_storage_cmd(config, cmd)
				}),
				BenchmarkCmd::Machine(cmd) =>
					runner.sync_run(|config| cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())),
				#[allow(unreachable_patterns)]
				_ => Err("Benchmarking sub-command unsupported or compilation feature missing. \
					Make sure to compile with --features=runtime-benchmarks \
					to enable all supported benchmarks."
					.into()),
			}
		},
		Some(Subcommand::Key(cmd)) => Ok(cmd.run(&cli)?),
		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;
			let polkadot_cli = RelayChainCli::new(runner.config(), cli.relay_chain_args.iter());
			let collator_options = cli.run.collator_options();

			runner.run_node_until_exit(|config| async move {
				// If Statemint (Statemine, Westmint, Rockmine) DB exists and we're using the
				// asset-hub chain spec, then rename the base path to the new chain ID. In the case
				// that both file paths exist, the node will exit, as the user must decide (by
				// deleting one path) the information that they want to use as their DB.
				let old_name = match config.chain_spec.id() {
					"asset-hub-polkadot" => Some("statemint"),
					"asset-hub-kusama" => Some("statemine"),
					"asset-hub-westend" => Some("westmint"),
					"asset-hub-rococo" => Some("rockmine"),
					_ => None,
				};

				if let Some(old_name) = old_name {
					let new_path = config.base_path.config_dir(config.chain_spec.id());
					let old_path = config.base_path.config_dir(old_name);

					if old_path.exists() && new_path.exists() {
						return Err(format!(
							"Found legacy {} path {} and new Asset Hub path {}. \
							Delete one path such that only one exists.",
							old_name,
							old_path.display(),
							new_path.display()
						)
						.into())
					}

					if old_path.exists() {
						std::fs::rename(old_path.clone(), new_path.clone())?;
						info!(
							"{} was renamed to Asset Hub. The filepath with associated data on disk \
							has been renamed from {} to {}.",
							old_name,
							old_path.display(),
							new_path.display()
						);
					}
				}

				let hwbench = (!cli.no_hardware_benchmarks)
					.then_some(config.database.path().map(|database_path| {
						let _ = std::fs::create_dir_all(database_path);
						sc_sysinfo::gather_hwbench(Some(database_path))
					}))
					.flatten();

				let para_id = chain_spec::Extensions::try_get(&*config.chain_spec)
					.map(|e| e.para_id)
					.ok_or("Could not find parachain extension in chain-spec.")?;

				let id = ParaId::from(para_id);

				let parachain_account =
					AccountIdConversion::<polkadot_primitives::AccountId>::into_account_truncating(
						&id,
					);

				let tokio_handle = config.tokio_handle.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, tokio_handle)
						.map_err(|err| format!("Relay chain argument error: {}", err))?;

				info!("🪪 Parachain id: {:?}", id);
				info!("🧾 Parachain Account: {}", parachain_account);
				info!("✍️ Is collating: {}", if config.role.is_authority() { "yes" } else { "no" });

				start_node(
					config,
					polkadot_config,
					collator_options,
					id,
					cli.node_extra_args(),
					hwbench,
				)
				.await
			})
		},
	}
}

#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_node(
	config: sc_service::Configuration,
	polkadot_config: sc_service::Configuration,
	collator_options: cumulus_client_cli::CollatorOptions,
	id: ParaId,
	extra_args: NodeExtraArgs,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> Result<sc_service::TaskManager> {
	let node_spec = new_node_spec(&config, &extra_args)?;
	node_spec
		.start_node(config, polkadot_config, collator_options, id, hwbench, extra_args)
		.await
		.map_err(Into::into)
}

impl DefaultConfigurationValues for RelayChainCli {
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

impl CliConfiguration<Self> for RelayChainCli {
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

	fn base_path(&self) -> Result<Option<BasePath>> {
		Ok(self
			.shared_params()
			.base_path()?
			.or_else(|| self.base_path.clone().map(Into::into)))
	}

	fn rpc_addr(&self, default_listen_port: u16) -> Result<Option<SocketAddr>> {
		self.base.base.rpc_addr(default_listen_port)
	}

	fn prometheus_config(
		&self,
		default_listen_port: u16,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<PrometheusConfig>> {
		self.base.base.prometheus_config(default_listen_port, chain_spec)
	}

	fn init<F>(
		&self,
		_support_url: &String,
		_impl_version: &String,
		_logger_hook: F,
		_config: &sc_service::Configuration,
	) -> Result<()>
	where
		F: FnOnce(&mut sc_cli::LoggerBuilder, &sc_service::Configuration),
	{
		unreachable!("PolkadotCli is never initialized; qed");
	}

	fn chain_id(&self, is_dev: bool) -> Result<String> {
		let chain_id = self.base.base.chain_id(is_dev)?;

		Ok(if chain_id.is_empty() { self.chain_id.clone().unwrap_or_default() } else { chain_id })
	}

	fn role(&self, is_dev: bool) -> Result<sc_service::Role> {
		self.base.base.role(is_dev)
	}

	fn transaction_pool(&self, is_dev: bool) -> Result<sc_service::config::TransactionPoolOptions> {
		self.base.base.transaction_pool(is_dev)
	}

	fn trie_cache_maximum_size(&self) -> Result<Option<usize>> {
		self.base.base.trie_cache_maximum_size()
	}

	fn rpc_methods(&self) -> Result<sc_service::config::RpcMethods> {
		self.base.base.rpc_methods()
	}

	fn rpc_max_connections(&self) -> Result<u32> {
		self.base.base.rpc_max_connections()
	}

	fn rpc_cors(&self, is_dev: bool) -> Result<Option<Vec<String>>> {
		self.base.base.rpc_cors(is_dev)
	}

	fn default_heap_pages(&self) -> Result<Option<u64>> {
		self.base.base.default_heap_pages()
	}

	fn force_authoring(&self) -> Result<bool> {
		self.base.base.force_authoring()
	}

	fn disable_grandpa(&self) -> Result<bool> {
		self.base.base.disable_grandpa()
	}

	fn max_runtime_instances(&self) -> Result<Option<usize>> {
		self.base.base.max_runtime_instances()
	}

	fn announce_block(&self) -> Result<bool> {
		self.base.base.announce_block()
	}

	fn telemetry_endpoints(
		&self,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<sc_telemetry::TelemetryEndpoints>> {
		self.base.base.telemetry_endpoints(chain_spec)
	}

	fn node_name(&self) -> Result<String> {
		self.base.base.node_name()
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		chain_spec::{get_account_id_from_seed, get_from_seed},
		command::{Consensus, Runtime, RuntimeResolver},
	};
	use sc_chain_spec::{ChainSpec, ChainSpecExtension, ChainSpecGroup, ChainType, Extension};
	use serde::{Deserialize, Serialize};
	use sp_core::sr25519;
	use std::path::PathBuf;
	use tempfile::TempDir;

	#[derive(
		Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension, Default,
	)]
	#[serde(deny_unknown_fields)]
	pub struct Extensions1 {
		pub attribute1: String,
		pub attribute2: u32,
	}

	#[derive(
		Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension, Default,
	)]
	#[serde(deny_unknown_fields)]
	pub struct Extensions2 {
		pub attribute_x: String,
		pub attribute_y: String,
		pub attribute_z: u32,
	}

	fn store_configuration(dir: &TempDir, spec: &dyn ChainSpec) -> PathBuf {
		let raw_output = true;
		let json = sc_service::chain_ops::build_spec(spec, raw_output)
			.expect("Failed to build json string");
		let mut cfg_file_path = dir.path().to_path_buf();
		cfg_file_path.push(spec.id());
		cfg_file_path.set_extension("json");
		std::fs::write(&cfg_file_path, json).expect("Failed to write to json file");
		cfg_file_path
	}

	pub type DummyChainSpec<E> = sc_service::GenericChainSpec<E>;

	pub fn create_default_with_extensions<E: Extension>(
		id: &str,
		extension: E,
	) -> DummyChainSpec<E> {
		DummyChainSpec::builder(
			rococo_parachain_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			extension,
		)
		.with_name("Dummy local testnet")
		.with_id(id)
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(crate::chain_spec::rococo_parachain::testnet_genesis(
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			vec![
				get_from_seed::<rococo_parachain_runtime::AuraId>("Alice"),
				get_from_seed::<rococo_parachain_runtime::AuraId>("Bob"),
			],
			vec![get_account_id_from_seed::<sr25519::Public>("Alice")],
			1000.into(),
		))
		.build()
	}

	#[test]
	fn test_resolve_runtime_for_different_configuration_files() {
		let temp_dir = tempfile::tempdir().expect("Failed to access tempdir");

		let path = store_configuration(
			&temp_dir,
			&create_default_with_extensions("shell-1", Extensions1::default()),
		);
		assert_eq!(Runtime::Shell, path.runtime().unwrap());

		let path = store_configuration(
			&temp_dir,
			&create_default_with_extensions("shell-2", Extensions2::default()),
		);
		assert_eq!(Runtime::Shell, path.runtime().unwrap());

		let path = store_configuration(
			&temp_dir,
			&create_default_with_extensions("seedling", Extensions2::default()),
		);
		assert_eq!(Runtime::Seedling, path.runtime().unwrap());

		let path = store_configuration(
			&temp_dir,
			&create_default_with_extensions("penpal-rococo-1000", Extensions2::default()),
		);
		assert_eq!(Runtime::Penpal(1000.into()), path.runtime().unwrap());

		let path = store_configuration(
			&temp_dir,
			&create_default_with_extensions("penpal-polkadot-2000", Extensions2::default()),
		);
		assert_eq!(Runtime::Penpal(2000.into()), path.runtime().unwrap());

		let path = store_configuration(
			&temp_dir,
			&crate::chain_spec::contracts::contracts_rococo_local_config(),
		);
		assert_eq!(Runtime::ContractsRococo, path.runtime().unwrap());

		let path = store_configuration(
			&temp_dir,
			&crate::chain_spec::rococo_parachain::rococo_parachain_local_config(),
		);
		assert_eq!(Runtime::Omni(Consensus::Aura), path.runtime().unwrap());
	}
}
