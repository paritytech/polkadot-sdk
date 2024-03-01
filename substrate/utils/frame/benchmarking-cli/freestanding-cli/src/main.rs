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

//! Entry point for the free standing `benchmark pallet` command runner.
//!
//! This runner has the advantage that the node does not need to be build with the
//! `runtime-benchmarks` feature or similar. It can be shipped independently and used by any chain -
//! as long as that chain's runtime is not making use of 3rd party host functions. In that case, it
//! would need to be forked (or extended with some plugin system).

use clap::Parser;
use frame_benchmarking_cli::BenchmarkCmd;
use sc_cli::Result;
use sp_runtime::traits::BlakeTwo256;

#[derive(Parser, Debug)]
pub struct Command {
	#[command(subcommand)]
	sub: BenchmarkCmd,
}

#[cfg(feature = "extended-host-functions")]
type ExtendedHostFunctions = sp_statement_store::runtime_api::HostFunctions;
#[cfg(not(feature = "extended-host-functions"))]
type ExtendedHostFunctions = ();

fn main() -> Result<()> {
	env_logger::init();
	log::warn!(
		"Experimental benchmark runner v{} - usage will change in the future.",
		env!("CARGO_PKG_VERSION")
	);

	match Command::parse().sub {
		BenchmarkCmd::Pallet(pallet) =>
			pallet.run_with_maybe_spec::<BlakeTwo256, ExtendedHostFunctions>(None),
		_ => Err("Invalid subcommand. Only `pallet` is supported.".into()),
	}
}
