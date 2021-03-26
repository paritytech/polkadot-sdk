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

//! Rialto <> Millau Bridge commands.

pub mod cli;
pub mod millau_headers_to_rialto;
pub mod millau_messages_to_rialto;
pub mod rialto_headers_to_millau;
pub mod rialto_messages_to_millau;
pub mod westend_headers_to_millau;

/// Millau node client.
pub type MillauClient = relay_substrate_client::Client<Millau>;
/// Rialto node client.
pub type RialtoClient = relay_substrate_client::Client<Rialto>;
/// Westend node client.
pub type WestendClient = relay_substrate_client::Client<Westend>;

use crate::cli::{ExplicitOrMaximal, HexBytes, Origins};
use codec::{Decode, Encode};
use frame_support::weights::{GetDispatchInfo, Weight};
use pallet_bridge_dispatch::{CallOrigin, MessagePayload};
use relay_millau_client::{Millau, SigningParams as MillauSigningParams};
use relay_rialto_client::{Rialto, SigningParams as RialtoSigningParams};
use relay_substrate_client::{Chain, ConnectionParams, TransactionSignScheme};
use relay_westend_client::Westend;
use sp_core::{Bytes, Pair};
use sp_runtime::traits::IdentifyAccount;
use std::fmt::Debug;

async fn run_init_bridge(command: cli::InitBridge) -> Result<(), String> {
	match command {
		cli::InitBridge::MillauToRialto {
			millau,
			rialto,
			rialto_sign,
		} => {
			let millau_client = millau.into_client().await?;
			let rialto_client = rialto.into_client().await?;
			let rialto_sign = rialto_sign.parse()?;

			crate::headers_initialize::initialize(
				millau_client,
				rialto_client.clone(),
				rialto_sign.signer.public().into(),
				move |transaction_nonce, initialization_data| {
					Bytes(
						Rialto::sign_transaction(
							*rialto_client.genesis_hash(),
							&rialto_sign.signer,
							transaction_nonce,
							rialto_runtime::SudoCall::sudo(Box::new(
								rialto_runtime::BridgeGrandpaMillauCall::initialize(initialization_data).into(),
							))
							.into(),
						)
						.encode(),
					)
				},
			)
			.await;
		}
		cli::InitBridge::RialtoToMillau {
			rialto,
			millau,
			millau_sign,
		} => {
			let rialto_client = rialto.into_client().await?;
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;

			crate::headers_initialize::initialize(
				rialto_client,
				millau_client.clone(),
				millau_sign.signer.public().into(),
				move |transaction_nonce, initialization_data| {
					let initialize_call = millau_runtime::BridgeGrandpaRialtoCall::<
						millau_runtime::Runtime,
						millau_runtime::RialtoGrandpaInstance,
					>::initialize(initialization_data);

					Bytes(
						Millau::sign_transaction(
							*millau_client.genesis_hash(),
							&millau_sign.signer,
							transaction_nonce,
							millau_runtime::SudoCall::sudo(Box::new(initialize_call.into())).into(),
						)
						.encode(),
					)
				},
			)
			.await;
		}
		cli::InitBridge::WestendToMillau {
			westend,
			millau,
			millau_sign,
		} => {
			let westend_client = westend.into_client().await?;
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;

			// at Westend -> Millau initialization we're not using sudo, because otherwise our deployments
			// may fail, because we need to initialize both Rialto -> Millau and Westend -> Millau bridge.
			// => since there's single possible sudo account, one of transaction may fail with duplicate nonce error
			crate::headers_initialize::initialize(
				westend_client,
				millau_client.clone(),
				millau_sign.signer.public().into(),
				move |transaction_nonce, initialization_data| {
					let initialize_call = millau_runtime::BridgeGrandpaWestendCall::<
						millau_runtime::Runtime,
						millau_runtime::WestendGrandpaInstance,
					>::initialize(initialization_data);

					Bytes(
						Millau::sign_transaction(
							*millau_client.genesis_hash(),
							&millau_sign.signer,
							transaction_nonce,
							initialize_call.into(),
						)
						.encode(),
					)
				},
			)
			.await;
		}
	}
	Ok(())
}

