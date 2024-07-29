// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implementation of the `generate-node-key` subcommand

use crate::{build_network_key_dir_or_default, Error, NODE_KEY_ED25519_FILE};
use clap::{Args, Parser};
use libp2p_identity::{ed25519, Keypair};
use sc_service::BasePath;
use std::{
	fs,
	io::{self, Write},
	path::PathBuf,
};

/// Common arguments accross all generate key commands, subkey and node.
#[derive(Debug, Args, Clone)]
pub struct GenerateKeyCmdCommon {
	/// Name of file to save secret key to.
	/// If not given, the secret key is printed to stdout.
	#[arg(long)]
	file: Option<PathBuf>,

	/// The output is in raw binary format.
	/// If not given, the output is written as an hex encoded string.
	#[arg(long)]
	bin: bool,
}

/// The `generate-node-key` command
#[derive(Debug, Clone, Parser)]
#[command(
	name = "generate-node-key",
	about = "Generate a random node key, write it to a file or stdout \
		 	and write the corresponding peer-id to stderr"
)]
pub struct GenerateNodeKeyCmd {
	#[clap(flatten)]
	pub common: GenerateKeyCmdCommon,
	/// Specify the chain specification.
	///
	/// It can be any of the predefined chains like dev, local, staging, polkadot, kusama.
	#[arg(long, value_name = "CHAIN_SPEC")]
	pub chain: Option<String>,
	/// A directory where the key should be saved. If a key already
	/// exists in the directory, it won't be overwritten.
	#[arg(long, conflicts_with_all = ["file", "default_base_path"])]
	base_path: Option<PathBuf>,

	/// Save the key in the default directory. If a key already
	/// exists in the directory, it won't be overwritten.
	#[arg(long, conflicts_with_all = ["base_path", "file"])]
	default_base_path: bool,
}

impl GenerateKeyCmdCommon {
	/// Run the command
	pub fn run(&self) -> Result<(), Error> {
		generate_key(&self.file, self.bin, None, &None, false, None)
	}
}

impl GenerateNodeKeyCmd {
	/// Run the command
	pub fn run(&self, chain_spec_id: &str, executable_name: &String) -> Result<(), Error> {
		generate_key(
			&self.common.file,
			self.common.bin,
			Some(chain_spec_id),
			&self.base_path,
			self.default_base_path,
			Some(executable_name),
		)
	}
}

// Utility function for generating a key based on the provided CLI arguments
//
// `file`  - Name of file to save secret key to
// `bin`
fn generate_key(
	file: &Option<PathBuf>,
	bin: bool,
	chain_spec_id: Option<&str>,
	base_path: &Option<PathBuf>,
	default_base_path: bool,
	executable_name: Option<&String>,
) -> Result<(), Error> {
	let keypair = ed25519::Keypair::generate();

	let secret = keypair.secret();

	let file_data = if bin {
		secret.as_ref().to_owned()
	} else {
		array_bytes::bytes2hex("", secret).into_bytes()
	};

	match (file, base_path, default_base_path) {
		(Some(file), None, false) => fs::write(file, file_data)?,
		(None, Some(_), false) | (None, None, true) => {
			let network_path = build_network_key_dir_or_default(
				base_path.clone().map(BasePath::new),
				chain_spec_id.unwrap_or_default(),
				executable_name.ok_or(Error::Input("Executable name not provided".into()))?,
			);

			fs::create_dir_all(network_path.as_path())?;

			let key_path = network_path.join(NODE_KEY_ED25519_FILE);
			if key_path.exists() {
				eprintln!("Skip generation, a key already exists in {:?}", key_path);
				return Err(Error::KeyAlreadyExistsInPath(key_path));
			} else {
				eprintln!("Generating key in {:?}", key_path);
				fs::write(key_path, file_data)?
			}
		},
		(None, None, false) => io::stdout().lock().write_all(&file_data)?,
		(_, _, _) => {
			// This should not happen, arguments are marked as mutually exclusive.
			return Err(Error::Input("Mutually exclusive arguments provided".into()));
		},
	}

	eprintln!("{}", Keypair::from(keypair).public().to_peer_id());

	Ok(())
}

#[cfg(test)]
pub mod tests {
	use crate::DEFAULT_NETWORK_CONFIG_PATH;

	use super::*;
	use std::io::Read;
	use tempfile::Builder;

	#[test]
	fn generate_node_key() {
		let mut file = Builder::new().prefix("keyfile").tempfile().unwrap();
		let file_path = file.path().display().to_string();
		let generate = GenerateNodeKeyCmd::parse_from(&["generate-node-key", "--file", &file_path]);
		assert!(generate.run("test", &String::from("test")).is_ok());
		let mut buf = String::new();
		assert!(file.read_to_string(&mut buf).is_ok());
		assert!(array_bytes::hex2bytes(&buf).is_ok());
	}

	#[test]
	fn generate_node_key_base_path() {
		let base_dir = Builder::new().prefix("keyfile").tempdir().unwrap();
		let key_path = base_dir
			.path()
			.join("chains/test_id/")
			.join(DEFAULT_NETWORK_CONFIG_PATH)
			.join(NODE_KEY_ED25519_FILE);
		let base_path = base_dir.path().display().to_string();
		let generate =
			GenerateNodeKeyCmd::parse_from(&["generate-node-key", "--base-path", &base_path]);
		assert!(generate.run("test_id", &String::from("test")).is_ok());
		let buf = fs::read_to_string(key_path.as_path()).unwrap();
		assert!(array_bytes::hex2bytes(&buf).is_ok());

		assert!(generate.run("test_id", &String::from("test")).is_err());
		let new_buf = fs::read_to_string(key_path).unwrap();
		assert_eq!(
			array_bytes::hex2bytes(&new_buf).unwrap(),
			array_bytes::hex2bytes(&buf).unwrap()
		);
	}
}
