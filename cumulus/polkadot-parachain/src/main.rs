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

use clap::{Parser, Subcommand};
use polkadot_omni_node_lib::{
	chain_spec::LoadSpec,
	cli,
	cli::{ExtraCommandProvider, ExtraSubcommand},
	run, CliConfig as CliConfigT, RunConfig, NODE_VERSION,
};
use sc_cli::Error::Application;
use std::io;

/// Struct to use extra commands within polkadot-parachain
pub struct ExportChainSpecExtra;

impl ExtraCommandProvider for ExportChainSpecExtra {
	type Command = ExtraSubcommand;

	fn handle_command(&self, cmd: &Self::Command) -> sc_cli::Result<()> {
		match cmd {
			ExtraSubcommand::ExportChainSpec(ref export_cmd) => {
				let spec = chain_spec::ChainSpecLoader
					.load_spec(&export_cmd.chain)
					.map_err(|e| Application(Box::new(io::Error::new(io::ErrorKind::Other, e))))?;
				export_cmd.run(spec)
			},
		}
	}

	fn augment_command(&self, cmd: clap::Command) -> clap::Command {
		cmd.subcommand(
			clap::Command::new("export-chain-spec")
				.about("Export the chain specification")
				.arg(
					clap::Arg::new("chain").long("chain").help("Specify the chain").required(true),
				),
		)
	}
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct ParachainCli {
	#[clap(flatten)]
	pub run: sc_cli::RunCmd,
	#[clap(subcommand)]
	pub command: ParachainCommand,
}
#[derive(Debug, Subcommand)]
enum ParachainCommand {
	#[command(flatten)]
	BuiltIn(cli::Subcommand),
	#[command(flatten)]
	Extra(cli::ExtraSubcommand),
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
		Some(Box::new(ExportChainSpecExtra)),
	);
	Ok(run::<CliConfig>(config)?)
}