async fn run_relay_headers(command: cli::RelayHeaders) -> Result<(), String> {
	match command {
		cli::RelayHeaders::MillauToRialto {
			millau,
			rialto,
			rialto_sign,
			prometheus_params,
		} => {
			let millau_client = millau.into_client().await?;
			let rialto_client = rialto.into_client().await?;
			let rialto_sign = rialto_sign.parse()?;
			millau_headers_to_rialto::run(millau_client, rialto_client, rialto_sign, prometheus_params.into()).await
		}
		cli::RelayHeaders::RialtoToMillau {
			rialto,
			millau,
			millau_sign,
			prometheus_params,
		} => {
			let rialto_client = rialto.into_client().await?;
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;
			rialto_headers_to_millau::run(rialto_client, millau_client, millau_sign, prometheus_params.into()).await
		}
		cli::RelayHeaders::WestendToMillau {
			westend,
			millau,
			millau_sign,
			prometheus_params,
		} => {
			let westend_client = westend.into_client().await?;
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;
			westend_headers_to_millau::run(westend_client, millau_client, millau_sign, prometheus_params.into()).await
		}
	}
}

async fn run_relay_messages(command: cli::RelayMessages) -> Result<(), String> {
	match command {
		cli::RelayMessages::MillauToRialto {
			millau,
			millau_sign,
			rialto,
			rialto_sign,
			prometheus_params,
			lane,
		} => {
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;
			let rialto_client = rialto.into_client().await?;
			let rialto_sign = rialto_sign.parse()?;

			millau_messages_to_rialto::run(
				millau_client,
				millau_sign,
				rialto_client,
				rialto_sign,
				lane.into(),
				prometheus_params.into(),
			)
			.await
		}
		cli::RelayMessages::RialtoToMillau {
			rialto,
			rialto_sign,
			millau,
			millau_sign,
			prometheus_params,
			lane,
		} => {
			let rialto_client = rialto.into_client().await?;
			let rialto_sign = rialto_sign.parse()?;
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;

			rialto_messages_to_millau::run(
				rialto_client,
				rialto_sign,
				millau_client,
				millau_sign,
				lane.into(),
				prometheus_params.into(),
			)
			.await
		}
	}
}

async fn run_send_message(command: cli::SendMessage) -> Result<(), String> {
	match command {
		cli::SendMessage::MillauToRialto {
			millau,
			millau_sign,
			rialto_sign,
			lane,
			message,
			dispatch_weight,
			fee,
			origin,
			..
		} => {
			let millau_client = millau.into_client().await?;
			let millau_sign = millau_sign.parse()?;
			let rialto_sign = rialto_sign.parse()?;
			let rialto_call = message.into_call()?;

			let payload =
				millau_to_rialto_message_payload(&millau_sign, &rialto_sign, &rialto_call, origin, dispatch_weight);
			let dispatch_weight = payload.weight;

			let lane = lane.into();
			let fee = get_fee(fee, || {
				estimate_message_delivery_and_dispatch_fee(
					&millau_client,
					bp_rialto::TO_RIALTO_ESTIMATE_MESSAGE_FEE_METHOD,
					lane,
					payload.clone(),
				)
			})
			.await?;

			millau_client
				.submit_signed_extrinsic(millau_sign.signer.public().clone().into(), |transaction_nonce| {
					let millau_call = millau_runtime::Call::BridgeRialtoMessages(
						millau_runtime::MessagesCall::send_message(lane, payload, fee),
					);

					let signed_millau_call = Millau::sign_transaction(
						*millau_client.genesis_hash(),
						&millau_sign.signer,
						transaction_nonce,
						millau_call,
					)
					.encode();

					log::info!(
						target: "bridge",
						"Sending message to Rialto. Size: {}. Dispatch weight: {}. Fee: {}",
						signed_millau_call.len(),
						dispatch_weight,
						fee,
					);
					log::info!(target: "bridge", "Signed Millau Call: {:?}", HexBytes::encode(&signed_millau_call));

					Bytes(signed_millau_call)
				})
				.await?;
		}
		cli::SendMessage::RialtoToMillau {
			rialto,
			rialto_sign,
			millau_sign,
			lane,
			message,
			dispatch_weight,
			fee,
			origin,
			..
		} => {
			let rialto_client = rialto.into_client().await?;
			let rialto_sign = rialto_sign.parse()?;
			let millau_sign = millau_sign.parse()?;
			let millau_call = message.into_call()?;

			let payload =
				rialto_to_millau_message_payload(&rialto_sign, &millau_sign, &millau_call, origin, dispatch_weight);
			let dispatch_weight = payload.weight;

			let lane = lane.into();
			let fee = get_fee(fee, || {
				estimate_message_delivery_and_dispatch_fee(
					&rialto_client,
					bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD,
					lane,
					payload.clone(),
				)
			})
			.await?;

			rialto_client
				.submit_signed_extrinsic(rialto_sign.signer.public().clone().into(), |transaction_nonce| {
					let rialto_call = rialto_runtime::Call::BridgeMillauMessages(
						rialto_runtime::MessagesCall::send_message(lane, payload, fee),
					);

					let signed_rialto_call = Rialto::sign_transaction(
						*rialto_client.genesis_hash(),
						&rialto_sign.signer,
						transaction_nonce,
						rialto_call,
					)
					.encode();

					log::info!(
						target: "bridge",
						"Sending message to Millau. Size: {}. Dispatch weight: {}. Fee: {}",
						signed_rialto_call.len(),
						dispatch_weight,
						fee,
					);
					log::info!(target: "bridge", "Signed Rialto Call: {:?}", HexBytes::encode(&signed_rialto_call));

					Bytes(signed_rialto_call)
				})
				.await?;
		}
	}
	Ok(())
}

