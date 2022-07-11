// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use std::net::SocketAddr;

use codec::Encode;
use cumulus_client_cli::generate_genesis_block;
use cumulus_primitives_core::ParaId;
use frame_benchmarking_cli::{BenchmarkCmd, SUBSTRATE_REFERENCE_HARDWARE};
use log::info;
use parachains_common::{AuraId, StatemintAuraId};
use sc_cli::{
	ChainSpec, CliConfiguration, DefaultConfigurationValues, ImportParams, KeystoreParams,
	NetworkParams, Result, RuntimeVersion, SharedParams, SubstrateCli,
};
use sc_service::{
	config::{BasePath, PrometheusConfig},
	TaskManager,
};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::traits::{AccountIdConversion, Block as BlockT};

use crate::{
	chain_spec,
	cli::{Cli, RelayChainCli, Subcommand},
	service::{
		new_partial, Block, ShellRuntimeExecutor, StatemineRuntimeExecutor,
		StatemintRuntimeExecutor, WestmintRuntimeExecutor,
	},
};

enum Runtime {
	/// This is the default runtime (based on rococo)
	Generic,
	Shell,
	Seedling,
	Statemint,
	Statemine,
	Westmint,
	ContractsRococo,
}

trait ChainType {
	fn runtime(&self) -> Runtime;
}

impl ChainType for dyn ChainSpec {
	fn runtime(&self) -> Runtime {
		runtime(self.id())
	}
}

use sc_chain_spec::GenericChainSpec;
impl ChainType
	for GenericChainSpec<rococo_parachain_runtime::GenesisConfig, chain_spec::Extensions>
{
	fn runtime(&self) -> Runtime {
		runtime(self.id())
	}
}

fn runtime(id: &str) -> Runtime {
	if id.starts_with("shell") {
		Runtime::Shell
	} else if id.starts_with("seedling") {
		Runtime::Seedling
	} else if id.starts_with("statemint") {
		Runtime::Statemint
	} else if id.starts_with("statemine") {
		Runtime::Statemine
	} else if id.starts_with("westmint") {
		Runtime::Westmint
	} else if id.starts_with("contracts-rococo") {
		Runtime::ContractsRococo
	} else {
		Runtime::Generic
	}
}

