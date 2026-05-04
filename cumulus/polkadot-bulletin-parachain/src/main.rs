// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! PoC custom parachain node for the Polkadot Bulletin Chain.
//!
//! This binary is the proof-of-concept requested in
//! [`paritytech/polkadot-bulletin-chain#479`](https://github.com/paritytech/polkadot-bulletin-chain/issues/479)
//! and the HOP discussion on
//! [`paritytech/polkadot-sdk#11662`](https://github.com/paritytech/polkadot-sdk/pull/11662).
//!
//! The point of the PoC is to **answer one question**: can a Bulletin-specific node binary be
//! built by composing `polkadot-omni-node-lib`, with the HOP wiring driven by the Bulletin
//! binary instead of being baked into the generic `polkadot-omni-node` binary?
//!
//! See `README.md` next to this file for findings and the open questions on lib extensibility.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

// Force the linker to keep the polkadot_jemalloc_shim crate (and its #[global_allocator]).
#[cfg(target_os = "linux")]
extern crate polkadot_jemalloc_shim;

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
		"https://github.com/paritytech/polkadot-bulletin-chain/issues/new".into()
	}

	fn copyright_start_year() -> u16 {
		2024
	}
}

fn main() -> color_eyre::eyre::Result<()> {
	color_eyre::install()?;
	let config = RunConfig::new(Box::new(DefaultRuntimeResolver), Box::new(DiskChainSpecLoader));
	Ok(run_with_custom_cli::<CliConfig, NoExtraSubcommand>(config)?)
}