async fn run_encode_call(call: cli::EncodeCall) -> Result<(), String> {
	match call {
		cli::EncodeCall::Rialto { call } => {
			let call = call.into_call()?;

			println!("{:?}", HexBytes::encode(&call));
		}
		cli::EncodeCall::Millau { call } => {
			let call = call.into_call()?;
			println!("{:?}", HexBytes::encode(&call));
		}
	}
	Ok(())
}

async fn run_encode_message_payload(call: cli::EncodeMessagePayload) -> Result<(), String> {
	match call {
		cli::EncodeMessagePayload::RialtoToMillau { payload } => {
			let payload = payload.into_payload()?;

			println!("{:?}", HexBytes::encode(&payload));
		}
		cli::EncodeMessagePayload::MillauToRialto { payload } => {
			let payload = payload.into_payload()?;

			println!("{:?}", HexBytes::encode(&payload));
		}
	}
	Ok(())
}

async fn run_estimate_fee(cmd: cli::EstimateFee) -> Result<(), String> {
	match cmd {
		cli::EstimateFee::RialtoToMillau { rialto, lane, payload } => {
			let client = rialto.into_client().await?;
			let lane = lane.into();
			let payload = payload.into_payload()?;

			let fee: Option<bp_rialto::Balance> = estimate_message_delivery_and_dispatch_fee(
				&client,
				bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD,
				lane,
				payload,
			)
			.await?;

			println!("Fee: {:?}", fee);
		}
		cli::EstimateFee::MillauToRialto { millau, lane, payload } => {
			let client = millau.into_client().await?;
			let lane = lane.into();
			let payload = payload.into_payload()?;

			let fee: Option<bp_millau::Balance> = estimate_message_delivery_and_dispatch_fee(
				&client,
				bp_rialto::TO_RIALTO_ESTIMATE_MESSAGE_FEE_METHOD,
				lane,
				payload,
			)
			.await?;

			println!("Fee: {:?}", fee);
		}
	}

	Ok(())
}

