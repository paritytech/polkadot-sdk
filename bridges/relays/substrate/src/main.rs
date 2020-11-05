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

use codec::Encode;
use frame_support::weights::GetDispatchInfo;
use pallet_bridge_call_dispatch::{CallOrigin, MessagePayload};
use relay_millau_client::{Millau, SigningParams as MillauSigningParams};
use relay_rialto_client::{Rialto, SigningParams as RialtoSigningParams};
use relay_substrate_client::{ConnectionParams, TransactionSignScheme};
use relay_utils::initialize::initialize_relay;
use sp_core::{Bytes, Pair};

/// Millau node client.
pub type MillauClient = relay_substrate_client::Client<Millau>;
/// Rialto node client.
pub type RialtoClient = relay_substrate_client::Client<Rialto>;

mod cli;
mod headers_maintain;
mod headers_pipeline;
mod headers_target;
mod messages_source;
mod messages_target;
mod millau_headers_to_rialto;
mod millau_messages_to_rialto;
mod rialto_headers_to_millau;

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
			prometheus_params,
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
			millau_headers_to_rialto::run(millau_client, rialto_client, rialto_sign, prometheus_params.into()).await;
		}
		cli::Command::RialtoHeadersToMillau {
			rialto,
			millau,
			millau_sign,
			prometheus_params,
		} => {
			let rialto_client = RialtoClient::new(ConnectionParams {
				host: rialto.rialto_host,
				port: rialto.rialto_port,
			})
			.await?;
			let millau_client = MillauClient::new(ConnectionParams {
				host: millau.millau_host,
				port: millau.millau_port,
			})
			.await?;
			let millau_sign = MillauSigningParams::from_suri(
				&millau_sign.millau_signer,
				millau_sign.millau_signer_password.as_deref(),
			)
			.map_err(|e| format!("Failed to parse millau-signer: {:?}", e))?;

			rialto_headers_to_millau::run(rialto_client, millau_client, millau_sign, prometheus_params.into()).await;
		}
		cli::Command::MillauMessagesToRialto {
			millau,
			millau_sign,
			rialto,
			rialto_sign,
			prometheus_params,
			lane,
		} => {
			let millau_client = MillauClient::new(ConnectionParams {
				host: millau.millau_host,
				port: millau.millau_port,
			})
			.await?;
			let millau_sign = MillauSigningParams::from_suri(
				&millau_sign.millau_signer,
				millau_sign.millau_signer_password.as_deref(),
			)
			.map_err(|e| format!("Failed to parse millau-signer: {:?}", e))?;
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

			millau_messages_to_rialto::run(
				millau_client,
				millau_sign,
				rialto_client,
				rialto_sign,
				lane.into(),
				prometheus_params.into(),
			);
		}
		cli::Command::SubmitMillauToRialtoMessage {
			millau,
			millau_sign,
			rialto_sign,
			lane,
			message,
			fee,
		} => {
			let millau_client = MillauClient::new(ConnectionParams {
				host: millau.millau_host,
				port: millau.millau_port,
			})
			.await?;
			let millau_sign = MillauSigningParams::from_suri(
				&millau_sign.millau_signer,
				millau_sign.millau_signer_password.as_deref(),
			)
			.map_err(|e| format!("Failed to parse millau-signer: {:?}", e))?;
			let rialto_sign = RialtoSigningParams::from_suri(
				&rialto_sign.rialto_signer,
				rialto_sign.rialto_signer_password.as_deref(),
			)
			.map_err(|e| format!("Failed to parse rialto-signer: {:?}", e))?;

			let rialto_call = match message {
				cli::ToRialtoMessage::Remark => rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(
					format!(
						"Unix time: {}",
						std::time::SystemTime::now()
							.duration_since(std::time::SystemTime::UNIX_EPOCH)
							.unwrap_or_default()
							.as_secs(),
					)
					.as_bytes()
					.to_vec(),
				)),
			};
			let rialto_call_weight = rialto_call.get_dispatch_info().weight;

			let millau_sender_public = millau_sign.signer.public();
			let rialto_origin_public = rialto_sign.signer.public();

			let mut rialto_origin_signature_message = Vec::new();
			rialto_call.encode_to(&mut rialto_origin_signature_message);
			millau_sender_public.encode_to(&mut rialto_origin_signature_message);
			let rialto_origin_signature = rialto_sign.signer.sign(&rialto_origin_signature_message);

			let millau_call =
				millau_runtime::Call::BridgeRialtoMessageLane(millau_runtime::MessageLaneCall::send_message(
					lane.into(),
					MessagePayload {
						spec_version: millau_runtime::VERSION.spec_version,
						weight: rialto_call_weight,
						origin: CallOrigin::RealAccount(
							millau_sender_public.into(),
							rialto_origin_public.into(),
							rialto_origin_signature.into(),
						),
						call: rialto_call.encode(),
					},
					fee,
				));

			let signed_millau_call = Millau::sign_transaction(
				&millau_client,
				&millau_sign.signer,
				millau_client.next_account_index(millau_sender_public.into()).await?,
				millau_call,
			);

			millau_client
				.submit_extrinsic(Bytes(signed_millau_call.encode()))
				.await?;
		}
	}

	Ok(())
}
