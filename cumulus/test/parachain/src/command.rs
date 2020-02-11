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

use std::sync::Arc;

use parachain_runtime::Block;

use sc_cli::{error, VersionInfo};
use sc_client::genesis;
use sc_service::{Configuration, Roles as ServiceRoles};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT},
	BuildStorage,
};
use sc_network::config::TransportConfig;
use polkadot_service::ChainSpec as ChainSpecPolkadot;

use codec::Encode;
use log::info;

const DEFAULT_POLKADOT_RPC_HTTP: &'static str = "127.0.0.1:9934";
const DEFAULT_POLKADOT_RPC_WS: &'static str = "127.0.0.1:9945";
const DEFAULT_POLKADOT_GRAFANA_PORT: &'static str = "127.0.0.1:9956";

/// Parse command line arguments into service configuration.
pub fn run(version: VersionInfo) -> error::Result<()> {
	let opt: Cli = sc_cli::from_args(&version);

	let mut config = sc_service::Configuration::new(&version);
	let mut polkadot_config = Configuration::new(&version);

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
			sc_cli::init(&opt.run.shared_params, &version)?;
			sc_cli::init_config(&mut config, &opt.run.shared_params, &version, load_spec)?;
			sc_cli::update_config_for_running_node(&mut config, opt.run)?;

			info!("{}", version.name);
			info!("  version {}", config.full_version());
			info!("  by {}, 2019", version.author);
			info!("Chain specification: {}", config.expect_chain_spec().name());
			info!("Node name: {}", config.name);
			info!("Roles: {:?}", config.roles);
			info!("Parachain id: {:?}", crate::PARA_ID);

			// TODO
			let key = Arc::new(sp_core::Pair::from_seed(&[10; 32]));

			polkadot_config.config_dir = config.in_chain_config_dir("polkadot");

			let polkadot_opt: PolkadotCli = sc_cli::from_iter(opt.relaychain_args, &version);
			let allow_private_ipv4 = !polkadot_opt.run.network_config.no_private_ipv4;

			polkadot_config.rpc_http = Some(DEFAULT_POLKADOT_RPC_HTTP.parse().unwrap());
			polkadot_config.rpc_ws = Some(DEFAULT_POLKADOT_RPC_WS.parse().unwrap());
			polkadot_config.grafana_port = Some(DEFAULT_POLKADOT_GRAFANA_PORT.parse().unwrap());

			sc_cli::init_config(
				&mut polkadot_config,
				&polkadot_opt.run.shared_params,
				&version,
				load_spec_polkadot,
			)?;
			sc_cli::update_config_for_running_node(&mut polkadot_config, polkadot_opt.run)?;

			// TODO: we disable mdns for the polkadot node because it prevents the process to exit
			//       properly. See https://github.com/paritytech/cumulus/issues/57
			polkadot_config.network.transport = TransportConfig::Normal {
				enable_mdns: false,
				allow_private_ipv4,
				wasm_external_transport: None,
			};

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
