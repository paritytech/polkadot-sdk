// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Key related CLI utilities

use super::{
	generate::GenerateCmd, generate_node_key::GenerateNodeKeyCmd, insert_key::InsertKeyCmd,
	inspect_key::InspectKeyCmd, inspect_node_key::InspectNodeKeyCmd,
};
use crate::{Error, SubstrateCli};

/// Key utilities for the cli.
#[derive(Debug, clap::Subcommand)]
pub enum KeySubcommand {
	/// Generate a random node key, write it to a file or stdout and write the
	/// corresponding peer-id to stderr
	GenerateNodeKey(GenerateNodeKeyCmd),

	/// Generate a random account
	Generate(GenerateCmd),

	/// Gets a public key and a SS58 address from the provided Secret URI
	Inspect(InspectKeyCmd),

	/// Load a node key from a file or stdin and print the corresponding peer-id
	InspectNodeKey(InspectNodeKeyCmd),

	/// Insert a key to the keystore of a node.
	Insert(InsertKeyCmd),
}

impl KeySubcommand {
	/// run the key subcommands
	pub fn run<C: SubstrateCli>(&self, cli: &C) -> Result<(), Error> {
		match self {
			KeySubcommand::GenerateNodeKey(cmd) => {
				let chain_spec = cli.load_spec(cmd.chain.as_deref().unwrap_or(""))?;
				cmd.run(chain_spec.id(), &C::executable_name())
			},
			KeySubcommand::Generate(cmd) => cmd.run(),
			KeySubcommand::Inspect(cmd) => cmd.run(),
			KeySubcommand::Insert(cmd) => cmd.run(cli),
			KeySubcommand::InspectNodeKey(cmd) => cmd.run(),
		}
	}
}
