// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Polkadot parachain node.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod chain_spec;

use clap::{Args, FromArgMatches};
use polkadot_omni_node_lib::{
	chain_spec::LoadSpec, cli::ExtraCommandProvider, run, CliConfig as CliConfigT, RunConfig,
	NODE_VERSION,
};
use polkadot_omni_node_lib::extra_commands::ExtraCommandsHandler;

/// Struct to use extra commands within polkadot-parachain
pub struct ParachainExtraCommands {
	handler: ExtraCommandsHandler,
}

impl ParachainExtraCommands {
	pub fn new(chain_spec_loader: Box<dyn LoadSpec>) -> Self {
		Self {
			handler: ExtraCommandsHandler::new(chain_spec_loader),
		}
	}
}

impl ExtraCommandProvider for ParachainExtraCommands {
	fn handle_extra_command(&self, name: &str, matches: &clap::ArgMatches) -> sc_cli::Result<()> {
		self.handler.handle(name, matches)
	}

	fn augment_command(&self, cmd: clap::Command) -> clap::Command {
		let mut cmd = cmd;
		for (name, subcmd) in ExtraCommandsHandler::available_commands() {
			if name == "export-chain-spec" { // <-- Optional condition: can choose which ones to expose
				cmd = cmd.subcommand(subcmd);
			}
		}
		cmd
	}

	fn available_commands(&self) -> Vec<&'static str> {
		vec!["export-chain-spec"]
	}
}

struct CliConfig;

impl CliConfigT for CliConfig {
	fn impl_version() -> String {
		let commit_hash = env!("SUBSTRATE_CLI_COMMIT_HASH");
		format!("{}-{commit_hash}", NODE_VERSION)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/polkadot-sdk/issues/new".into()
	}

	fn copyright_start_year() -> u16 {
		2017
	}
}

fn main() -> color_eyre::eyre::Result<()> {
	color_eyre::install()?;

	let config = RunConfig::new(
		Box::new(chain_spec::RuntimeResolver),
		Box::new(chain_spec::ChainSpecLoader),
		Some(Box::new(ParachainExtraCommands::new(Box::new(chain_spec::ChainSpecLoader)))),
	);
	Ok(run::<CliConfig>(config)?)
}
