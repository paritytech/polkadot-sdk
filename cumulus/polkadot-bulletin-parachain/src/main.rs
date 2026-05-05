// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Custom parachain node for the Polkadot Bulletin Chain.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

#[allow(missing_docs)]
mod bulletin_fake_runtime_api;
mod hop_extension;

/// Bulletin-local [`polkadot_omni_node_lib::RuntimeApiBundle`] that uses the
/// `bulletin_fake_runtime_api` types (which implement `sp_hop::HopRuntimeApi`)
/// in all four slots, so the lib's own fake stays HOP-free.
pub struct BulletinRuntimeApiBundle;

impl polkadot_omni_node_lib::RuntimeApiBundle for BulletinRuntimeApiBundle {
	type AuraSr25519U32 = bulletin_fake_runtime_api::aura_sr25519::RuntimeApi;
	type AuraEd25519U32 = bulletin_fake_runtime_api::aura_ed25519::RuntimeApi;
	type AuraSr25519U64 = bulletin_fake_runtime_api::aura_sr25519::RuntimeApi;
	type AuraEd25519U64 = bulletin_fake_runtime_api::aura_ed25519::RuntimeApi;
}

// Keep the `#[global_allocator]` from being dropped by the linker.
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
		2026
	}
}

fn main() -> color_eyre::eyre::Result<()> {
	color_eyre::install()?;

	let cli_command = polkadot_omni_node_lib::cli::Cli::<CliConfig>::command();
	let cli_command = NoExtraSubcommand::augment_subcommands(cli_command);
	let cli_command = polkadot_omni_node_lib::cli::Cli::<CliConfig>::setup_command(cli_command);
	let cli_command = sc_hop::HopParams::augment_args(cli_command);

	let matches = cli_command.get_matches();

	let hop_params = sc_hop::HopParams::from_arg_matches(&matches)
		.expect("HopParams::augment_args was applied to the parser; qed");

	let mut config: RunConfig<BulletinRuntimeApiBundle> =
		RunConfig::new(Box::new(DefaultRuntimeResolver), Box::new(DiskChainSpecLoader));
	config.extensions.aura_sr25519_u32 = vec![Box::new(hop_extension::HopExtension::<
		polkadot_omni_node_lib::BlockU32,
		bulletin_fake_runtime_api::aura_sr25519::RuntimeApi,
	>::new(hop_params))];

	Ok(polkadot_omni_node_lib::run_with_matches::<
		CliConfig,
		NoExtraSubcommand,
		BulletinRuntimeApiBundle,
	>(config, matches)?)
}
