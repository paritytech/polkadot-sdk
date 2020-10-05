// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate-to-substrate relay entrypoint.

#![warn(missing_docs)]

use relay_rialto_client::SigningParams as RialtoSigningParams;
use relay_substrate_client::ConnectionParams;
use relay_utils::initialize::initialize_relay;

/// Millau node client.
pub type MillauClient = relay_substrate_client::Client<relay_millau_client::Millau>;
/// Rialto node client.
pub type RialtoClient = relay_substrate_client::Client<relay_rialto_client::Rialto>;

mod cli;
mod millau_headers_to_rialto;

fn main() {
	initialize_relay();

	let result = async_std::task::block_on(run_command(cli::parse_args()));
	if let Err(error) = result {
		log::error!(target: "bridge", "Failed to start relay: {}", error);
	}
}

async fn run_command(command: cli::Command) -> Result<(), String> {
	match command {
		cli::Command::MillauHeadersToRialto {
			millau,
			rialto,
			rialto_sign,
		} => {
			let millau_client = MillauClient::new(ConnectionParams {
				host: millau.millau_host,
				port: millau.millau_port,
			})
			.await?;
			let rialto_client = RialtoClient::new(ConnectionParams {
				host: rialto.rialto_host,
				port: rialto.rialto_port,
			})
			.await?;
			let rialto_sign = RialtoSigningParams::from_suri(
				&rialto_sign.rialto_signer,
				rialto_sign.rialto_signer_password.as_deref(),
			)
			.map_err(|e| format!("Failed to parse rialto-signer: {:?}", e))?;
			millau_headers_to_rialto::run(millau_client, rialto_client, rialto_sign);
		}
	}

	Ok(())
}
