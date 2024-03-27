use crate::{
	cli::{Cli, Subcommand},
	service,
	standards::OpaqueBlock as Block,
};
use sc_cli::SubstrateCli;
use sc_service::{ChainType, PartialComponents, Properties};

pub type ChainSpec = sc_service::GenericChainSpec<()>;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Substrate Omni Node".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		env!("CARGO_PKG_DESCRIPTION").into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"support.anonymous.an".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, maybe_path: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(Box::new(if maybe_path.is_empty() {
			let code = std::fs::read(&self.runtime)
				.map_err(|e| format!("Failed to read runtime {}: {}", &self.runtime, e))?;

			log::info!("No --chain provided; using default chain-spec and --runtime");

			let mut properties = Properties::new();
			properties.insert("tokenDecimals".to_string(), 0.into());
			properties.insert("tokenSymbol".to_string(), "SOLO".into());

			let tmp = sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<'_, ()>::new(&code);
			let genesis = tmp.get_default_config()?;

			ChainSpec::builder(code.as_ref(), Default::default())
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

pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&config)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, .. } = service::new_partial(&config)?;
				Ok((cmd.run(client, config.database), task_manager))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, .. } = service::new_partial(&config)?;
				Ok((cmd.run(client, config.chain_spec), task_manager))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&config)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.database))
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, backend, .. } =
					service::new_partial(&config)?;
				let aux_revert = Box::new(|client, _, blocks| {
					sc_consensus_grandpa::revert(client, blocks)?;
					Ok(())
				});
				Ok((cmd.run(client, backend, Some(aux_revert)), task_manager))
			})
		},
		Some(Subcommand::ChainInfo(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run::<Block>(&config))
		},
		None => {
			let runner = cli.create_runner(&cli.run)?;
			runner.run_node_until_exit(|config| async move {
				service::new_full(config).map_err(sc_cli::Error::Service)
			})
		},
	}
}
