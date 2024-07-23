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

use crate::{
	cli::{Cli, Subcommand},
	service,
};
use sc_cli::SubstrateCli;
use sc_service::{ChainType, NoExtension, PartialComponents, Properties};
type ChainSpec = sc_chain_spec::GenericChainSpec<NoExtension, ()>;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Polkadot SDK Dev Omni Node".into()
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
		match maybe_path {
			"" => Err("No --chain provided".into()),
			"dev" | "local" =>
				Err("--dev, --chain=dev, --chain=local or any other 'magic' chain id is not \
				supported in omni-node, please provide --chain chain-spec.json or --chain \
				runtime.wasm"
					.into()),
			x if x.ends_with("json") => {
				log::info!("Loading json chain spec from {}", maybe_path);
				Ok(Box::new(ChainSpec::from_json_file(std::path::PathBuf::from(maybe_path))?))
			},
			x if x.ends_with(".wasm") => {
				log::info!("wasm file provided to --chain using default 'preset' for genesis and given wasm file as code");
				let code = std::fs::read(maybe_path)
					.map_err(|e| format!("Failed to read wasm runtime {}: {}", &maybe_path, e))?;

				let mut properties = Properties::new();
				properties.insert("tokenDecimals".to_string(), 0.into());
				properties.insert("tokenSymbol".to_string(), "OMNI".into());

				Ok(Box::new(
					ChainSpec::builder(code.as_ref(), None)
						.with_name("Development")
						.with_id("dev")
						.with_chain_type(ChainType::Development)
						.with_properties(properties)
						.build(),
				))
			},
			_ => Err("Unknown argument to --chain. should be `.wasm` or `.json`".into()),
		}
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
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
			runner.sync_run(|config| cmd.run::<crate::standards::OpaqueBlock>(&config))
		},
		None => {
			let runner = cli.create_runner(&cli.run)?;
			runner.run_node_until_exit(|config| async move {
				service::new_full(config, cli.consensus).map_err(sc_cli::Error::Service)
			})
		},
	}
}
