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

use clap::{Args, CommandFactory, FromArgMatches, Subcommand};
use polkadot_omni_node_lib::{
	chain_spec::DiskChainSpecLoader, extra_subcommand::NoExtraSubcommand,
	runtime::DefaultRuntimeResolver, CliConfig as CliConfigT, RunConfig, NODE_VERSION,
};
use sc_cli::SubstrateCli;

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

	// Build the parser ourselves so we can flatten in `sc_hop::HopParams`
	// alongside the lib's `Cli<CliConfig>`. Then dispatch via the lib's
	// `run_with_matches`, which expects a pre-parsed `ArgMatches`.
	let cli_command = polkadot_omni_node_lib::cli::Cli::<CliConfig>::command();
	let cli_command = NoExtraSubcommand::augment_subcommands(cli_command);
	let cli_command = polkadot_omni_node_lib::cli::Cli::<CliConfig>::setup_command(cli_command);
	let cli_command = sc_hop::HopParams::augment_args(cli_command);

	let matches = cli_command.get_matches();

	let hop_params = sc_hop::HopParams::from_arg_matches(&matches)
		.expect("HopParams::augment_args was applied to the parser; qed");
	let extension_factory = hop_extension::HopExtensionFactory::new(hop_params);

	let config = RunConfig::with_extension_factory(
		Box::new(DefaultRuntimeResolver),
		Box::new(DiskChainSpecLoader),
		Box::new(extension_factory),
	);

	Ok(polkadot_omni_node_lib::run_with_matches::<CliConfig, NoExtraSubcommand>(
		config, matches,
	)?)
}