async fn run_derive_account(cmd: cli::DeriveAccount) -> Result<(), String> {
	match cmd {
		cli::DeriveAccount::RialtoToMillau { account } => {
			let account = account.into_rialto();
			let acc = bp_runtime::SourceAccount::Account(account.clone());
			let id = bp_millau::derive_account_from_rialto_id(acc);
			println!(
				"{} (Rialto)\n\nCorresponding (derived) account id:\n-> {} (Millau)",
				account, id
			)
		}
		cli::DeriveAccount::MillauToRialto { account } => {
			let account = account.into_millau();
			let acc = bp_runtime::SourceAccount::Account(account.clone());
			let id = bp_rialto::derive_account_from_millau_id(acc);
			println!(
				"{} (Millau)\n\nCorresponding (derived) account id:\n-> {} (Rialto)",
				account, id
			)
		}
	}

	Ok(())
}

async fn estimate_message_delivery_and_dispatch_fee<Fee: Decode, C: Chain, P: Encode>(
	client: &relay_substrate_client::Client<C>,
	estimate_fee_method: &str,
	lane: bp_messages::LaneId,
	payload: P,
) -> Result<Option<Fee>, relay_substrate_client::Error> {
	let encoded_response = client
		.state_call(estimate_fee_method.into(), (lane, payload).encode().into(), None)
		.await?;
	let decoded_response: Option<Fee> =
		Decode::decode(&mut &encoded_response.0[..]).map_err(relay_substrate_client::Error::ResponseParseFailed)?;
	Ok(decoded_response)
}

