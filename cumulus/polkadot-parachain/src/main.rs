// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
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

use clap::{Command, CommandFactory, FromArgMatches};
use color_eyre::eyre;
use polkadot_omni_node_lib::{
	chain_spec::LoadSpec, cli::Cli as OmniCli, run, CliConfig as CliConfigT, RunConfig,
	NODE_VERSION,
};
use sc_cli::ExportChainSpecCmd;

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

fn main() -> eyre::Result<()> {
	color_eyre::install()?;

	// Build the omni-node CLI command with version info.
	let mut cmd: Command = OmniCli::<CliConfig>::command().version(NODE_VERSION);

	// Add our export command under the new name "export-chain-spec".
	cmd = cmd.subcommand(ExportChainSpecCmd::command().name("export-chain-spec"));

	// Parse the combined CLI.
	let matches = cmd.get_matches();

	// If the export-chain-spec subcommand is invoked, execute that branch.
	if let Some(export_matches) = matches.subcommand_matches("export-chain-spec") {
		// Clone the matches to get an owned mutable instance.
		let mut export_matches_owned = export_matches.clone();
		let export_cmd = ExportChainSpecCmd::from_arg_matches_mut(&mut export_matches_owned)?;
		let loader = chain_spec::ChainSpecLoader;
		let spec = loader.load_spec(&export_cmd.chain).map_err(|e: String| eyre::eyre!(e))?;
		export_cmd.run(spec).map_err(Into::into)
	} else {
		let config = RunConfig::new(
			Box::new(chain_spec::RuntimeResolver),
			Box::new(chain_spec::ChainSpecLoader),
		);
		Ok(run::<CliConfig>(config)?)
	}
}
