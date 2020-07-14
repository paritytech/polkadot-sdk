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

#![recursion_limit = "1024"]

mod ethereum_client;
mod ethereum_deploy_contract;
mod ethereum_exchange;
mod ethereum_sync_loop;
mod ethereum_types;
mod exchange;
mod headers;
mod rpc;
mod rpc_errors;
mod substrate_client;
mod substrate_sync_loop;
mod substrate_types;
mod sync;
mod sync_loop;
mod sync_loop_tests;
mod sync_types;
mod utils;

use ethereum_client::{EthereumConnectionParams, EthereumSigningParams};
use ethereum_sync_loop::EthereumSyncParams;
use parity_crypto::publickey::{KeyPair, Secret};
use sp_core::crypto::Pair;
use std::io::Write;
use substrate_client::{SubstrateConnectionParams, SubstrateSigningParams};
use substrate_sync_loop::SubstrateSyncParams;

fn main() {
	initialize();

	let yaml = clap::load_yaml!("cli.yml");
	let matches = clap::App::from_yaml(yaml).get_matches();
	match matches.subcommand() {
		("eth-to-sub", Some(eth_to_sub_matches)) => {
			if ethereum_sync_loop::run(match ethereum_sync_params(&eth_to_sub_matches) {
				Ok(ethereum_sync_params) => ethereum_sync_params,
				Err(err) => {
					log::error!(target: "bridge", "Error parsing parameters: {}", err);
					return;
				}
			})
			.is_err()
			{
				log::error!(target: "bridge", "Unable to get Substrate genesis block for Ethereum sync.");
				return;
			};
		}
		("sub-to-eth", Some(sub_to_eth_matches)) => {
			if substrate_sync_loop::run(match substrate_sync_params(&sub_to_eth_matches) {
				Ok(substrate_sync_params) => substrate_sync_params,
				Err(err) => {
					log::error!(target: "bridge", "Error parsing parameters: {}", err);
					return;
				}
			})
			.is_err()
			{
				log::error!(target: "bridge", "Unable to get Substrate genesis block for Substrate sync.");
				return;
			};
		}
		("eth-deploy-contract", Some(eth_deploy_matches)) => {
			ethereum_deploy_contract::run(match ethereum_deploy_contract_params(&eth_deploy_matches) {
				Ok(ethereum_deploy_matches) => ethereum_deploy_matches,
				Err(err) => {
					log::error!(target: "bridge", "Error during contract deployment: {}", err);
					return;
				}
			});
		}
		("eth-exchange-sub", Some(eth_exchange_matches)) => {
			ethereum_exchange::run(match ethereum_exchange_params(&eth_exchange_matches) {
				Ok(eth_exchange_params) => eth_exchange_params,
				Err(err) => {
					log::error!(target: "bridge", "Error relaying Ethereum transactions proofs: {}", err);
					return;
				}
			});
		}
		("", _) => {
			log::error!(target: "bridge", "No subcommand specified");
			return;
		}
		_ => unreachable!("all possible subcommands are checked above; qed"),
	}
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
			let timestamp = time::OffsetDateTime::now_local().format("%Y-%m-%d %H:%M:%S %z");
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
				format!(
					"{} {} {} {}",
					Color::Fixed(8).bold().paint(timestamp),
					log_level,
					Color::Fixed(8).paint(record.target()),
					record.args()
				)
			}
		})
	});

	builder.init();
}

fn ethereum_connection_params(matches: &clap::ArgMatches) -> Result<EthereumConnectionParams, String> {
	let mut params = EthereumConnectionParams::default();
	if let Some(eth_host) = matches.value_of("eth-host") {
		params.host = eth_host.into();
	}
	if let Some(eth_port) = matches.value_of("eth-port") {
		params.port = eth_port
			.parse()
			.map_err(|e| format!("Failed to parse eth-port: {}", e))?;
	}
	Ok(params)
}

fn ethereum_signing_params(matches: &clap::ArgMatches) -> Result<EthereumSigningParams, String> {
	let mut params = EthereumSigningParams::default();
	if let Some(eth_signer) = matches.value_of("eth-signer") {
		params.signer = eth_signer
			.parse::<Secret>()
			.map_err(|e| format!("Failed to parse eth-signer: {}", e))
			.and_then(|secret| KeyPair::from_secret(secret).map_err(|e| format!("Invalid eth-signer: {}", e)))?;
	}
	Ok(params)
}