fn load_spec(id: &str) -> std::result::Result<Box<dyn ChainSpec>, String> {
	Ok(match id {
		"staging" => Box::new(chain_spec::staging_test_net()),
		"tick" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/tick.json")[..],
		)?),
		"trick" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/trick.json")[..],
		)?),
		"track" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/track.json")[..],
		)?),
		"shell" => Box::new(chain_spec::shell::get_shell_chain_spec()),
		// -- Statemint
		"seedling" => Box::new(chain_spec::seedling::get_seedling_chain_spec()),
		"statemint-dev" => Box::new(chain_spec::statemint::statemint_development_config()),
		"statemint-local" => Box::new(chain_spec::statemint::statemint_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"statemint-genesis" => Box::new(chain_spec::statemint::statemint_config()),
		// the shell-based chain spec as used for syncing
		"statemint" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/statemint.json")[..],
		)?),
		// -- Statemine
		"statemine-dev" => Box::new(chain_spec::statemint::statemine_development_config()),
		"statemine-local" => Box::new(chain_spec::statemint::statemine_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"statemine-genesis" => Box::new(chain_spec::statemint::statemine_config()),
		// the shell-based chain spec as used for syncing
		"statemine" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/statemine.json")[..],
		)?),
		// -- Westmint
		"westmint-dev" => Box::new(chain_spec::statemint::westmint_development_config()),
		"westmint-local" => Box::new(chain_spec::statemint::westmint_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"westmint-genesis" => Box::new(chain_spec::statemint::westmint_config()),
		// the shell-based chain spec as used for syncing
		"westmint" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/westmint.json")[..],
		)?),
		// -- Contracts on Rococo
		"contracts-rococo-dev" =>
			Box::new(chain_spec::contracts::contracts_rococo_development_config()),
		"contracts-rococo-local" =>
			Box::new(chain_spec::contracts::contracts_rococo_local_config()),
		"contracts-rococo-genesis" => Box::new(chain_spec::contracts::contracts_rococo_config()),
		"contracts-rococo" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../../parachains/chain-specs/contracts-rococo.json")[..],
		)?),
		// -- Fallback (generic chainspec)
		"" => Box::new(chain_spec::get_chain_spec()),
		// -- Loading a specific spec from disk
		path => {
			let chain_spec = chain_spec::ChainSpec::from_json_file(path.into())?;
			match chain_spec.runtime() {
				Runtime::Statemint => Box::new(
					chain_spec::statemint::StatemintChainSpec::from_json_file(path.into())?,
				),
				Runtime::Statemine => Box::new(
					chain_spec::statemint::StatemineChainSpec::from_json_file(path.into())?,
				),
				Runtime::Westmint =>
					Box::new(chain_spec::statemint::WestmintChainSpec::from_json_file(path.into())?),
				Runtime::Shell =>
					Box::new(chain_spec::shell::ShellChainSpec::from_json_file(path.into())?),
				Runtime::Seedling =>
					Box::new(chain_spec::seedling::SeedlingChainSpec::from_json_file(path.into())?),
				Runtime::ContractsRococo => Box::new(
					chain_spec::contracts::ContractsRococoChainSpec::from_json_file(path.into())?,
				),
				Runtime::Generic => Box::new(chain_spec),
			}
		},
	})
}

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Polkadot parachain".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Polkadot parachain\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relaychain node.\n\n\
		{} [parachain-args] -- [relaychain-args]",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/cumulus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		load_spec(id)
	}

	fn native_runtime_version(chain_spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		match chain_spec.runtime() {
			Runtime::Statemint => &statemint_runtime::VERSION,
			Runtime::Statemine => &statemine_runtime::VERSION,
			Runtime::Westmint => &westmint_runtime::VERSION,
			Runtime::Shell => &shell_runtime::VERSION,
			Runtime::Seedling => &seedling_runtime::VERSION,
			Runtime::ContractsRococo => &contracts_rococo_runtime::VERSION,
			Runtime::Generic => &rococo_parachain_runtime::VERSION,
		}
	}
}

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String {
		"Polkadot parachain".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Polkadot parachain\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		{} [parachain-args] -- [relay_chain-args]",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/cumulus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name()].iter()).load_spec(id)
	}

	fn native_runtime_version(chain_spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		polkadot_cli::Cli::native_runtime_version(chain_spec)
	}
}

/// Creates partial components for the runtimes that are supported by the benchmarks.
macro_rules! construct_benchmark_partials {
	($config:expr, |$partials:ident| $code:expr) => {
		match $config.chain_spec.runtime() {
			Runtime::Statemine => {
				let $partials = new_partial::<statemine_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, AuraId>,
				)?;
				$code
			},
			Runtime::Westmint => {
				let $partials = new_partial::<westmint_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, AuraId>,
				)?;
				$code
			},
			Runtime::Statemint => {
				let $partials = new_partial::<statemint_runtime::RuntimeApi, _>(
					&$config,
					crate::service::aura_build_import_queue::<_, StatemintAuraId>,
				)?;
				$code
			},
			_ => Err("The chain is not supported".into()),
		}
	};
}

