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

use sc_client::genesis;
use sc_service::{Configuration, Role as ServiceRole, config::PrometheusConfig};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT},
	BuildStorage,
};
use sc_network::config::TransportConfig;

use codec::Encode;
use log::info;

const DEFAULT_POLKADOT_RPC_HTTP: &'static str = "127.0.0.1:9934";
const DEFAULT_POLKADOT_RPC_WS: &'static str = "127.0.0.1:9945";
const DEFAULT_POLKADOT_PROMETHEUS_PORT: &'static str = "127.0.0.1:9616";

/// Parse command line arguments into service configuration.
pub fn run(version: sc_cli::VersionInfo) -> sc_cli::Result<()> {
	let opt: Cli = sc_cli::from_args(&version);

	let mut config = sc_service::Configuration::from_version(&version);
	let mut polkadot_config = Configuration::from_version(&version);

	match opt.subcommand {
		Some(Subcommand::Base(subcommand)) => {
			subcommand.init(&version)?;
			subcommand.update_config(&mut config, load_spec, &version)?;
			subcommand.run(
				config,
				|config: Configuration| Ok(new_full_start!(config).0),
			)
		},
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
			opt.run.init(&version)?;
			opt.run.update_config(&mut config, load_spec, &version)?;

			info!("{}", version.name);
			info!("  version {}", config.full_version());
			info!("  by {}, 2019", version.author);
			info!("Chain specification: {}", config.expect_chain_spec().name());
			info!("Node name: {}", config.name);
			info!("Roles: {:?}", config.role);
			info!("Parachain id: {:?}", crate::PARA_ID);

			// TODO
			let key = Arc::new(sp_core::Pair::from_seed(&[10; 32]));

			polkadot_config.config_dir = config.in_chain_config_dir("polkadot");

			let polkadot_opt: PolkadotCli = sc_cli::from_iter(
				[version.executable_name.to_string()].iter().chain(opt.relaychain_args.iter()),
				&version,
			);
			let allow_private_ipv4 = !polkadot_opt.run.base.network_config.no_private_ipv4;

			polkadot_config.rpc_http = Some(DEFAULT_POLKADOT_RPC_HTTP.parse().unwrap());
			polkadot_config.rpc_ws = Some(DEFAULT_POLKADOT_RPC_WS.parse().unwrap());
			polkadot_config.prometheus_config = Some(
				PrometheusConfig::new_with_default_registry(
					DEFAULT_POLKADOT_PROMETHEUS_PORT.parse().unwrap(),
				)
			);

			polkadot_opt.run.base.update_config(
				&mut polkadot_config,
				load_spec_polkadot,
				&version,
			)?;

			// TODO: we disable mdns for the polkadot node because it prevents the process to exit
			//       properly. See https://github.com/paritytech/cumulus/issues/57
			polkadot_config.network.transport = TransportConfig::Normal {
				enable_mdns: false,
				allow_private_ipv4,
				wasm_external_transport: None,
				use_yamux_flow_control: false,
			};

			match config.role {
				ServiceRole::Light => unimplemented!("Light client not supported!"),
				_ => crate::service::run_collator(config, key, polkadot_config),
			}
		},
	}
}

fn load_spec(_: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
	Ok(Box::new(chain_spec::get_chain_spec()))
}

fn load_spec_polkadot(_: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
	polkadot_service::PolkadotChainSpec::from_json_bytes(
		&include_bytes!("../res/polkadot_chainspec.json")[..],
	).map(|r| Box::new(r) as Box<_>)
}
