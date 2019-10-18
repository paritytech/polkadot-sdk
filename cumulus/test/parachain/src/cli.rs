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

use parachain_runtime::Block;

pub use substrate_cli::{VersionInfo, IntoExit, error::{self, Result}};
use substrate_cli::{parse_and_prepare, ParseAndPrepare, NoCustom};
use substrate_service::{Roles as ServiceRoles, Configuration};
use sr_primitives::{traits::{Block as BlockT, Header as HeaderT, Hash as HashT}, BuildStorage};
use substrate_client::genesis;
use substrate_primitives::hexdisplay::HexDisplay;

use codec::Encode;

use log::info;

use std::{path::PathBuf, cell::RefCell, sync::Arc};

use structopt::StructOpt;

use futures::{sync::oneshot, future, Future};

/// Sub-commands supported by the collator.
#[derive(Debug, StructOpt, Clone)]
enum SubCommands {
	/// Export the genesis state of the parachain.
	#[structopt(name = "export-genesis-state")]
	ExportGenesisState(ExportGenesisStateCommand),
}

impl substrate_cli::GetLogFilter for SubCommands {
	fn get_log_filter(&self) -> Option<String> { None }
}

/// Command for exporting the genesis state of the parachain
#[derive(Debug, StructOpt, Clone)]
struct ExportGenesisStateCommand {
	/// Output file name or stdout if unspecified.
	#[structopt(parse(from_os_str))]
	pub output: Option<PathBuf>,
}

/// Parse command line arguments into service configuration.
pub fn run<I, T, E>(args: I, exit: E, version: VersionInfo) -> error::Result<()>
	where
		I: IntoIterator<Item = T>,
		T: Into<std::ffi::OsString> + Clone,
		E: IntoExit + Send + 'static,
{
	type Config<T> = Configuration<(), T>;
	match parse_and_prepare::<SubCommands, NoCustom, _>(
		&version,
		"cumulus-test-parachain-collator",
		args,
	) {
		ParseAndPrepare::Run(cmd) => cmd.run(load_spec, exit,
		|exit, _cli_args, _custom_args, mut config: Config<_>| {
			info!("{}", version.name);
			info!("  version {}", config.full_version());
			info!("  by {}, 2019", version.author);
			info!("Chain specification: {}", config.chain_spec.name());
			info!("Node name: {}", config.name);
			info!("Roles: {:?}", config.roles);
			info!("Parachain id: {:?}", crate::PARA_ID);

			// TODO
			let key = Arc::new(substrate_primitives::Pair::from_seed(&[10; 32]));

			// TODO
			config.network.listen_addresses = Vec::new();
			config.chain_spec = chain_spec::get_chain_spec();

			match config.roles {
				ServiceRoles::LIGHT => unimplemented!("Light client not supported!"),
				_ => crate::service::run_collator(config, exit, key, version.clone()),
			}.map_err(|e| format!("{:?}", e))
		}),
		ParseAndPrepare::BuildSpec(cmd) => cmd.run(load_spec),
		ParseAndPrepare::ExportBlocks(cmd) => cmd.run_with_builder(|config: Config<_>|
			Ok(new_full_start!(config).0), load_spec, exit),
		ParseAndPrepare::ImportBlocks(cmd) => cmd.run_with_builder(|config: Config<_>|
			Ok(new_full_start!(config).0), load_spec, exit),
		ParseAndPrepare::PurgeChain(cmd) => cmd.run(load_spec),
		ParseAndPrepare::RevertChain(cmd) => cmd.run_with_builder(|config: Config<_>|
			Ok(new_full_start!(config).0), load_spec),
		ParseAndPrepare::CustomCommand(SubCommands::ExportGenesisState(cmd)) => {
			export_genesis_state(cmd.output)
		}
	}?;

	Ok(())
}

fn load_spec(_: &str) -> std::result::Result<Option<chain_spec::ChainSpec>, String> {
	Ok(Some(chain_spec::get_chain_spec()))
}

/// Export the genesis state of the parachain.
fn export_genesis_state(output: Option<PathBuf>) -> error::Result<()> {
	let storage = chain_spec::get_chain_spec().build_storage()?;

	let child_roots = storage.1.iter().map(|(sk, child_map)| {
		let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
			child_map.clone().into_iter().collect()
		);
		(sk.clone(), state_root.encode())
	});
	let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
		storage.0.clone().into_iter().chain(child_roots).collect()
	);
	let block: Block = genesis::construct_genesis_block(state_root);

	let header_hex = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

	if let Some(output) = output {
		std::fs::write(output, header_hex)?;
	} else {
		println!("{}", header_hex);
	}

	Ok(())
}

// handles ctrl-c
pub struct Exit;
impl IntoExit for Exit {
	type Exit = future::MapErr<oneshot::Receiver<()>, fn(oneshot::Canceled) -> ()>;
	fn into_exit(self) -> Self::Exit {
		// can't use signal directly here because CtrlC takes only `Fn`.
		let (exit_send, exit) = oneshot::channel();

		let exit_send_cell = RefCell::new(Some(exit_send));
		ctrlc::set_handler(move || {
			let exit_send = exit_send_cell.try_borrow_mut().expect("signal handler not reentrant; qed").take();
			if let Some(exit_send) = exit_send {
				exit_send.send(()).expect("Error sending exit notification");
			}
		}).expect("Error setting Ctrl-C handler");

		exit.map_err(drop)
	}
}
