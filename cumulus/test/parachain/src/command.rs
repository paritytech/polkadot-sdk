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
use codec::Encode;
use log::info;
use parachain_runtime::Block;
use sc_cli::{
	CliConfiguration, Error, ImportParams, KeystoreParams, NetworkParams, Result, SharedParams,
	SubstrateCli,
};
use sc_service::client::genesis;
use sc_network::config::TransportConfig;
use sc_service::config::{NetworkConfiguration, NodeKeyConfig, PrometheusConfig};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT},
	BuildStorage,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

impl SubstrateCli for Cli {
	fn impl_name() -> &'static str {
		"Cumulus Test Parachain Collator"
	}

	fn impl_version() -> &'static str {
		env!("SUBSTRATE_CLI_IMPL_VERSION")
	}

	fn description() -> &'static str {
		"Cumulus test parachain collator\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relaychain node.\n\n\
		cumulus-test-parachain-collator [parachain-args] -- [relaychain-args]"
	}

	fn author() -> &'static str {
		env!("CARGO_PKG_AUTHORS")
	}

	fn support_url() -> &'static str {
		"https://github.com/paritytech/cumulus/issues/new"
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn executable_name() -> &'static str {
		"cumulus-test-parachain-collator"
	}

	fn load_spec(&self, _id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(Box::new(chain_spec::get_chain_spec()))
	}
}

impl SubstrateCli for PolkadotCli {
	fn impl_name() -> &'static str {
		"Cumulus Test Parachain Collator"
	}

	fn impl_version() -> &'static str {
		env!("SUBSTRATE_CLI_IMPL_VERSION")
	}

	fn description() -> &'static str {
		"Cumulus test parachain collator\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relaychain node.\n\n\
		cumulus-test-parachain-collator [parachain-args] -- [relaychain-args]"
	}

	fn author() -> &'static str {
		env!("CARGO_PKG_AUTHORS")
	}

	fn support_url() -> &'static str {
		"https://github.com/paritytech/cumulus/issues/new"
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn executable_name() -> &'static str {
		"cumulus-test-parachain-collator"
	}

	fn load_spec(&self, _id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		polkadot_service::PolkadotChainSpec::from_json_bytes(
			&include_bytes!("../res/polkadot_chainspec.json")[..],
		)
		.map(|r| Box::new(r) as Box<_>)
	}
}

/// Parse command line arguments into service configuration.
pub fn run() -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Base(subcommand)) => {
			let runner = cli.create_runner(subcommand)?;

			runner.run_subcommand(subcommand, |config| Ok(new_full_start!(config).0))
		}
		Some(Subcommand::ExportGenesisState(params)) => {
			sc_cli::init_logger("");

			let storage = (&chain_spec::get_chain_spec()).build_storage()?;

			let child_roots = storage.children_default.iter().map(|(sk, child_content)| {
				let state_root =
					<<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
						child_content.data.clone().into_iter().collect(),
					);
				(sk.clone(), state_root.encode())
			});
			let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
				storage.top.clone().into_iter().chain(child_roots).collect(),
			);
			let block: Block = genesis::construct_genesis_block(state_root);

			let header_hex = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

			if let Some(output) = &params.output {
				std::fs::write(output, header_hex)?;
			} else {
				println!("{}", header_hex);
			}

			Ok(())
		}
		None => {
			let runner = cli.create_runner(&cli.run)?;

			// TODO
			let key = Arc::new(sp_core::Pair::from_seed(&[10; 32]));
			let key2 = Arc::new(sp_core::Pair::from_seed(&[10; 32]));

			let mut polkadot_cli = PolkadotCli::from_iter(
				[PolkadotCli::executable_name().to_string()]
					.iter()
					.chain(cli.relaychain_args.iter()),
			);

			polkadot_cli.base_path = cli.run.base_path()?.map(|x| x.join("polkadot"));

			runner.run_node(
				|config| {
					let task_executor = config.task_executor.clone();
					let polkadot_config = SubstrateCli::create_configuration(
						&polkadot_cli,
						&polkadot_cli,
						task_executor,
					)
					.unwrap();

					info!("Parachain id: {:?}", crate::PARA_ID);

					crate::service::run_collator(config, key2, polkadot_config)
				},
				|config| {
					let task_executor = config.task_executor.clone();
					let polkadot_config = SubstrateCli::create_configuration(
						&polkadot_cli,
						&polkadot_cli,
						task_executor,
					)
					.unwrap();

					info!("Parachain id: {:?}", crate::PARA_ID);

					crate::service::run_collator(config, key, polkadot_config)
				},
				parachain_runtime::VERSION,
			)
		}
	}
}

