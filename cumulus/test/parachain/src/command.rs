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
use cumulus_primitives::ParaId;
use log::info;
use parachain_runtime::Block;
use polkadot_parachain::primitives::AccountIdConversion;
use sc_cli::{
	ChainSpec, CliConfiguration, Error, ImportParams, KeystoreParams, NetworkParams, Result, Role,
	RuntimeVersion, SharedParams, SubstrateCli,
};
use sc_service::config::{BasePath, PrometheusConfig};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT, Zero},
	BuildStorage,
};
use std::{net::SocketAddr, sync::Arc};

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Cumulus Test Parachain Collator".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Cumulus test parachain collator\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relaychain node.\n\n\
		{} [parachain-args] -- [relaychain-args]",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/cumulus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, _id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		// Such a hack :(
		Ok(Box::new(chain_spec::get_chain_spec(
			self.run.parachain_id.into(),
		)))
	}

	fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		&parachain_runtime::VERSION
	}
}

impl SubstrateCli for PolkadotCli {
	fn impl_name() -> String {
		"Cumulus Test Parachain Collator".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		"Cumulus test parachain collator\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relaychain node.\n\n\
		cumulus-test-parachain-collator [parachain-args] -- [relaychain-args]"
			.into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/paritytech/cumulus/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([PolkadotCli::executable_name().to_string()].iter())
			.load_spec(id)
	}

	fn native_runtime_version(chain_spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		polkadot_cli::Cli::native_runtime_version(chain_spec)
	}
}

/// Generate the genesis block
pub fn generate_genesis_state(para_id: ParaId) -> Result<Block> {
	let storage = (&chain_spec::get_chain_spec(para_id)).build_storage()?;

	let child_roots = storage.children_default.iter().map(|(sk, child_content)| {
		let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
			child_content.data.clone().into_iter().collect(),
		);
		(sk.clone(), state_root.encode())
	});
	let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
		storage.top.clone().into_iter().chain(child_roots).collect(),
	);

	let extrinsics_root =
		<<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(Vec::new());

	Ok(Block::new(
		<<Block as BlockT>::Header as HeaderT>::new(
			Zero::zero(),
			extrinsics_root,
			state_root,
			Default::default(),
			Default::default(),
		),
		Default::default(),
	))
}

/// Parse command line arguments into service configuration.
pub fn run() -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Base(subcommand)) => {
			let runner = cli.create_runner(subcommand)?;

			runner.run_subcommand(subcommand, |config| {
				Ok(new_full_start!(config).0.to_chain_ops_parts())
			})
		}
		Some(Subcommand::ExportGenesisState(params)) => {
			sc_cli::init_logger("");

			let block = generate_genesis_state(params.parachain_id.into())?;
			let header_hex = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

			if let Some(output) = &params.output {
				std::fs::write(output, header_hex)?;
			} else {
				println!("{}", header_hex);
			}

			Ok(())
		}
		None => {
			let runner = cli.create_runner(&*cli.run)?;

			// TODO
			let key = Arc::new(sp_core::Pair::generate().0);

			let mut polkadot_cli = PolkadotCli::from_iter(
				[PolkadotCli::executable_name().to_string()]
					.iter()
					.chain(cli.relaychain_args.iter()),
			);

			let id = ParaId::from(cli.run.parachain_id);

			let parachain_account =
				AccountIdConversion::<polkadot_primitives::AccountId>::into_account(&id);

			let block = generate_genesis_state(id)?;
			let genesis_state = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

			runner.run_node_until_exit(|config| {
				if matches!(config.role, Role::Light) {
					return Err("Light client not supporter!".into());
				}

				polkadot_cli.base_path =
					config.base_path.as_ref().map(|x| x.path().join("polkadot"));

				let task_executor = config.task_executor.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, task_executor)
						.unwrap();

				info!("Parachain id: {:?}", id);
				info!("Parachain Account: {}", parachain_account);
				info!("Parachain genesis state: {}", genesis_state);

				crate::service::run_collator(config, key, polkadot_config, id)
					.map(|x| x.task_manager)
			})
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

	fn base_path(&self) -> Result<Option<BasePath>> {
		Ok(self
			.shared_params()
			.base_path()
			.or_else(|| self.base_path.clone().map(Into::into)))
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
