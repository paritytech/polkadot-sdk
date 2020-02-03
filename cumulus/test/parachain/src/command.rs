// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::chain_spec;
use crate::cli::{Cli, PolkadotCli, Subcommand};

use std::{path::PathBuf, sync::Arc};
use futures::{future::Map, FutureExt};

use parachain_runtime::Block;

use sc_cli::{error::{self, Result}, VersionInfo};
use sc_client::genesis;
use sc_service::{Configuration, Roles as ServiceRoles};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT},
	BuildStorage,
};
use polkadot_service::ChainSpec as ChainSpecPolkadot;

use codec::Encode;
use log::info;
use structopt::StructOpt;

/// Parse command line arguments into service configuration.
pub fn run(version: VersionInfo) -> error::Result<()> {
	let opt: Cli = sc_cli::from_args(&version);

	let mut config = sc_service::Configuration::default();
	config.impl_name = "cumulus-test-parachain-collator";

	match opt.subcommand {
		Some(Subcommand::Base(subcommand)) => sc_cli::run_subcommand(
			config,
			subcommand,
			load_spec,
			|config: Configuration<_, _>| Ok(new_full_start!(config).0),
			&version,
		),
		Some(Subcommand::ExportGenesisState(params)) => {
			sc_cli::init_logger("");

			let storage = (&chain_spec::get_chain_spec()).build_storage()?;

			let child_roots = storage.children.iter().map(|(sk, child_content)| {
				let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
					child_content.data.clone().into_iter().collect(),
				);
				(sk.clone(), state_root.encode())
			});
			let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
				storage.top.clone().into_iter().chain(child_roots).collect(),
			);
			let block: Block = genesis::construct_genesis_block(state_root);

			let header_hex = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

			if let Some(output) = params.output {
				std::fs::write(output, header_hex)?;
			} else {
				println!("{}", header_hex);
			}

			Ok(())
		},
		None => {
			sc_cli::init(&mut config, load_spec, &opt.run.shared_params, &version)?;

			info!("{}", version.name);
			info!("  version {}", config.full_version());
			info!("  by {}, 2019", version.author);
			info!("Chain specification: {}", config.expect_chain_spec().name());
			info!("Node name: {}", config.name);
			info!("Roles: {:?}", config.roles);
			info!("Parachain id: {:?}", crate::PARA_ID);

			// TODO
			let key = Arc::new(sp_core::Pair::from_seed(&[10; 32]));

			let mut polkadot_config = Configuration::default();
			polkadot_config.impl_name = "cumulus-test-parachain-collator";
			polkadot_config.config_dir = config.in_chain_config_dir("polkadot");

			// TODO: parse_address is private
			/*
			let rpc_interface: &str = interface_str(opt.run.rpc_external, opt.run.unsafe_rpc_external, opt.run.validator)?;
			config.rpc_http = Some(parse_address(&format!("{}:{}", rpc_interface, 9934), opt.run.rpc_port)?);
			let ws_interface: &str = interface_str(opt.run.ws_external, opt.run.unsafe_ws_external, opt.run.validator)?;
			config.rpc_ws = Some(parse_address(&format!("{}:{}", ws_interface, 9945), opt.run.ws_port)?);
			let grafana_interface: &str = if opt.run.grafana_external { "0.0.0.0" } else { "127.0.0.1" };
			config.grafana_port = Some(
				parse_address(&format!("{}:{}", grafana_interface, 9956), opt.run.grafana_port)?
			);
			*/

			let polkadot_opt: PolkadotCli = sc_cli::from_iter(opt.relaychain_args, &version);

			// TODO
			polkadot_config.chain_spec = Some(sc_cli::load_spec(&polkadot_opt.run.shared_params, load_spec_polkadot)?);
			// TODO: base_path is private
			//polkadot_config.config_dir = Some(sc_cli::base_path(&polkadot_opt.run.shared_params, &version));
			polkadot_config.impl_commit = version.commit;
			polkadot_config.impl_version = version.version;

			// TODO
			if let Some(ref config_dir) = polkadot_config.config_dir {
				polkadot_config.database = sc_service::config::DatabaseConfig::Path {
					cache_size: Default::default(),
					path: config_dir.join("db"),
				};
			}
			// TODO
			polkadot_config.network.boot_nodes = polkadot_config.network.boot_nodes.clone();
			polkadot_config.telemetry_endpoints = polkadot_config.expect_chain_spec().telemetry_endpoints().clone();

			sc_cli::update_config_for_running_node(&mut polkadot_config, polkadot_opt.run);

			match config.roles {
				ServiceRoles::LIGHT => unimplemented!("Light client not supported!"),
				_ => crate::service::run_collator(config, key, polkadot_config),
			}
		},
	}
}

fn load_spec(_: &str) -> std::result::Result<Option<chain_spec::ChainSpec>, String> {
	Ok(Some(chain_spec::get_chain_spec()))
}

fn load_spec_polkadot(_: &str) -> std::result::Result<Option<ChainSpecPolkadot>, String> {
	Some(polkadot_service::ChainSpec::from_json_bytes(
		&include_bytes!("../res/polkadot_chainspec.json")[..],
	)).transpose()
}
