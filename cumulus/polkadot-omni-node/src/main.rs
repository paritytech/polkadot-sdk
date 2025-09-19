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

//! White labeled polkadot omni-node.
//!
//! For documentation, see [`polkadot_omni_node_lib`].

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

use polkadot_omni_node_lib::{
	chain_spec::DiskChainSpecLoader, extra_subcommand::NoExtraSubcommand, run_with_custom_cli,
	runtime::DefaultRuntimeResolver, CliConfig as CliConfigT, RunConfig, NODE_VERSION,
};

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
	let config = RunConfig::new(Box::new(DefaultRuntimeResolver), Box::new(DiskChainSpecLoader));
	Ok(run_with_custom_cli::<CliConfig, NoExtraSubcommand>(config)?)
}
