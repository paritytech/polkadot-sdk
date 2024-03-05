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
use frame_benchmarking_cli::BenchmarkCmd;
use sc_cli::Result;
use sp_runtime::traits::BlakeTwo256;

/// Benchmark FRAME runtimes.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Command {
	#[command(subcommand)]
	sub: SubCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
	/// Sub-commands concerned with benchmarking.
	V1(V1Command),
	// NOTE: Here we can add new commands in a forward-compatible way. For example when
	// transforming the CLI from a monolithic design to a data driven pipeline, there could be
	// commands like `measure`, `analyze` and `render`.
}

/// A command that conforms to the legacy `benchmark` argument syntax.
#[derive(Parser, Debug)]
pub struct V1Command {
	#[command(subcommand)]
	sub: BenchmarkCmd,
}

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

fn main() -> Result<()> {
	env_logger::init();
	log::warn!(
		"FRAME benchmark runner v{} is not yet battle tested - use with care.",
		env!("CARGO_PKG_VERSION")
	);

	match Command::parse().sub {
		SubCommand::V1(V1Command { sub: BenchmarkCmd::Pallet(pallet) }) =>
			pallet.run_with_spec::<BlakeTwo256, HostFunctions>(None),
		_ => Err("Invalid subcommand. Only `v1 pallet` is supported.".into()),
	}
}
