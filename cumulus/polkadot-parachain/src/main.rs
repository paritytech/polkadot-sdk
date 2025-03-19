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

use clap::{Parser};
use polkadot_omni_node_lib::{
	chain_spec::LoadSpec, cli::CustomCommandHandler, run, CliConfig as CliConfigT, RunConfig,
	NODE_VERSION,
};
use sc_cli::ExportChainSpecCmd;

struct MyCustomCommandHandler;

impl CustomCommandHandler for MyCustomCommandHandler {
	fn handle_command(&self, subcommand: &str, args: &[String]) -> Option<sc_cli::Result<()>> {
		if subcommand == "export-chain-spec" {
			// Reconstruct the full argument vector; first element is the command name.
			let full_args = std::iter::once(subcommand.to_string())
				.chain(args.iter().cloned())
				.collect::<Vec<String>>();
			// Parse the arguments into an ExportChainSpecCmd using Clap.
			let export_cmd = match ExportChainSpecCmd::try_parse_from(full_args) {
				Ok(cmd) => cmd,
				Err(e) => return Some(Err(sc_cli::Error::Application(Box::new(
					std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
				)))),
			};
			// Load the chain spec using your local chain spec loader.
			let spec = match chain_spec::ChainSpecLoader.load_spec(&export_cmd.chain) {
				Ok(spec) => spec,
				Err(e) => return Some(Err(sc_cli::Error::Application(Box::new(
					std::io::Error::new(std::io::ErrorKind::Other, e)
				)))),
			};
			return Some(export_cmd.run(spec));
		}
		None
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
		Some(Box::new(MyCustomCommandHandler)),
	);
	Ok(run::<CliConfig>(config)?)
}
