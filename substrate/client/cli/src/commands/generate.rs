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

//! Implementation of the `generate` subcommand
use crate::{
	utils::print_from_uri, with_crypto_scheme, CryptoSchemeFlag, Error, KeystoreParams,
	NetworkSchemeFlag, OutputTypeFlag,
};
use bip39::Mnemonic;
use clap::Parser;
use itertools::Itertools;

/// The `generate` command
#[derive(Debug, Clone, Parser)]
#[command(name = "generate", about = "Generate a random account")]
pub struct GenerateCmd {
	/// The number of words in the phrase to generate. One of 12 (default), 15, 18, 21 and 24.
	#[arg(short = 'w', long, value_name = "WORDS")]
	words: Option<usize>,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub keystore_params: KeystoreParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub network_scheme: NetworkSchemeFlag,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub output_scheme: OutputTypeFlag,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub crypto_scheme: CryptoSchemeFlag,
}

impl GenerateCmd {
	/// Run the command
	pub fn run(&self) -> Result<(), Error> {
		let words = match self.words {
			Some(words_count) if [12, 15, 18, 21, 24].contains(&words_count) => Ok(words_count),
			Some(_) => Err(Error::Input(
				"Invalid number of words given for phrase: must be 12/15/18/21/24".into(),
			)),
			None => Ok(12),
		}?;
		let mnemonic = Mnemonic::generate(words)
			.map_err(|e| Error::Input(format!("Mnemonic generation failed: {e}").into()))?;
		let password = self.keystore_params.read_password()?;
		let output = self.output_scheme.output_type;

		let phrase = mnemonic.words().join(" ");

		with_crypto_scheme!(
			self.crypto_scheme.scheme,
			print_from_uri(&phrase, password, self.network_scheme.network, output)
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn generate() {
		let generate = GenerateCmd::parse_from(&["generate", "--password", "12345"]);
		assert!(generate.run().is_ok())
	}
}