macro_rules! construct_async_run {
	(|$components:ident, $cli:ident, $cmd:ident, $config:ident| $( $code:tt )* ) => {{
		let runner = $cli.create_runner($cmd)?;
		match runner.config().chain_spec.runtime() {
			Runtime::Westmint => {
				runner.async_run(|$config| {
					let $components = new_partial::<westmint_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Statemine => {
				runner.async_run(|$config| {
					let $components = new_partial::<statemine_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, AuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Statemint => {
				runner.async_run(|$config| {
					let $components = new_partial::<statemint_runtime::RuntimeApi, _>(
						&$config,
						crate::service::aura_build_import_queue::<_, StatemintAuraId>,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Shell => {
				runner.async_run(|$config| {
					let $components = new_partial::<shell_runtime::RuntimeApi, _>(
						&$config,
						crate::service::shell_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Seedling => {
				runner.async_run(|$config| {
					let $components = new_partial::<seedling_runtime::RuntimeApi, _>(
						&$config,
						crate::service::shell_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::ContractsRococo => {
				runner.async_run(|$config| {
					let $components = new_partial::<contracts_rococo_runtime::RuntimeApi, _>(
						&$config,
						crate::service::contracts_rococo_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			},
			Runtime::Generic => {
				runner.async_run(|$config| {
					let $components = new_partial::<
						rococo_parachain_runtime::RuntimeApi,
						_
					>(
						&$config,
						crate::service::rococo_parachain_build_import_queue,
					)?;
					let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
				})
			}
		}
	}}
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
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.import_queue))
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, config.database))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, config.chain_spec))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.import_queue))
			})
		},
		Some(Subcommand::Revert(cmd)) => construct_async_run!(|components, cli, cmd, config| {
			Ok(cmd.run(components.client, components.backend, None))
		}),
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name()].iter().chain(cli.relaychain_args.iter()),
				);

				let polkadot_config = SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|err| format!("Relay chain argument error: {}", err))?;

				cmd.run(config, polkadot_config)
			})
		},
		Some(Subcommand::ExportGenesisState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|_config| {
				let spec = cli.load_spec(&cmd.shared_params.chain.clone().unwrap_or_default())?;
				let state_version = Cli::native_runtime_version(&spec).state_version();
				cmd.run::<crate::service::Block>(&*spec, state_version)
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
				BenchmarkCmd::Pallet(cmd) =>
					if cfg!(feature = "runtime-benchmarks") {
						runner.sync_run(|config| match config.chain_spec.runtime() {
							Runtime::Statemine =>
								cmd.run::<Block, StatemineRuntimeExecutor>(config),
							Runtime::Westmint => cmd.run::<Block, WestmintRuntimeExecutor>(config),
							Runtime::Statemint =>
								cmd.run::<Block, StatemintRuntimeExecutor>(config),
							_ => Err("Chain doesn't support benchmarking".into()),
						})
					} else {
						Err("Benchmarking wasn't enabled when building the node. \
				You can enable it with `--features runtime-benchmarks`."
							.into())
					},
				BenchmarkCmd::Block(cmd) => runner.sync_run(|config| {
					construct_benchmark_partials!(config, |partials| cmd.run(partials.client))
				}),
				BenchmarkCmd::Storage(cmd) => runner.sync_run(|config| {
					construct_benchmark_partials!(config, |partials| {
						let db = partials.backend.expose_db();
						let storage = partials.backend.expose_storage();

						cmd.run(config, partials.client.clone(), db, storage)
					})
				}),
				BenchmarkCmd::Overhead(_) => Err("Unsupported benchmarking command".into()),
				BenchmarkCmd::Machine(cmd) =>
					runner.sync_run(|config| cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())),
			}
		},
		Some(Subcommand::TryRuntime(cmd)) => {
			if cfg!(feature = "try-runtime") {
				// grab the task manager.
				let runner = cli.create_runner(cmd)?;
				let registry = &runner.config().prometheus_config.as_ref().map(|cfg| &cfg.registry);
				let task_manager =
					TaskManager::new(runner.config().tokio_handle.clone(), *registry)
						.map_err(|e| format!("Error: {:?}", e))?;

				match runner.config().chain_spec.runtime() {
					Runtime::Statemine => runner.async_run(|config| {
						Ok((cmd.run::<Block, StatemineRuntimeExecutor>(config), task_manager))
					}),
					Runtime::Westmint => runner.async_run(|config| {
						Ok((cmd.run::<Block, WestmintRuntimeExecutor>(config), task_manager))
					}),
					Runtime::Statemint => runner.async_run(|config| {
						Ok((cmd.run::<Block, StatemintRuntimeExecutor>(config), task_manager))
					}),
					Runtime::Shell => runner.async_run(|config| {
						Ok((cmd.run::<Block, ShellRuntimeExecutor>(config), task_manager))
					}),
					_ => Err("Chain doesn't support try-runtime".into()),
				}
			} else {
				Err("Try-runtime must be enabled by `--features try-runtime`.".into())
			}
		},
		Some(Subcommand::Key(cmd)) => Ok(cmd.run(&cli)?),
		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;
			let collator_options = cli.run.collator_options();

			runner.run_node_until_exit(|config| async move {
				let hwbench = if !cli.no_hardware_benchmarks {
					config.database.path().map(|database_path| {
						let _ = std::fs::create_dir_all(&database_path);
						sc_sysinfo::gather_hwbench(Some(database_path))
					})
				} else {
					None
				};

				let para_id = chain_spec::Extensions::try_get(&*config.chain_spec)
					.map(|e| e.para_id)
					.ok_or_else(|| "Could not find parachain extension in chain-spec.")?;

				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name()].iter().chain(cli.relaychain_args.iter()),
				);

				let id = ParaId::from(para_id);

				let parachain_account =
					AccountIdConversion::<polkadot_primitives::v2::AccountId>::into_account_truncating(&id);

				let state_version = Cli::native_runtime_version(&config.chain_spec).state_version();

				let block: crate::service::Block =
					generate_genesis_block(&*config.chain_spec, state_version)
						.map_err(|e| format!("{:?}", e))?;
				let genesis_state = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

				let tokio_handle = config.tokio_handle.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, tokio_handle)
						.map_err(|err| format!("Relay chain argument error: {}", err))?;

				info!("Parachain id: {:?}", id);
				info!("Parachain Account: {}", parachain_account);
				info!("Parachain genesis state: {}", genesis_state);
				info!("Is collating: {}", if config.role.is_authority() { "yes" } else { "no" });

				match config.chain_spec.runtime() {
					Runtime::Statemint => crate::service::start_generic_aura_node::<
						statemint_runtime::RuntimeApi,
						StatemintAuraId,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::Statemine => crate::service::start_generic_aura_node::<
						statemine_runtime::RuntimeApi,
						AuraId,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::Westmint => crate::service::start_generic_aura_node::<
						westmint_runtime::RuntimeApi,
						AuraId,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::Shell =>
						crate::service::start_shell_node::<shell_runtime::RuntimeApi>(
							config,
							polkadot_config,
							collator_options,
							id,
							hwbench,
						)
						.await
						.map(|r| r.0)
						.map_err(Into::into),
					Runtime::Seedling => crate::service::start_shell_node::<
						seedling_runtime::RuntimeApi,
					>(config, polkadot_config, collator_options, id, hwbench)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::ContractsRococo => crate::service::start_contracts_rococo_node(
						config,
						polkadot_config,
						collator_options,
						id,
						hwbench,
					)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
					Runtime::Generic => crate::service::start_rococo_parachain_node(
						config,
						polkadot_config,
						collator_options,
						id,
						hwbench,
					)
					.await
					.map(|r| r.0)
					.map_err(Into::into),
				}
			})
		},
	}
}