impl CliConfiguration for PolkadotCli {
	fn shared_params(&self) -> &SharedParams {
		self.base.base.shared_params()
	}

	fn import_params(&self) -> Option<&ImportParams> {
		self.base.base.import_params()
	}

	fn network_params(&self) -> Option<&NetworkParams> {
		self.base.base.network_params()
	}

	fn keystore_params(&self) -> Option<&KeystoreParams> {
		self.base.base.keystore_params()
	}

	fn base_path(&self) -> Result<Option<PathBuf>> {
		Ok(self.shared_params().base_path().or(self.base_path.clone()))
	}

	fn rpc_http(&self) -> Result<Option<SocketAddr>> {
		let rpc_external = self.base.base.rpc_external;
		let unsafe_rpc_external = self.base.base.unsafe_rpc_external;
		let validator = self.base.base.validator;
		let rpc_port = self.base.base.rpc_port;
		// copied directly from substrate
		let rpc_interface: &str = interface_str(rpc_external, unsafe_rpc_external, validator)?;

		Ok(Some(parse_address(
			&format!("{}:{}", rpc_interface, 9934),
			rpc_port,
		)?))
	}

	fn rpc_ws(&self) -> Result<Option<SocketAddr>> {
		let ws_external = self.base.base.ws_external;
		let unsafe_ws_external = self.base.base.unsafe_ws_external;
		let validator = self.base.base.validator;
		let ws_port = self.base.base.ws_port;
		// copied directly from substrate
		let ws_interface: &str = interface_str(ws_external, unsafe_ws_external, validator)?;

		Ok(Some(parse_address(
			&format!("{}:{}", ws_interface, 9945),
			ws_port,
		)?))
	}

	fn prometheus_config(&self) -> Result<Option<PrometheusConfig>> {
		let no_prometheus = self.base.base.no_prometheus;
		let prometheus_external = self.base.base.prometheus_external;
		let prometheus_port = self.base.base.prometheus_port;

		if no_prometheus {
			Ok(None)
		} else {
			let prometheus_interface: &str = if prometheus_external {
				"0.0.0.0"
			} else {
				"127.0.0.1"
			};

			Ok(Some(PrometheusConfig::new_with_default_registry(
				parse_address(
					&format!("{}:{}", prometheus_interface, 9616),
					prometheus_port,
				)?,
			)))
		}
	}

	// TODO: we disable mdns for the polkadot node because it prevents the process to exit
	//       properly. See https://github.com/paritytech/cumulus/issues/57
	fn network_config(
		&self,
		chain_spec: &Box<dyn sc_service::ChainSpec>,
		is_dev: bool,
		net_config_dir: PathBuf,
		client_id: &str,
		node_name: &str,
		node_key: NodeKeyConfig,
	) -> Result<NetworkConfiguration> {
		let (mut network, allow_private_ipv4) = self
			.network_params()
			.map(|x| {
				(
					x.network_config(
						chain_spec,
						is_dev,
						Some(net_config_dir),
						client_id,
						node_name,
						node_key,
					),
					!x.no_private_ipv4,
				)
			})
			.expect("NetworkParams is always available on RunCmd; qed");

		network.transport = TransportConfig::Normal {
			enable_mdns: false,
			allow_private_ipv4,
			wasm_external_transport: None,
			use_yamux_flow_control: false,
		};

		Ok(network)
	}

	fn init<C: SubstrateCli>(&self) -> Result<()> {
		unreachable!("PolkadotCli is never initialized; qed");
	}
}

// copied directly from substrate
fn parse_address(address: &str, port: Option<u16>) -> std::result::Result<SocketAddr, String> {
	let mut address: SocketAddr = address
		.parse()
		.map_err(|_| format!("Invalid address: {}", address))?;
	if let Some(port) = port {
		address.set_port(port);
	}

	Ok(address)
}

// copied directly from substrate
fn interface_str(
	is_external: bool,
	is_unsafe_external: bool,
	is_validator: bool,
) -> Result<&'static str> {
	if is_external && is_validator {
		return Err(Error::Input(
			"--rpc-external and --ws-external options shouldn't be \
		used if the node is running as a validator. Use `--unsafe-rpc-external` if you understand \
		the risks. See the options description for more information."
				.to_owned(),
		));
	}

	if is_external || is_unsafe_external {
		log::warn!(
			"It isn't safe to expose RPC publicly without a proxy server that filters \
		available set of RPC methods."
		);

		Ok("0.0.0.0")
	} else {
		Ok("127.0.0.1")
	}
}
