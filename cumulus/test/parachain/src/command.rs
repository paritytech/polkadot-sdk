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

use crate::{
	chain_spec,
	cli::{Cli, RelayChainCli, Subcommand},
};
use codec::Encode;
use cumulus_primitives::ParaId;
use log::info;
use parachain_runtime::Block;
use polkadot_parachain::primitives::AccountIdConversion;
use sc_cli::{
	ChainSpec, CliConfiguration, Error, ImportParams, KeystoreParams, NetworkParams, Result,
	RuntimeVersion, SharedParams, SubstrateCli,
};
use sc_service::config::{BasePath, PrometheusConfig};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::traits::{Block as BlockT, Hash as HashT, Header as HeaderT, Zero};
use std::{io::Write, net::SocketAddr, sync::Arc};

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

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		match id {
			"staging" => Ok(Box::new(chain_spec::staging_test_net(
				self.run.parachain_id.unwrap_or(100).into(),
			))),
			"tick" => Ok(Box::new(chain_spec::ChainSpec::from_json_bytes(
				&include_bytes!("../res/tick.json")[..],
			)?)),
			"trick" => Ok(Box::new(chain_spec::ChainSpec::from_json_bytes(
				&include_bytes!("../res/trick.json")[..],
			)?)),
			"track" => Ok(Box::new(chain_spec::ChainSpec::from_json_bytes(
				&include_bytes!("../res/track.json")[..],
			)?)),
			"" => Ok(Box::new(chain_spec::get_chain_spec(
				self.run.parachain_id.unwrap_or(100).into(),
			))),
			path => Ok(Box::new(chain_spec::ChainSpec::from_json_file(
				path.into(),
			)?)),
		}
	}

	fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		&parachain_runtime::VERSION
	}
}

impl SubstrateCli for RelayChainCli {
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
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name().to_string()].iter())
			.load_spec(id)
	}

	fn native_runtime_version(chain_spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		polkadot_cli::Cli::native_runtime_version(chain_spec)
	}
}

pub fn generate_genesis_state(chain_spec: &Box<dyn sc_service::ChainSpec>) -> Result<Block> {
	let storage = chain_spec.build_storage()?;

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

fn extract_genesis_wasm(chain_spec: &Box<dyn sc_service::ChainSpec>) -> Result<Vec<u8>> {
	let mut storage = chain_spec.build_storage()?;

	storage
		.top
		.remove(sp_core::storage::well_known_keys::CODE)
		.ok_or_else(|| "Could not find wasm file in genesis state!".into())
}

/// Parse command line arguments into service configuration.
pub fn run() -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Base(subcommand)) => {
			let runner = cli.create_runner(subcommand)?;

			runner.run_subcommand(subcommand, |mut config| {
				let params = crate::service::new_partial(&mut config)?;

				Ok((
					params.client,
					params.backend,
					params.import_queue,
					params.task_manager,
				))
			})
		}
		Some(Subcommand::ExportGenesisState(params)) => {
			sc_cli::init_logger("");

			let block =
				generate_genesis_state(&cli.load_spec(&params.chain.clone().unwrap_or_default())?)?;
			let header_hex = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

			if let Some(output) = &params.output {
				std::fs::write(output, header_hex)?;
			} else {
				println!("{}", header_hex);
			}

			Ok(())
		}
		Some(Subcommand::ExportGenesisWasm(params)) => {
			sc_cli::init_logger("");

			let wasm_file =
				extract_genesis_wasm(&cli.load_spec(&params.chain.clone().unwrap_or_default())?)?;

			if let Some(output) = &params.output {
				std::fs::write(output, wasm_file)?;
			} else {
				std::io::stdout().write_all(&wasm_file)?;
			}

			Ok(())
		}
		None => {
			let runner = cli.create_runner(&*cli.run)?;

			runner.run_node_until_exit(|config| {
				// TODO
				let key = Arc::new(sp_core::Pair::generate().0);

				let extension = chain_spec::Extensions::try_get(&config.chain_spec);
				let relay_chain_id = extension.map(|e| e.relay_chain.clone());
				let para_id = extension.map(|e| e.para_id);

				let polkadot_cli = RelayChainCli::new(
					config.base_path.as_ref().map(|x| x.path().join("polkadot")),
					relay_chain_id,
					[RelayChainCli::executable_name().to_string()]
						.iter()
						.chain(cli.relaychain_args.iter()),
				);

				let id = ParaId::from(cli.run.parachain_id.or(para_id).unwrap_or(100));

				let parachain_account =
					AccountIdConversion::<polkadot_primitives::v0::AccountId>::into_account(&id);

				let block =
					generate_genesis_state(&config.chain_spec).map_err(|e| format!("{:?}", e))?;
				let genesis_state = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

				let task_executor = config.task_executor.clone();
				let polkadot_config =
					SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, task_executor)
						.map_err(|err| format!("Relay chain argument error: {}", err))?;

				info!("Parachain id: {:?}", id);
				info!("Parachain Account: {}", parachain_account);
				info!("Parachain genesis state: {}", genesis_state);
				info!(
					"Is collating: {}",
					if cli.run.base.validator { "yes" } else { "no" }
				);

				crate::service::run_collator(
					config,
					key,
					polkadot_config,
					id,
					cli.run.base.validator,
				)
				.map(|(x, _)| x)
			})
		}
	}
}

impl CliConfiguration for RelayChainCli {
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

	fn rpc_ipc(&self) -> Result<Option<String>> {
		self.base.base.rpc_ipc()
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

	fn chain_id(&self, is_dev: bool) -> Result<String> {
		let chain_id = self.base.base.chain_id(is_dev)?;

		Ok(if chain_id.is_empty() {
			self.chain_id.clone().unwrap_or_default()
		} else {
			chain_id
		})
	}

	fn role(&self, is_dev: bool) -> Result<sc_service::Role> {
		self.base.base.role(is_dev)
	}

	fn transaction_pool(&self) -> Result<sc_service::config::TransactionPoolOptions> {
		self.base.base.transaction_pool()
	}

	fn state_cache_child_ratio(&self) -> Result<Option<usize>> {
		self.base.base.state_cache_child_ratio()
	}

	fn rpc_methods(&self) -> Result<sc_service::config::RpcMethods> {
		self.base.base.rpc_methods()
	}

	fn rpc_ws_max_connections(&self) -> Result<Option<usize>> {
		self.base.base.rpc_ws_max_connections()
	}

	fn rpc_cors(&self, is_dev: bool) -> Result<Option<Vec<String>>> {
		self.base.base.rpc_cors(is_dev)
	}

	fn telemetry_external_transport(&self) -> Result<Option<sc_service::config::ExtTransport>> {
		self.base.base.telemetry_external_transport()
	}

	fn default_heap_pages(&self) -> Result<Option<u64>> {
		self.base.base.default_heap_pages()
	}

	fn force_authoring(&self) -> Result<bool> {
		self.base.base.force_authoring()
	}

	fn disable_grandpa(&self) -> Result<bool> {
		self.base.base.disable_grandpa()
	}

	fn max_runtime_instances(&self) -> Result<Option<usize>> {
		self.base.base.max_runtime_instances()
	}

	fn announce_block(&self) -> Result<bool> {
		self.base.base.announce_block()
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