fn remark_payload(remark_size: Option<ExplicitOrMaximal<usize>>, maximal_allowed_size: u32) -> Vec<u8> {
	match remark_size {
		Some(ExplicitOrMaximal::Explicit(remark_size)) => vec![0; remark_size],
		Some(ExplicitOrMaximal::Maximal) => vec![0; maximal_allowed_size as _],
		None => format!(
			"Unix time: {}",
			std::time::SystemTime::now()
				.duration_since(std::time::SystemTime::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs(),
		)
		.as_bytes()
		.to_vec(),
	}
}

fn message_payload<SAccountId, TPublic, TSignature>(
	spec_version: u32,
	weight: Weight,
	origin: CallOrigin<SAccountId, TPublic, TSignature>,
	call: &impl Encode,
) -> MessagePayload<SAccountId, TPublic, TSignature, Vec<u8>>
where
	SAccountId: Encode + Debug,
	TPublic: Encode + Debug,
	TSignature: Encode + Debug,
{
	// Display nicely formatted call.
	let payload = MessagePayload {
		spec_version,
		weight,
		origin,
		call: HexBytes::encode(call),
	};

	log::info!(target: "bridge", "Created Message Payload: {:#?}", payload);
	log::info!(target: "bridge", "Encoded Message Payload: {:?}", HexBytes::encode(&payload));

	// re-pack to return `Vec<u8>`
	let MessagePayload {
		spec_version,
		weight,
		origin,
		call,
	} = payload;
	MessagePayload {
		spec_version,
		weight,
		origin,
		call: call.0,
	}
}

fn rialto_to_millau_message_payload(
	rialto_sign: &RialtoSigningParams,
	millau_sign: &MillauSigningParams,
	millau_call: &millau_runtime::Call,
	origin: Origins,
	user_specified_dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
) -> rialto_runtime::millau_messages::ToMillauMessagePayload {
	let millau_call_weight = prepare_call_dispatch_weight(
		user_specified_dispatch_weight,
		ExplicitOrMaximal::Explicit(millau_call.get_dispatch_info().weight),
		compute_maximal_message_dispatch_weight(bp_millau::max_extrinsic_weight()),
	);
	let rialto_sender_public: bp_rialto::AccountSigner = rialto_sign.signer.public().clone().into();
	let rialto_account_id: bp_rialto::AccountId = rialto_sender_public.into_account();
	let millau_origin_public = millau_sign.signer.public();

	message_payload(
		millau_runtime::VERSION.spec_version,
		millau_call_weight,
		match origin {
			Origins::Source => CallOrigin::SourceAccount(rialto_account_id),
			Origins::Target => {
				let digest = rialto_runtime::millau_account_ownership_digest(
					&millau_call,
					rialto_account_id.clone(),
					millau_runtime::VERSION.spec_version,
				);

				let digest_signature = millau_sign.signer.sign(&digest);

				CallOrigin::TargetAccount(rialto_account_id, millau_origin_public.into(), digest_signature.into())
			}
		},
		&millau_call,
	)
}

fn millau_to_rialto_message_payload(
	millau_sign: &MillauSigningParams,
	rialto_sign: &RialtoSigningParams,
	rialto_call: &rialto_runtime::Call,
	origin: Origins,
	user_specified_dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
) -> millau_runtime::rialto_messages::ToRialtoMessagePayload {
	let rialto_call_weight = prepare_call_dispatch_weight(
		user_specified_dispatch_weight,
		ExplicitOrMaximal::Explicit(rialto_call.get_dispatch_info().weight),
		compute_maximal_message_dispatch_weight(bp_rialto::max_extrinsic_weight()),
	);
	let millau_sender_public: bp_millau::AccountSigner = millau_sign.signer.public().clone().into();
	let millau_account_id: bp_millau::AccountId = millau_sender_public.into_account();
	let rialto_origin_public = rialto_sign.signer.public();

	message_payload(
		rialto_runtime::VERSION.spec_version,
		rialto_call_weight,
		match origin {
			Origins::Source => CallOrigin::SourceAccount(millau_account_id),
			Origins::Target => {
				let digest = millau_runtime::rialto_account_ownership_digest(
					&rialto_call,
					millau_account_id.clone(),
					rialto_runtime::VERSION.spec_version,
				);

				let digest_signature = rialto_sign.signer.sign(&digest);

				CallOrigin::TargetAccount(millau_account_id, rialto_origin_public.into(), digest_signature.into())
			}
		},
		&rialto_call,
	)
}

fn prepare_call_dispatch_weight(
	user_specified_dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
	weight_from_pre_dispatch_call: ExplicitOrMaximal<Weight>,
	maximal_allowed_weight: Weight,
) -> Weight {
	match user_specified_dispatch_weight.unwrap_or(weight_from_pre_dispatch_call) {
		ExplicitOrMaximal::Explicit(weight) => weight,
		ExplicitOrMaximal::Maximal => maximal_allowed_weight,
	}
}

async fn get_fee<Fee, F, R, E>(fee: Option<Fee>, f: F) -> Result<Fee, String>
where
	Fee: Decode,
	F: FnOnce() -> R,
	R: std::future::Future<Output = Result<Option<Fee>, E>>,
	E: Debug,
{
	match fee {
		Some(fee) => Ok(fee),
		None => match f().await {
			Ok(Some(fee)) => Ok(fee),
			Ok(None) => Err("Failed to estimate message fee. Message is too heavy?".into()),
			Err(error) => Err(format!("Failed to estimate message fee: {:?}", error)),
		},
	}
}

fn compute_maximal_message_dispatch_weight(maximal_extrinsic_weight: Weight) -> Weight {
	bridge_runtime_common::messages::target::maximal_incoming_message_dispatch_weight(maximal_extrinsic_weight)
}

fn compute_maximal_message_arguments_size(
	maximal_source_extrinsic_size: u32,
	maximal_target_extrinsic_size: u32,
) -> u32 {
	// assume that both signed extensions and other arguments fit 1KB
	let service_tx_bytes_on_source_chain = 1024;
	let maximal_source_extrinsic_size = maximal_source_extrinsic_size - service_tx_bytes_on_source_chain;
	let maximal_call_size =
		bridge_runtime_common::messages::target::maximal_incoming_message_size(maximal_target_extrinsic_size);
	let maximal_call_size = if maximal_call_size > maximal_source_extrinsic_size {
		maximal_source_extrinsic_size
	} else {
		maximal_call_size
	};

	// bytes in Call encoding that are used to encode everything except arguments
	let service_bytes = 1 + 1 + 4;
	maximal_call_size - service_bytes
}

impl cli::MillauToRialtoMessagePayload {
	/// Parse the CLI parameters and construct message payload.
	pub fn into_payload(
		self,
	) -> Result<MessagePayload<bp_rialto::AccountId, bp_rialto::AccountSigner, bp_rialto::Signature, Vec<u8>>, String> {
		match self {
			Self::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Millau's MessagePayload: {:?}", e)),
			Self::Message { message, sender } => {
				let spec_version = rialto_runtime::VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.into_millau());
				let call = message.into_call()?;
				let weight = call.get_dispatch_info().weight;

				Ok(message_payload(spec_version, weight, origin, &call))
			}
		}
	}
}

