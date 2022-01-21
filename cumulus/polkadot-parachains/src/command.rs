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

use crate::{
	chain_spec,
	cli::{Cli, RelayChainCli, Subcommand},
	service::{
		new_partial, Block, RococoParachainRuntimeExecutor, SeedlingRuntimeExecutor,
		ShellRuntimeExecutor, StatemineRuntimeExecutor, StatemintRuntimeExecutor,
		WestmintRuntimeExecutor,
	},
};
use codec::Encode;
use cumulus_client_service::genesis::generate_genesis_block;
use cumulus_primitives_core::ParaId;
use log::info;
use parachains_common::{AuraId, StatemintAuraId};
use polkadot_parachain::primitives::AccountIdConversion;
use sc_cli::{
	ChainSpec, CliConfiguration, DefaultConfigurationValues, ImportParams, KeystoreParams,
	NetworkParams, Result, RuntimeVersion, SharedParams, SubstrateCli,
};
use sc_service::{
	config::{BasePath, PrometheusConfig},
	TaskManager,
};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::traits::Block as BlockT;
use std::{io::Write, net::SocketAddr};

trait IdentifyChain {
	fn is_shell(&self) -> bool;
	fn is_seedling(&self) -> bool;
	fn is_statemint(&self) -> bool;
	fn is_statemine(&self) -> bool;
	fn is_westmint(&self) -> bool;
}

impl IdentifyChain for dyn sc_service::ChainSpec {
	fn is_shell(&self) -> bool {
		self.id().starts_with("shell")
	}
	fn is_seedling(&self) -> bool {
		self.id().starts_with("seedling")
	}
	fn is_statemint(&self) -> bool {
		self.id().starts_with("statemint")
	}
	fn is_statemine(&self) -> bool {
		self.id().starts_with("statemine")
	}
	fn is_westmint(&self) -> bool {
		self.id().starts_with("westmint")
	}
}

impl<T: sc_service::ChainSpec + 'static> IdentifyChain for T {
	fn is_shell(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_shell(self)
	}
	fn is_seedling(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_seedling(self)
	}
	fn is_statemint(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_statemint(self)
	}
	fn is_statemine(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_statemine(self)
	}
	fn is_westmint(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_westmint(self)
	}
}

fn load_spec(id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
	Ok(match id {
		"staging" => Box::new(chain_spec::staging_test_net()),
		"tick" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../res/tick.json")[..],
		)?),
		"trick" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../res/trick.json")[..],
		)?),
		"track" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../res/track.json")[..],
		)?),
		"shell" => Box::new(chain_spec::get_shell_chain_spec()),
		// -- Statemint
		"seedling" => Box::new(chain_spec::get_seedling_chain_spec()),
		"statemint-dev" => Box::new(chain_spec::statemint_development_config()),
		"statemint-local" => Box::new(chain_spec::statemint_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"statemint-genesis" => Box::new(chain_spec::statemint_config()),
		// the shell-based chain spec as used for syncing
		"statemint" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../res/statemint.json")[..],
		)?),
		// -- Statemine
		"statemine-dev" => Box::new(chain_spec::statemine_development_config()),
		"statemine-local" => Box::new(chain_spec::statemine_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"statemine-genesis" => Box::new(chain_spec::statemine_config()),
		// the shell-based chain spec as used for syncing
		"statemine" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../res/statemine.json")[..],
		)?),
		// -- Westmint
		"westmint-dev" => Box::new(chain_spec::westmint_development_config()),
		"westmint-local" => Box::new(chain_spec::westmint_local_config()),
		// the chain spec as used for generating the upgrade genesis values
		"westmint-genesis" => Box::new(chain_spec::westmint_config()),
		// the shell-based chain spec as used for syncing
		"westmint" => Box::new(chain_spec::ChainSpec::from_json_bytes(
			&include_bytes!("../res/westmint.json")[..],
		)?),
		"" => Box::new(chain_spec::get_chain_spec()),
		path => {
			let chain_spec = chain_spec::ChainSpec::from_json_file(path.into())?;
			if chain_spec.is_statemint() {
				Box::new(chain_spec::StatemintChainSpec::from_json_file(path.into())?)
			} else if chain_spec.is_statemine() {
				Box::new(chain_spec::StatemineChainSpec::from_json_file(path.into())?)
			} else if chain_spec.is_westmint() {
				Box::new(chain_spec::WestmintChainSpec::from_json_file(path.into())?)
			} else if chain_spec.is_shell() {
				Box::new(chain_spec::ShellChainSpec::from_json_file(path.into())?)
			} else if chain_spec.is_seedling() {
				Box::new(chain_spec::SeedlingChainSpec::from_json_file(path.into())?)
			} else {
				Box::new(chain_spec)
			}
		},
	})
}

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Polkadot collator".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Polkadot collator\n\nThe command-line arguments provided first will be \
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
		if chain_spec.is_statemint() {
			&statemint_runtime::VERSION
		} else if chain_spec.is_statemine() {
			&statemine_runtime::VERSION
		} else if chain_spec.is_westmint() {
			&westmint_runtime::VERSION
		} else if chain_spec.is_shell() {
			&shell_runtime::VERSION
		} else if chain_spec.is_seedling() {
			&seedling_runtime::VERSION
		} else {
			&rococo_parachain_runtime::VERSION
		}
	}
}

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String {
		"Polkadot collator".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Polkadot collator\n\nThe command-line arguments provided first will be \
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
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name().to_string()].iter())
			.load_spec(id)
	}

	fn native_runtime_version(chain_spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		polkadot_cli::Cli::native_runtime_version(chain_spec)
	}
}

