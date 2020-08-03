// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use crate::cli::{Cli, Subcommand};
use crate::service;
use crate::service::new_full_params;
use bridge_node_runtime::Block;
use sc_cli::{ChainSpec, Role, RuntimeVersion, SubstrateCli};
use sc_service::ServiceParams;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Bridge Node".into()
	}

	fn impl_version() -> String {
		env!("CARGO_PKG_VERSION").into()
	}

	fn description() -> String {
		"Bridge Node".into()
	}

	fn author() -> String {
		"Parity Technologies".into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/parity-bridges-common/".into()
	}

	fn copyright_start_year() -> i32 {
		2019
	}

	fn executable_name() -> String {
		"bridge-node".into()
	}

	fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		&bridge_node_runtime::VERSION
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(Box::new(
			match id {
				"" | "dev" => crate::chain_spec::Alternative::Development,
				"local" => crate::chain_spec::Alternative::LocalTestnet,
				_ => return Err(format!("Unsupported chain specification: {}", id)),
			}
			.load()?,
		))
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Benchmark(cmd)) => {
			if cfg!(feature = "runtime-benchmarks") {
				let runner = cli.create_runner(cmd)?;

				runner.sync_run(|config| cmd.run::<Block, service::Executor>(config))
			} else {
				println!(
					"Benchmarking wasn't enabled when building the node. \
				You can enable it with `--features runtime-benchmarks`."
				);
				Ok(())
			}
		}
		Some(Subcommand::Base(subcommand)) => {
			let runner = cli.create_runner(subcommand)?;
			runner.run_subcommand(subcommand, |config| {
				let (
					ServiceParams {
						client,
						backend,
						task_manager,
						import_queue,
						..
					},
					..,
				) = new_full_params(config)?;
				Ok((client, backend, import_queue, task_manager))
			})
		}
		None => {
			let runner = cli.create_runner(&cli.run)?;
			runner.run_node_until_exit(|config| match config.role {
				Role::Light => service::new_light(config),
				_ => service::new_full(config),
			})
		}
	}
}
