// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Parity-Bridge.

// Parity-Bridge is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity-Bridge is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity-Bridge.  If not, see <http://www.gnu.org/licenses/>.

#![recursion_limit="1024"]

mod ethereum_client;
mod ethereum_headers;
mod ethereum_sync;
mod ethereum_sync_loop;
mod ethereum_types;
mod substrate_client;
mod substrate_types;

use std::io::Write;
use sp_core::crypto::Pair;

fn main() {
	initialize();

	ethereum_sync_loop::run(match ethereum_sync_params() {
		Ok(ethereum_sync_params) => ethereum_sync_params,
		Err(err) => {
			log::error!(target: "bridge", "Error parsing parameters: {}", err);
			return;
		},
	});
}

fn initialize() {
	let mut builder = env_logger::Builder::new();

	let filters = match std::env::var("RUST_LOG") {
		Ok(env_filters) => format!("bridge=info,{}", env_filters),
		Err(_) => "bridge=info".into(),
	};

	builder.parse_filters(&filters);
	builder.format(move |buf, record| {
		writeln!(buf, "{}", {
			let timestamp = time::strftime("%Y-%m-%d %H:%M:%S %Z", &time::now())
				.expect("Time is incorrectly formatted");
			if cfg!(windows) {
				format!("{} {} {} {}", timestamp, record.level(), record.target(), record.args())
			} else {
				use ansi_term::Colour as Color;
				let log_level = match record.level() {
					log::Level::Error => Color::Fixed(9).bold().paint(record.level().to_string()),
					log::Level::Warn => Color::Fixed(11).bold().paint(record.level().to_string()),
					log::Level::Info => Color::Fixed(10).paint(record.level().to_string()),
					log::Level::Debug => Color::Fixed(14).paint(record.level().to_string()),
					log::Level::Trace => Color::Fixed(12).paint(record.level().to_string()),
				};
				format!("{} {} {} {}"
					, Color::Fixed(8).bold().paint(timestamp)
					, log_level
					, Color::Fixed(8).paint(record.target())
					, record.args())
			}
		})
	});

	builder.init();
}

fn ethereum_sync_params() -> Result<ethereum_sync_loop::EthereumSyncParams, String> {
	let yaml = clap::load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();

	let mut eth_sync_params = ethereum_sync_loop::EthereumSyncParams::default();
	if let Some(eth_host) = matches.value_of("eth-host") {
		eth_sync_params.eth_host = eth_host.into();
	}
	if let Some(eth_port) = matches.value_of("eth-port") {
		eth_sync_params.eth_port = eth_port.parse().map_err(|e| format!("{}", e))?;
	}
	if let Some(sub_host) = matches.value_of("sub-host") {
		eth_sync_params.sub_host = sub_host.into();
	}
	if let Some(sub_port) = matches.value_of("sub-port") {
		eth_sync_params.sub_port = sub_port.parse().map_err(|e| format!("{}", e))?;
	}
	if let Some(sub_signer) = matches.value_of("sub-signer") {
		let sub_signer_password = matches.value_of("sub-signer-password");
		eth_sync_params.sub_signer = sp_core::sr25519::Pair::from_string(sub_signer, sub_signer_password)
			.map_err(|e| format!("{:?}", e))?;
	}

	Ok(eth_sync_params)
}
