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

use clap::Parser;
use frame_benchmarking_cli::{BenchmarkCmd, OpaqueBlock};
use sc_cli::Result;
use sp_runtime::traits::BlakeTwo256;

#[derive(Parser, Debug)]
#[clap(author, version, about, verbatim_doc_comment)]
pub struct Command {
	#[command(subcommand)]
	sub: SubCommand,
}

/// Root-level subcommands.
#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
	/// Compatibility syntax with the old benchmark runner.
	V1(V1Command),
	// NOTE: Here we can add new commands in a forward-compatible way. For example when
	// transforming the CLI from a monolithic design to a data driven pipeline, there could be
	// commands like `measure`, `analyze` and `render`.
}

/// A command that conforms to the legacy `benchmark` argument syntax.
#[derive(Parser, Debug)]
pub struct V1Command {
	#[command(subcommand)]
	sub: V1SubCommand,
}

/// The `v1 benchmark` subcommand.
#[derive(Debug, clap::Subcommand)]
pub enum V1SubCommand {
	Benchmark(V1BenchmarkCommand),
}

/// Subcommands for `v1 benchmark`.
#[derive(Parser, Debug)]
pub struct V1BenchmarkCommand {
	#[command(subcommand)]
	sub: BenchmarkCmd,
}

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
	sp_crypto_ec_utils::HostFunctionsRfc163,
);

impl Command {
	pub fn run(self) -> Result<()> {
		match self.sub {
			SubCommand::V1(V1Command { sub }) => sub.run(),
		}
	}
}
impl V1SubCommand {
	pub fn run(self) -> Result<()> {
		match self {
			V1SubCommand::Benchmark(V1BenchmarkCommand { sub }) => match sub {
				BenchmarkCmd::Pallet(pallet) => {
					pallet.run_with_spec::<BlakeTwo256, HostFunctions>(None)
				},
				BenchmarkCmd::Overhead(overhead_cmd) =>
					overhead_cmd.run_with_default_builder_and_spec::<OpaqueBlock, HostFunctions>(None),
				_ =>
					return Err(
						"Only the `v1 benchmark pallet` and `v1 benchmark overhead` command is currently supported".into()
					),
			},
		}
	}
}