impl cli::RialtoToMillauMessagePayload {
	/// Parse the CLI parameters and construct message payload.
	pub fn into_payload(
		self,
	) -> Result<MessagePayload<bp_millau::AccountId, bp_millau::AccountSigner, bp_millau::Signature, Vec<u8>>, String> {
		match self {
			Self::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Rialto's MessagePayload: {:?}", e)),
			Self::Message { message, sender } => {
				let spec_version = millau_runtime::VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.into_rialto());
				let call = message.into_call()?;
				let weight = call.get_dispatch_info().weight;

				Ok(message_payload(spec_version, weight, origin, &call))
			}
		}
	}
}

impl cli::RialtoSigningParams {
	/// Parse CLI parameters into typed signing params.
	pub fn parse(self) -> Result<RialtoSigningParams, String> {
		RialtoSigningParams::from_suri(&self.rialto_signer, self.rialto_signer_password.as_deref())
			.map_err(|e| format!("Failed to parse rialto-signer: {:?}", e))
	}
}

impl cli::MillauSigningParams {
	/// Parse CLI parameters into typed signing params.
	pub fn parse(self) -> Result<MillauSigningParams, String> {
		MillauSigningParams::from_suri(&self.millau_signer, self.millau_signer_password.as_deref())
			.map_err(|e| format!("Failed to parse millau-signer: {:?}", e))
	}
}

impl cli::MillauConnectionParams {
	/// Convert CLI connection parameters into Millau RPC Client.
	pub async fn into_client(self) -> relay_substrate_client::Result<MillauClient> {
		MillauClient::new(ConnectionParams {
			host: self.millau_host,
			port: self.millau_port,
			secure: self.millau_secure,
		})
		.await
	}
}

impl cli::RialtoConnectionParams {
	/// Convert CLI connection parameters into Rialto RPC Client.
	pub async fn into_client(self) -> relay_substrate_client::Result<RialtoClient> {
		RialtoClient::new(ConnectionParams {
			host: self.rialto_host,
			port: self.rialto_port,
			secure: self.rialto_secure,
		})
		.await
	}
}

impl cli::WestendConnectionParams {
	/// Convert CLI connection parameters into Westend RPC Client.
	pub async fn into_client(self) -> relay_substrate_client::Result<WestendClient> {
		WestendClient::new(ConnectionParams {
			host: self.westend_host,
			port: self.westend_port,
			secure: self.westend_secure,
		})
		.await
	}
}

impl cli::ToRialtoMessage {
	/// Convert CLI call request into runtime `Call` instance.
	pub fn into_call(self) -> Result<rialto_runtime::Call, String> {
		let call = match self {
			cli::ToRialtoMessage::Raw { data } => {
				Decode::decode(&mut &*data.0).map_err(|e| format!("Unable to decode message: {:#?}", e))?
			}
			cli::ToRialtoMessage::Remark { remark_size } => {
				rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(remark_payload(
					remark_size,
					compute_maximal_message_arguments_size(
						bp_millau::max_extrinsic_size(),
						bp_rialto::max_extrinsic_size(),
					),
				)))
			}
			cli::ToRialtoMessage::Transfer { recipient, amount } => {
				let recipient = recipient.into_rialto();
				rialto_runtime::Call::Balances(rialto_runtime::BalancesCall::transfer(recipient, amount))
			}
			cli::ToRialtoMessage::MillauSendMessage { lane, payload, fee } => {
				let payload = cli::RialtoToMillauMessagePayload::Raw { data: payload }.into_payload()?;
				let lane = lane.into();
				rialto_runtime::Call::BridgeMillauMessages(rialto_runtime::MessagesCall::send_message(
					lane, payload, fee,
				))
			}
		};

		log::info!(target: "bridge", "Generated Rialto call: {:#?}", call);
		log::info!(target: "bridge", "Weight of Rialto call: {}", call.get_dispatch_info().weight);
		log::info!(target: "bridge", "Encoded Rialto call: {:?}", HexBytes::encode(&call));