fn substrate_connection_params(matches: &clap::ArgMatches) -> Result<SubstrateConnectionParams, String> {
	let mut params = SubstrateConnectionParams::default();
	if let Some(sub_host) = matches.value_of("sub-host") {
		params.host = sub_host.into();
	}
	if let Some(sub_port) = matches.value_of("sub-port") {
		params.port = sub_port
			.parse()
			.map_err(|e| format!("Failed to parse sub-port: {}", e))?;
	}
	Ok(params)
}

fn substrate_signing_params(matches: &clap::ArgMatches) -> Result<SubstrateSigningParams, String> {
	let mut params = SubstrateSigningParams::default();
	if let Some(sub_signer) = matches.value_of("sub-signer") {
		let sub_signer_password = matches.value_of("sub-signer-password");
		params.signer = sp_core::sr25519::Pair::from_string(sub_signer, sub_signer_password)
			.map_err(|e| format!("Failed to parse sub-signer: {:?}", e))?;
	}
	Ok(params)
}

fn ethereum_sync_params(matches: &clap::ArgMatches) -> Result<EthereumSyncParams, String> {
	let mut eth_sync_params = EthereumSyncParams::default();
	eth_sync_params.eth = ethereum_connection_params(matches)?;
	eth_sync_params.sub = substrate_connection_params(matches)?;
	eth_sync_params.sub_sign = substrate_signing_params(matches)?;

	match matches.value_of("sub-tx-mode") {
		Some("signed") => eth_sync_params.sync_params.target_tx_mode = sync::TargetTransactionMode::Signed,
		Some("unsigned") => {
			eth_sync_params.sync_params.target_tx_mode = sync::TargetTransactionMode::Unsigned;

			// tx pool won't accept too much unsigned transactions
			eth_sync_params.sync_params.max_headers_in_submitted_status = 10;
		}
		Some("backup") => eth_sync_params.sync_params.target_tx_mode = sync::TargetTransactionMode::Backup,
		Some(mode) => return Err(format!("Invalid sub-tx-mode: {}", mode)),
		None => eth_sync_params.sync_params.target_tx_mode = sync::TargetTransactionMode::Signed,
	}

	Ok(eth_sync_params)
}

fn substrate_sync_params(matches: &clap::ArgMatches) -> Result<SubstrateSyncParams, String> {
	let mut sub_sync_params = SubstrateSyncParams::default();
	sub_sync_params.eth = ethereum_connection_params(matches)?;
	sub_sync_params.eth_sign = ethereum_signing_params(matches)?;
	sub_sync_params.sub = substrate_connection_params(matches)?;

	if let Some(eth_contract) = matches.value_of("eth-contract") {
		sub_sync_params.eth_contract_address = eth_contract.parse().map_err(|e| format!("{}", e))?;
	}

	Ok(sub_sync_params)
}

fn ethereum_deploy_contract_params(
	matches: &clap::ArgMatches,
) -> Result<ethereum_deploy_contract::EthereumDeployContractParams, String> {
	let mut eth_deploy_params = ethereum_deploy_contract::EthereumDeployContractParams::default();
	eth_deploy_params.eth = ethereum_connection_params(matches)?;
	eth_deploy_params.eth_sign = ethereum_signing_params(matches)?;
	eth_deploy_params.sub = substrate_connection_params(matches)?;

	if let Some(eth_contract_code) = matches.value_of("eth-contract-code") {
		eth_deploy_params.eth_contract_code =
			hex::decode(&eth_contract_code).map_err(|e| format!("Failed to parse eth-contract-code: {}", e))?;
	}

	Ok(eth_deploy_params)
}

fn ethereum_exchange_params(matches: &clap::ArgMatches) -> Result<ethereum_exchange::EthereumExchangeParams, String> {
	let mut params = ethereum_exchange::EthereumExchangeParams::default();
	params.eth = ethereum_connection_params(matches)?;
	params.sub = substrate_connection_params(matches)?;
	params.sub_sign = substrate_signing_params(matches)?;

	params.eth_tx_hash = matches
		.value_of("eth-tx-hash")
		.expect("eth-tx-hash is a required parameter; clap verifies that required parameters have matches; qed")
		.parse()
		.map_err(|e| format!("Failed to parse eth-tx-hash: {}", e))?;

	Ok(params)
}
