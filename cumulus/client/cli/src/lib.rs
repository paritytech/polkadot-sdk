// Copyright 2021 Parity Technologies (UK) Ltd.
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

//! Cumulus CLI library.

#![warn(missing_docs)]

use sc_cli;
use std::{
	fs,
	io::{self, Write},
};
use structopt::StructOpt;

/// The `purge-chain` command used to remove the whole chain: the parachain and the relaychain.
#[derive(Debug, StructOpt)]
pub struct PurgeChainCmd {
	/// The base struct of the purge-chain command.
	#[structopt(flatten)]
	pub base: sc_cli::PurgeChainCmd,

	/// Only delete the para chain database
	#[structopt(long, aliases = &["para"])]
	pub parachain: bool,

	/// Only delete the relay chain database
	#[structopt(long, aliases = &["relay"])]
	pub relaychain: bool,
}

impl PurgeChainCmd {
	/// Run the purge command
	pub fn run(
		&self,
		para_config: sc_service::Configuration,
		relay_config: sc_service::Configuration,
	) -> sc_cli::Result<()> {
		let databases = match (self.parachain, self.relaychain) {
			(true, true) | (false, false) => vec![
				("parachain", para_config.database),
				("relaychain", relay_config.database),
			],
			(true, false) => vec![("parachain", para_config.database)],
			(false, true) => vec![("relaychain", relay_config.database)],
		};

		let db_paths = databases
			.iter()
			.map(|(chain_label, database)| {
				database.path().ok_or_else(|| sc_cli::Error::Input(format!(
					"Cannot purge custom database implementation of: {}",
					chain_label,
				)))
			})
			.collect::<sc_cli::Result<Vec<_>>>()?;

		if !self.base.yes {
			for db_path in &db_paths {
				println!("{}", db_path.display());
			}
			print!("Are you sure to remove? [y/N]: ");
			io::stdout().flush().expect("failed to flush stdout");

			let mut input = String::new();
			io::stdin().read_line(&mut input)?;
			let input = input.trim();

			match input.chars().nth(0) {
				Some('y') | Some('Y') => {}
				_ => {
					println!("Aborted");
					return Ok(());
				}
			}
		}

		for db_path in &db_paths {
			match fs::remove_dir_all(&db_path) {
				Ok(_) => {
					println!("{:?} removed.", &db_path);
				}
				Err(ref err) if err.kind() == io::ErrorKind::NotFound => {
					eprintln!("{:?} did not exist.", &db_path);
				}
				Err(err) => return Err(err.into()),
			}
		}

		Ok(())
	}
}

impl sc_cli::CliConfiguration for PurgeChainCmd {
	fn shared_params(&self) -> &sc_cli::SharedParams {
		&self.base.shared_params
	}

	fn database_params(&self) -> Option<&sc_cli::DatabaseParams> {
		Some(&self.base.database_params)
	}
}