		Ok(call)
	}
}

impl cli::ToMillauMessage {
	/// Convert CLI call request into runtime `Call` instance.
	pub fn into_call(self) -> Result<millau_runtime::Call, String> {
		let call = match self {
			cli::ToMillauMessage::Raw { data } => {
				Decode::decode(&mut &*data.0).map_err(|e| format!("Unable to decode message: {:#?}", e))?
			}
			cli::ToMillauMessage::Remark { remark_size } => {
				millau_runtime::Call::System(millau_runtime::SystemCall::remark(remark_payload(
					remark_size,
					compute_maximal_message_arguments_size(
						bp_rialto::max_extrinsic_size(),
						bp_millau::max_extrinsic_size(),
					),
				)))
			}
			cli::ToMillauMessage::Transfer { recipient, amount } => {
				let recipient = recipient.into_millau();
				millau_runtime::Call::Balances(millau_runtime::BalancesCall::transfer(recipient, amount))
			}
			cli::ToMillauMessage::RialtoSendMessage { lane, payload, fee } => {
				let payload = cli::MillauToRialtoMessagePayload::Raw { data: payload }.into_payload()?;
				let lane = lane.into();
				millau_runtime::Call::BridgeRialtoMessages(millau_runtime::MessagesCall::send_message(
					lane, payload, fee,
				))
			}
		};

		log::info!(target: "bridge", "Generated Millau call: {:#?}", call);
		log::info!(target: "bridge", "Weight of Millau call: {}", call.get_dispatch_info().weight);
		log::info!(target: "bridge", "Encoded Millau call: {:?}", HexBytes::encode(&call));

		Ok(call)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_messages::source_chain::TargetHeaderChain;
	use sp_core::Pair;
	use sp_runtime::traits::{IdentifyAccount, Verify};

	#[test]
	fn millau_signature_is_valid_on_rialto() {
		let millau_sign = relay_millau_client::SigningParams::from_suri("//Dave", None).unwrap();

		let call = rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(vec![]));

		let millau_public: bp_millau::AccountSigner = millau_sign.signer.public().clone().into();
		let millau_account_id: bp_millau::AccountId = millau_public.into_account();

		let digest = millau_runtime::rialto_account_ownership_digest(
			&call,
			millau_account_id,
			rialto_runtime::VERSION.spec_version,
		);

		let rialto_signer = relay_rialto_client::SigningParams::from_suri("//Dave", None).unwrap();
		let signature = rialto_signer.signer.sign(&digest);

		assert!(signature.verify(&digest[..], &rialto_signer.signer.public()));
	}

	#[test]
	fn rialto_signature_is_valid_on_millau() {
		let rialto_sign = relay_rialto_client::SigningParams::from_suri("//Dave", None).unwrap();

		let call = millau_runtime::Call::System(millau_runtime::SystemCall::remark(vec![]));

		let rialto_public: bp_rialto::AccountSigner = rialto_sign.signer.public().clone().into();
		let rialto_account_id: bp_rialto::AccountId = rialto_public.into_account();

		let digest = rialto_runtime::millau_account_ownership_digest(
			&call,
			rialto_account_id,
			millau_runtime::VERSION.spec_version,
		);

		let millau_signer = relay_millau_client::SigningParams::from_suri("//Dave", None).unwrap();
		let signature = millau_signer.signer.sign(&digest);

		assert!(signature.verify(&digest[..], &millau_signer.signer.public()));
	}

	#[test]
	fn maximal_rialto_to_millau_message_arguments_size_is_computed_correctly() {
		use rialto_runtime::millau_messages::Millau;

		let maximal_remark_size =
			compute_maximal_message_arguments_size(bp_rialto::max_extrinsic_size(), bp_millau::max_extrinsic_size());

		let call: millau_runtime::Call = millau_runtime::SystemCall::remark(vec![42; maximal_remark_size as _]).into();
		let payload = message_payload(
			Default::default(),
			call.get_dispatch_info().weight,
			pallet_bridge_dispatch::CallOrigin::SourceRoot,
			&call,
		);
		assert_eq!(Millau::verify_message(&payload), Ok(()));

		let call: millau_runtime::Call =
			millau_runtime::SystemCall::remark(vec![42; (maximal_remark_size + 1) as _]).into();
		let payload = message_payload(
			Default::default(),
			call.get_dispatch_info().weight,
			pallet_bridge_dispatch::CallOrigin::SourceRoot,
			&call,
		);
		assert!(Millau::verify_message(&payload).is_err());
	}

