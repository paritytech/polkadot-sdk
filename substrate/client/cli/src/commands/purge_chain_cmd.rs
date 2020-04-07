// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use crate::error;
use crate::params::SharedParams;
use crate::CliConfiguration;
use sc_service::{config::DatabaseConfig, Configuration};
use std::fmt::Debug;
use std::fs;
use std::io::{self, Write};
use structopt::StructOpt;

/// The `purge-chain` command used to remove the whole chain.
#[derive(Debug, StructOpt, Clone)]
pub struct PurgeChainCmd {
	/// Skip interactive prompt by answering yes automatically.
	#[structopt(short = "y")]
	pub yes: bool,

	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub shared_params: SharedParams,
}

impl PurgeChainCmd {
	/// Run the purge command
	pub fn run(&self, config: Configuration) -> error::Result<()> {
		let db_path = match &config.database {
			DatabaseConfig::Path { path, .. } => path,
			_ => {
				eprintln!("Cannot purge custom database implementation");
				return Ok(());
			}
		};

		if !self.yes {
			print!("Are you sure to remove {:?}? [y/N]: ", &db_path);
			io::stdout().flush().expect("failed to flush stdout");

			let mut input = String::new();
			io::stdin().read_line(&mut input)?;
			let input = input.trim();

			match input.chars().nth(0) {
				Some('y') | Some('Y') => {},
				_ => {
					println!("Aborted");
					return Ok(());
				},
			}
		}

		match fs::remove_dir_all(&db_path) {
			Ok(_) => {
				println!("{:?} removed.", &db_path);
				Ok(())
			},
			Err(ref err) if err.kind() == io::ErrorKind::NotFound => {
				eprintln!("{:?} did not exist.", &db_path);
				Ok(())
			},
			Err(err) => Result::Err(err.into()),
		}
	}
}

impl CliConfiguration for PurgeChainCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}
}
