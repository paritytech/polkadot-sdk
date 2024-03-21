// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// TODO:
// - [ ] Ability to pass genesis state in a user friendly way.
// - [ ] Test this with multiple variants of minimal runtime.
// - [ ] what is happening when I run this with --execution Native?
// - [ ] Give it the polkadot runtime. Maybe even that will work?
// - [ ] Give it a parachain runtime.

use crate::{
	cli::{Cli, Subcommand},
	service,
};
use sc_cli::SubstrateCli;
use sc_service::{ChainType, PartialComponents, Properties};

pub type ChainSpec = sc_service::GenericChainSpec<()>;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Substrate Minimal Omni Node".into()
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
		// TODO: it would be good to be able to provide a chain-spec that has no code, and code is
		// still read from `--runtime`. So your chain spec can be cleaner, and it is only initial
		// state.
		let code = std::fs::read(&self.runtime)
			.map_err(|e| format!("Failed to runtime read {}: {}", &self.runtime, e))?;

		Ok(Box::new(if maybe_path.is_empty() {
			println!("Using development chain spec, no genesis state set.");
			let mut properties = Properties::new();
			properties.insert("tokenDecimals".to_string(), 0.into());
			properties.insert("tokenSymbol".to_string(), "MINI".into());

			// `.with_genesis_config(Default::default)` won't work, but should.
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
			println!("Loading chain spec from {}", maybe_path);
			ChainSpec::from_json_file(std::path::PathBuf::from(maybe_path))?
		}))
	}
}

/// Parse and run command line arguments
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
				Ok((cmd.run(client, backend, None), task_manager))
			})
		},
		Some(Subcommand::ChainInfo(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run::<crate::types::OpaqueBlock>(&config))
		},
		None => {
			let runner = cli.create_runner(&cli.run)?;
			runner.run_node_until_exit(|config| async move {
				service::new_full(config, cli.consensus).map_err(sc_cli::Error::Service)
			})
		},
	}
}
