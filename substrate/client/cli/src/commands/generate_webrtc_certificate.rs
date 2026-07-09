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

//! Implementation of the `generate-webrtc-certificate` subcommand

use crate::{build_network_key_dir_or_default, Error};
use clap::Parser;
use sc_network::webrtc::{
	certhash, encode_certificate, generate_certificate, WEBRTC_CERTIFICATE_FILE,
};
use sc_service::BasePath;
use std::{
	fs,
	io::{self, Write},
	path::PathBuf,
};

/// The `generate-webrtc-certificate` command
#[derive(Debug, Clone, Parser)]
#[command(
	name = "generate-webrtc-certificate",
	about = "Generate a WebRTC DTLS certificate, write it to a file or stdout \
		 	and write the corresponding certhash to stderr"
)]
pub struct GenerateWebRtcCertificateCmd {
	/// Name of file to save the certificate to.
	/// If not given, the certificate is printed to stdout.
	#[arg(long)]
	file: Option<PathBuf>,

	/// The output is in raw binary format.
	/// If not given, the output is written as an hex encoded string.
	#[arg(long)]
	bin: bool,

	/// Specify the chain specification.
	///
	/// It can be any of the predefined chains like dev, local, staging, polkadot, kusama.
	#[arg(long, value_name = "CHAIN_SPEC")]
	pub chain: Option<String>,

	/// A directory where the certificate should be saved. If a certificate already
	/// exists in the directory, it won't be overwritten.
	#[arg(long, conflicts_with_all = ["file", "default_base_path"])]
	base_path: Option<PathBuf>,

	/// Save the certificate in the default directory. If a certificate already
	/// exists in the directory, it won't be overwritten.
	#[arg(long, conflicts_with_all = ["base_path", "file"])]
	default_base_path: bool,
}

impl GenerateWebRtcCertificateCmd {
	/// Run the command
	pub fn run(&self, chain_spec_id: &str, executable_name: &String) -> Result<(), Error> {
		let certificate = generate_certificate().map_err(Error::Input)?;

		let encoded = encode_certificate(&certificate);
		let file_data =
			if self.bin { encoded } else { array_bytes::bytes2hex("", &encoded).into_bytes() };

		match (&self.file, &self.base_path, self.default_base_path) {
			(Some(file), None, false) => fs::write(file, file_data)?,
			(None, Some(_), false) | (None, None, true) => {
				let network_path = build_network_key_dir_or_default(
					self.base_path.clone().map(BasePath::new),
					chain_spec_id,
					executable_name,
				);

				fs::create_dir_all(network_path.as_path())?;

				let certificate_path = network_path.join(WEBRTC_CERTIFICATE_FILE);
				if certificate_path.exists() {
					eprintln!(
						"Skip generation, a certificate already exists in {:?}",
						certificate_path
					);
					return Err(Error::KeyAlreadyExistsInPath(certificate_path));
				} else {
					eprintln!("Generating certificate in {:?}", certificate_path);
					fs::write(certificate_path, file_data)?
				}
			},
			(None, None, false) => io::stdout().lock().write_all(&file_data)?,
			(_, _, _) => {
				// This should not happen, arguments are marked as mutually exclusive.
				return Err(Error::Input("Mutually exclusive arguments provided".into()));
			},
		}

		eprintln!("{}", certhash(&certificate));

		Ok(())
	}
}

#[cfg(test)]
pub mod tests {
	use crate::DEFAULT_NETWORK_CONFIG_PATH;

	use super::*;
	use sc_network::webrtc::decode_certificate;
	use std::io::Read;
	use tempfile::Builder;

	#[test]
	fn generate_webrtc_certificate_file() {
		let mut file = Builder::new().prefix("certfile").tempfile().unwrap();
		let file_path = file.path().display().to_string();
		let generate = GenerateWebRtcCertificateCmd::parse_from(&[
			"generate-webrtc-certificate",
			"--file",
			&file_path,
		]);
		assert!(generate.run("test", &String::from("test")).is_ok());
		let mut buf = String::new();
		assert!(file.read_to_string(&mut buf).is_ok());
		assert!(decode_certificate(buf.as_bytes()).is_ok());
	}

	#[test]
	fn generate_webrtc_certificate_base_path() {
		let base_dir = Builder::new().prefix("certfile").tempdir().unwrap();
		let certificate_path = base_dir
			.path()
			.join("chains/test_id/")
			.join(DEFAULT_NETWORK_CONFIG_PATH)
			.join(WEBRTC_CERTIFICATE_FILE);
		let base_path = base_dir.path().display().to_string();
		let generate = GenerateWebRtcCertificateCmd::parse_from(&[
			"generate-webrtc-certificate",
			"--base-path",
			&base_path,
		]);
		assert!(generate.run("test_id", &String::from("test")).is_ok());
		let buf = fs::read_to_string(certificate_path.as_path()).unwrap();
		assert!(decode_certificate(buf.as_bytes()).is_ok());

		assert!(generate.run("test_id", &String::from("test")).is_err());
		let new_buf = fs::read_to_string(certificate_path).unwrap();
		assert_eq!(new_buf, buf);
	}
}