	#[test]
	fn maximal_size_remark_to_rialto_is_generated_correctly() {
		assert!(
			bridge_runtime_common::messages::target::maximal_incoming_message_size(
				bp_rialto::max_extrinsic_size()
			) > bp_millau::max_extrinsic_size(),
			"We can't actually send maximal messages to Rialto from Millau, because Millau extrinsics can't be that large",
		)
	}

	#[test]
	fn maximal_rialto_to_millau_message_dispatch_weight_is_computed_correctly() {
		use rialto_runtime::millau_messages::Millau;

		let maximal_dispatch_weight = compute_maximal_message_dispatch_weight(bp_millau::max_extrinsic_weight());
		let call: millau_runtime::Call = rialto_runtime::SystemCall::remark(vec![]).into();

		let payload = message_payload(
			Default::default(),
			maximal_dispatch_weight,
			pallet_bridge_dispatch::CallOrigin::SourceRoot,
			&call,
		);
		assert_eq!(Millau::verify_message(&payload), Ok(()));

		let payload = message_payload(
			Default::default(),
			maximal_dispatch_weight + 1,
			pallet_bridge_dispatch::CallOrigin::SourceRoot,
			&call,
		);
		assert!(Millau::verify_message(&payload).is_err());
	}

	#[test]
	fn maximal_weight_fill_block_to_rialto_is_generated_correctly() {
		use millau_runtime::rialto_messages::Rialto;

		let maximal_dispatch_weight = compute_maximal_message_dispatch_weight(bp_rialto::max_extrinsic_weight());
		let call: rialto_runtime::Call = millau_runtime::SystemCall::remark(vec![]).into();

		let payload = message_payload(
			Default::default(),
			maximal_dispatch_weight,
			pallet_bridge_dispatch::CallOrigin::SourceRoot,
			&call,
		);
		assert_eq!(Rialto::verify_message(&payload), Ok(()));

		let payload = message_payload(
			Default::default(),
			maximal_dispatch_weight + 1,
			pallet_bridge_dispatch::CallOrigin::SourceRoot,
			&call,
		);
		assert!(Rialto::verify_message(&payload).is_err());
	}

	#[test]
	fn rialto_tx_extra_bytes_constant_is_correct() {
		let rialto_call = rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(vec![]));
		let rialto_tx = Rialto::sign_transaction(
			Default::default(),
			&sp_keyring::AccountKeyring::Alice.pair(),
			0,
			rialto_call.clone(),
		);
		let extra_bytes_in_transaction = rialto_tx.encode().len() - rialto_call.encode().len();
		assert!(
			bp_rialto::TX_EXTRA_BYTES as usize >= extra_bytes_in_transaction,
			"Hardcoded number of extra bytes in Rialto transaction {} is lower than actual value: {}",
			bp_rialto::TX_EXTRA_BYTES,
			extra_bytes_in_transaction,
		);
	}

	#[test]
	fn millau_tx_extra_bytes_constant_is_correct() {
		let millau_call = millau_runtime::Call::System(millau_runtime::SystemCall::remark(vec![]));
		let millau_tx = Millau::sign_transaction(
			Default::default(),
			&sp_keyring::AccountKeyring::Alice.pair(),
			0,
			millau_call.clone(),
		);
		let extra_bytes_in_transaction = millau_tx.encode().len() - millau_call.encode().len();
		assert!(
			bp_millau::TX_EXTRA_BYTES as usize >= extra_bytes_in_transaction,
			"Hardcoded number of extra bytes in Millau transaction {} is lower than actual value: {}",
			bp_millau::TX_EXTRA_BYTES,
			extra_bytes_in_transaction,
		);
	}
}