impl DefaultConfigurationValues for RelayChainCli {
	fn p2p_listen_port() -> u16 {
		30334
	}

	fn rpc_ws_listen_port() -> u16 {
		9945
	}

	fn rpc_http_listen_port() -> u16 {
		9934
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
			.base_path()
			.or_else(|| self.base_path.clone().map(Into::into)))
	}

	fn rpc_http(&self, default_listen_port: u16) -> Result<Option<SocketAddr>> {
		self.base.base.rpc_http(default_listen_port)
	}

	fn rpc_ipc(&self) -> Result<Option<String>> {
		self.base.base.rpc_ipc()
	}

	fn rpc_ws(&self, default_listen_port: u16) -> Result<Option<SocketAddr>> {
		self.base.base.rpc_ws(default_listen_port)
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

	fn transaction_pool(&self) -> Result<sc_service::config::TransactionPoolOptions> {
		self.base.base.transaction_pool()
	}

	fn state_cache_child_ratio(&self) -> Result<Option<usize>> {
		self.base.base.state_cache_child_ratio()
	}

	fn rpc_methods(&self) -> Result<sc_service::config::RpcMethods> {
		self.base.base.rpc_methods()
	}

	fn rpc_ws_max_connections(&self) -> Result<Option<usize>> {
		self.base.base.rpc_ws_max_connections()
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