fn extract_genesis_wasm(chain_spec: &Box<dyn sc_service::ChainSpec>) -> Result<Vec<u8>> {
	let mut storage = chain_spec.build_storage()?;

	storage
		.top
		.remove(sp_core::storage::well_known_keys::CODE)
		.ok_or_else(|| "Could not find wasm file in genesis state!".into())
}

macro_rules! construct_async_run {
	(|$components:ident, $cli:ident, $cmd:ident, $config:ident| $( $code:tt )* ) => {{
		let runner = $cli.create_runner($cmd)?;
		if runner.config().chain_spec.is_westmint() {
			runner.async_run(|$config| {
				let $components = new_partial::<westmint_runtime::RuntimeApi, WestmintRuntimeExecutor, _>(
					&$config,
					crate::service::statemint_build_import_queue::<_, _, AuraId>,
				)?;
				let task_manager = $components.task_manager;
				{ $( $code )* }.map(|v| (v, task_manager))
			})
		} else if runner.config().chain_spec.is_statemine() {
			runner.async_run(|$config| {
				let $components = new_partial::<statemine_runtime::RuntimeApi, StatemineRuntimeExecutor, _>(
					&$config,
					crate::service::statemint_build_import_queue::<_, _, AuraId>,
				)?;
				let task_manager = $components.task_manager;
				{ $( $code )* }.map(|v| (v, task_manager))
			})
		} else if runner.config().chain_spec.is_statemint() {
			runner.async_run(|$config| {
				let $components = new_partial::<statemint_runtime::RuntimeApi, StatemintRuntimeExecutor, _>(
					&$config,
					crate::service::statemint_build_import_queue::<_, _, StatemintAuraId>,
				)?;
				let task_manager = $components.task_manager;
				{ $( $code )* }.map(|v| (v, task_manager))
			})
		} else if runner.config().chain_spec.is_shell() {
			runner.async_run(|$config| {
				let $components = new_partial::<shell_runtime::RuntimeApi, ShellRuntimeExecutor, _>(
					&$config,
					crate::service::shell_build_import_queue,
				)?;
				let task_manager = $components.task_manager;
				{ $( $code )* }.map(|v| (v, task_manager))
			})
		} else if runner.config().chain_spec.is_seedling() {
			runner.async_run(|$config| {
				let $components = new_partial::<seedling_runtime::RuntimeApi, SeedlingRuntimeExecutor, _>(
					&$config,
					crate::service::shell_build_import_queue,
				)?;
				let task_manager = $components.task_manager;
				{ $( $code )* }.map(|v| (v, task_manager))
			})
		} else {
			runner.async_run(|$config| {
				let $components = new_partial::<
					rococo_parachain_runtime::RuntimeApi,
					RococoParachainRuntimeExecutor,
					_
				>(
					&$config,
					crate::service::rococo_parachain_build_import_queue,
				)?;
				let task_manager = $components.task_manager;
				{ $( $code )* }.map(|v| (v, task_manager))
			})
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
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name().to_string()]
						.iter()
						.chain(cli.relaychain_args.iter()),
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
		Some(Subcommand::Revert(cmd)) => construct_async_run!(|components, cli, cmd, config| {
			Ok(cmd.run(components.client, components.backend))
		}),
		Some(Subcommand::ExportGenesisState(params)) => {
			let mut builder = sc_cli::LoggerBuilder::new("");
			builder.with_profiling(sc_tracing::TracingReceiver::Log, "");
			let _ = builder.init();

			let spec = load_spec(&params.chain.clone().unwrap_or_default())?;
			let state_version = Cli::native_runtime_version(&spec).state_version();

			let block: crate::service::Block = generate_genesis_block(&spec, state_version)?;
			let raw_header = block.header().encode();
			let output_buf = if params.raw {
				raw_header
			} else {
				format!("0x{:?}", HexDisplay::from(&block.header().encode())).into_bytes()
			};

			if let Some(output) = &params.output {
				std::fs::write(output, output_buf)?;
			} else {
				std::io::stdout().write_all(&output_buf)?;
			}

			Ok(())
		},
		Some(Subcommand::ExportGenesisWasm(params)) => {
			let mut builder = sc_cli::LoggerBuilder::new("");
			builder.with_profiling(sc_tracing::TracingReceiver::Log, "");
			let _ = builder.init();

			let raw_wasm_blob =
				extract_genesis_wasm(&cli.load_spec(&params.chain.clone().unwrap_or_default())?)?;
			let output_buf = if params.raw {
				raw_wasm_blob
			} else {
				format!("0x{:?}", HexDisplay::from(&raw_wasm_blob)).into_bytes()
			};

			if let Some(output) = &params.output {
				std::fs::write(output, output_buf)?;
			} else {
				std::io::stdout().write_all(&output_buf)?;
			}

			Ok(())
		},
		Some(Subcommand::Benchmark(cmd)) =>
			if cfg!(feature = "runtime-benchmarks") {
				let runner = cli.create_runner(cmd)?;
				if runner.config().chain_spec.is_statemine() {
					runner.sync_run(|config| cmd.run::<Block, StatemineRuntimeExecutor>(config))
				} else if runner.config().chain_spec.is_westmint() {
					runner.sync_run(|config| cmd.run::<Block, WestmintRuntimeExecutor>(config))
				} else if runner.config().chain_spec.is_statemint() {
					runner.sync_run(|config| cmd.run::<Block, StatemintRuntimeExecutor>(config))
				} else {
					Err("Chain doesn't support benchmarking".into())
				}
			} else {
				Err("Benchmarking wasn't enabled when building the node. \
				You can enable it with `--features runtime-benchmarks`."
					.into())
			},
		Some(Subcommand::TryRuntime(cmd)) => {
			if cfg!(feature = "try-runtime") {
				// grab the task manager.
				let runner = cli.create_runner(cmd)?;
				let registry = &runner.config().prometheus_config.as_ref().map(|cfg| &cfg.registry);
				let task_manager =
					TaskManager::new(runner.config().tokio_handle.clone(), *registry)
						.map_err(|e| format!("Error: {:?}", e))?;

				if runner.config().chain_spec.is_statemine() {
					runner.async_run(|config| {
						Ok((cmd.run::<Block, StatemineRuntimeExecutor>(config), task_manager))
					})
				} else if runner.config().chain_spec.is_westmint() {
					runner.async_run(|config| {
						Ok((cmd.run::<Block, WestmintRuntimeExecutor>(config), task_manager))
					})
				} else if runner.config().chain_spec.is_statemint() {
					runner.async_run(|config| {
						Ok((cmd.run::<Block, StatemintRuntimeExecutor>(config), task_manager))
					})
				} else if runner.config().chain_spec.is_shell() {
					runner.async_run(|config| {
						Ok((cmd.run::<Block, ShellRuntimeExecutor>(config), task_manager))
					})
				} else {
					Err("Chain doesn't support try-runtime".into())
				}
			} else {
				Err("Try-runtime must be enabled by `--features try-runtime`.".into())
			}
		},
		Some(Subcommand::Key(cmd)) => Ok(cmd.run(&cli)?),
		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;

			runner.run_node_until_exit(|config| async move {
				let para_id = chain_spec::Extensions::try_get(&*config.chain_spec)
					.map(|e| e.para_id)
					.ok_or_else(|| "Could not find parachain extension in chain-spec.")?;

				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name().to_string()]
						.iter()
						.chain(cli.relaychain_args.iter()),
				);

				let id = ParaId::from(para_id);

				let parachain_account =
					AccountIdConversion::<polkadot_primitives::v0::AccountId>::into_account(&id);

				let state_version =
					RelayChainCli::native_runtime_version(&config.chain_spec).state_version();

				let block: crate::service::Block =
					generate_genesis_block(&config.chain_spec, state_version)
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

				if config.chain_spec.is_statemint() {
					crate::service::start_statemint_node::<
						statemint_runtime::RuntimeApi,
						StatemintRuntimeExecutor,
						StatemintAuraId,
					>(config, polkadot_config, id)
					.await
					.map(|r| r.0)
					.map_err(Into::into)
				} else if config.chain_spec.is_statemine() {
					crate::service::start_statemint_node::<
						statemine_runtime::RuntimeApi,
						StatemineRuntimeExecutor,
						AuraId,
					>(config, polkadot_config, id)
					.await
					.map(|r| r.0)
					.map_err(Into::into)
				} else if config.chain_spec.is_westmint() {
					crate::service::start_statemint_node::<
						westmint_runtime::RuntimeApi,
						WestmintRuntimeExecutor,
						AuraId,
					>(config, polkadot_config, id)
					.await
					.map(|r| r.0)
					.map_err(Into::into)
				} else if config.chain_spec.is_shell() {
					crate::service::start_shell_node::<
						shell_runtime::RuntimeApi,
						ShellRuntimeExecutor,
					>(config, polkadot_config, id)
					.await
					.map(|r| r.0)
					.map_err(Into::into)
				} else if config.chain_spec.is_seedling() {
					crate::service::start_shell_node::<
						seedling_runtime::RuntimeApi,
						SeedlingRuntimeExecutor,
					>(config, polkadot_config, id)
					.await
					.map(|r| r.0)
					.map_err(Into::into)
				} else {
					crate::service::start_rococo_parachain_node(config, polkadot_config, id)
						.await
						.map(|r| r.0)
						.map_err(Into::into)
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
}
