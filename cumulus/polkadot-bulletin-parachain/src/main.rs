// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! PoC custom parachain node for the Polkadot Bulletin Chain.
//!
//! This binary is the proof-of-concept requested in
//! [`paritytech/polkadot-bulletin-chain#479`](https://github.com/paritytech/polkadot-bulletin-chain/issues/479)
//! and the HOP discussion on
//! [`paritytech/polkadot-sdk#11662`](https://github.com/paritytech/polkadot-sdk/pull/11662).
//!
//! After Pass 2 (this commit), HOP is wired entirely from the Bulletin side via the lib's
//! [`polkadot_omni_node_lib::NodeExtensionFactory`] hook. The lib stays HOP-free.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod hop_extension;

// Force the linker to keep the polkadot_jemalloc_shim crate (and its #[global_allocator]).
#[cfg(target_os = "linux")]
extern crate polkadot_jemalloc_shim;

use clap::Parser;
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

	// Pass 2 HOP wiring: construct `HopParams` and register the Bulletin-side
	// extension factory on `RunConfig`. The lib will pull the extension out of
	// the factory at the right point in startup and call its `on_start` /
	// `build_rpc_extension` hooks.
	//
	// TODO(pass-3): expose HOP CLI flags via a Bulletin-owned `Cli` that
	// flattens `sc_hop::HopParams` alongside `polkadot_omni_node_lib::Cli`.
	// Until then the binary uses HOP defaults plus `--enable-hop=true` so the
	// extension is exercised end-to-end on dev runs.
	let hop_params = sc_hop::HopParams::parse_from([
		"polkadot-bulletin-parachain",
		"--enable-hop",
	]);
	let extension_factory = hop_extension::HopExtensionFactory::new(hop_params);

	let config = RunConfig::with_extension_factory(
		Box::new(DefaultRuntimeResolver),
		Box::new(DiskChainSpecLoader),
		Box::new(extension_factory),
	);
	Ok(run_with_custom_cli::<CliConfig, NoExtraSubcommand>(config)?)
}
