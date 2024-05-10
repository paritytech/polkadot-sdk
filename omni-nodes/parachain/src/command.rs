use crate::cli::{Cli, RelayChainCli, Subcommand};
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use log::info;
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_cli::{
	ChainSpec as ChainSpecT, CliConfiguration, DefaultConfigurationValues, ImportParams,
	KeystoreParams, NetworkParams, Result, SharedParams, SubstrateCli,
};
use sc_service::config::{BasePath, PrometheusConfig};
use serde::{Deserialize, Serialize};
use sp_runtime::traits::AccountIdConversion;
use std::net::SocketAddr;

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type ChainSpec = sc_service::GenericChainSpec<(), Extensions>;

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
// #[serde_alias::serde_alias(CamelCase, SnakeCase)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	#[serde(alias = "relayChain", alias = "RelayChain")]
	pub relay_chain: String,
	/// The id of the Parachain.
	#[serde(alias = "paraId", alias = "ParaId")]
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Parachain Collator Template".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Parachain Collator Template\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		{} <parachain-args> -- <relay-chain-args>",
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
		2020
	}

	fn load_spec(
		&self,
		maybe_path: &str,
	) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		use sc_chain_spec::{ChainType, Properties};
		Ok(Box::new(if maybe_path.is_empty() {
			let code = std::fs::read(&self.runtime)
				.map_err(|e| format!("Failed to read runtime {}: {}", &self.runtime, e))?;

			log::info!("No --chain provided; using temp default chain-spec and --runtime");

			let mut properties = Properties::new();
			properties.insert("tokenDecimals".to_string(), 0.into());
			properties.insert("tokenSymbol".to_string(), "SOLO".into());

			// TODO: these need to be set manually for now.
			let extensions = Extensions { relay_chain: "rococo-local".into(), para_id: 2000 };

			let tmp = sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<'_, ()>::new(&code);
			let genesis = tmp.get_default_config()?;

			ChainSpec::builder(code.as_ref(), extensions)
				.with_name("Development")
				.with_id("dev")
				.with_chain_type(ChainType::Development)
				.with_properties(properties)
				.with_genesis_config(genesis)
				.build()
		} else {
			log::info!(
				"Loading chain spec from {}; this will ignore --runtime for now",
				maybe_path
			);
			ChainSpec::from_json_file(std::path::PathBuf::from(maybe_path))?
		}))
	}
}

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String {
		"Parachain Collator Template".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Parachain Collator Template\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		{} <parachain-args> -- <relay-chain-args>",
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
		2020
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name()].iter()).load_spec(id)
	}
}

/// Parse command line arguments into service configuration.
pub fn run(builder_config: crate::builder::Builder) -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			todo!();
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			todo!();
		},
		Some(Subcommand::ExportState(cmd)) => {
			todo!();
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			todo!();
		},
		Some(Subcommand::Revert(cmd)) => {
			todo!();
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name()].iter().chain(cli.relay_chain_args.iter()),
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
		Some(Subcommand::ExportGenesisHead(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let partials = cumulus_service::new_partial(
					&config,
					cumulus_service::aura::build_import_queue::<
						crate::service::parachain_service::Block,
						crate::service::parachain_service::RuntimeApi,
						crate::service::parachain_service::HostFunctions,
					>,
				)?;
				cmd.run(partials.client)
			})
		},
		Some(Subcommand::ExportGenesisWasm(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|_config| {
				let spec = cli.load_spec(&cmd.shared_params.chain.clone().unwrap_or_default())?;
				cmd.run(&*spec)
			})
		},
		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;
			let collator_options = cli.run.collator_options();

			runner.run_node_until_exit(|parachain_config| async move {
				if let Some(on_load_fn) = builder_config.on_service_load {
					on_load_fn(&parachain_config, Some(collator_options.clone()))?;
				}

				let hwbench = (!cli.no_hardware_benchmarks)
					.then_some(parachain_config.database.path().map(|database_path| {
						let _ = std::fs::create_dir_all(database_path);
						sc_sysinfo::gather_hwbench(Some(database_path))
					}))
					.flatten();

				let para_id = Extensions::try_get(&*parachain_config.chain_spec)
					.map(|e| e.para_id)
					.ok_or("Could not find parachain ID in chain-spec.")?;
				let para_id = ParaId::from(para_id);

				let polkadot_cli = RelayChainCli::new(
					&parachain_config,
					[RelayChainCli::executable_name()].iter().chain(cli.relay_chain_args.iter()),
				);

				let parachain_account =
					AccountIdConversion::<crate::standards::AccountId>::into_account_truncating(
						&para_id,
					);

				let tokio_handle = parachain_config.tokio_handle.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, tokio_handle)
						.map_err(|err| format!("Relay chain argument error: {}", err))?;

				info!("Parachain id: {:?}", para_id);
				info!("Parachain Account: {parachain_account}");
				info!(
					"Is collating: {}",
					if parachain_config.role.is_authority() { "yes" } else { "no" }
				);

				use crate::{
					builder::{NodeType, ParachainConsensus},
					service::parachain_service::start_node_impl,
				};

				match builder_config.node_type {
					NodeType::Parachain(parachain_builder_config) =>
						match parachain_builder_config.consensus {
							ParachainConsensus::Relay(block_time) => {
								use cumulus_service::relay::{build_import_queue, start_consensus};
								start_node_impl(
									parachain_config,
									polkadot_config,
									collator_options,
									CollatorSybilResistance::Unresistant,
									para_id,
									parachain_builder_config.shared.rpc_extensions,
									build_import_queue,
									start_consensus,
									hwbench,
								)
								.await
								.map(|r| r.0)
								.map_err(Into::into)
							},
							ParachainConsensus::Aura(block_time) => {
								use cumulus_service::aura::{build_import_queue, start_consensus};
								start_node_impl(
									parachain_config,
									polkadot_config,
									collator_options,
									CollatorSybilResistance::Resistant,
									para_id,
									parachain_builder_config.shared.rpc_extensions,
									build_import_queue,
									start_consensus,
									hwbench,
								)
								.await
								.map(|r| r.0)
								.map_err(Into::into)
							},
							ParachainConsensus::AuraAsyncBacking(block_time) => {
								use cumulus_service::aura_async_backing::{
									build_import_queue, start_consensus,
								};
								start_node_impl(
									parachain_config,
									polkadot_config,
									collator_options,
									CollatorSybilResistance::Resistant,
									para_id,
									parachain_builder_config.shared.rpc_extensions,
									build_import_queue,
									start_consensus,
									hwbench,
								)
								.await
								.map(|r| r.0)
								.map_err(Into::into)
							},
							ParachainConsensus::RelayToAura(block_time) => {
								todo!("call into start_node_impl using fns from cumulus_service");
							},
						},
					NodeType::Solochain(config) => {
						todo!();
					},
				}
			})
		},
	}
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
		chain_spec: &Box<dyn ChainSpecT>,
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
		chain_spec: &Box<dyn ChainSpecT>,
	) -> Result<Option<sc_telemetry::TelemetryEndpoints>> {
		self.base.base.telemetry_endpoints(chain_spec)
	}

	fn node_name(&self) -> Result<String> {
		self.base.base.node_name()
	}
}
