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

use crate::cli::{
	AccountId, CliChain, ExplicitOrMaximal, HexBytes, Origins, SourceConnectionParams, SourceSigningParams,
	TargetConnectionParams, TargetSigningParams,
};
use codec::{Decode, Encode};
use frame_support::weights::{GetDispatchInfo, Weight};
use pallet_bridge_dispatch::{CallOrigin, MessagePayload};
use relay_millau_client::Millau;
use relay_rialto_client::Rialto;
use relay_substrate_client::{Chain, ConnectionParams, TransactionSignScheme};
use relay_westend_client::Westend;
use sp_core::{Bytes, Pair};
use sp_runtime::{traits::IdentifyAccount, MultiSigner};
use sp_version::RuntimeVersion;
use std::fmt::Debug;

async fn run_relay_messages(command: cli::RelayMessages) -> Result<(), String> {
	match command {
		cli::RelayMessages::MillauToRialto {
			source,
			source_sign,
			target,
			target_sign,
			prometheus_params,
			lane,
		} => {
			type Source = Millau;
			type Target = Rialto;

			let source_client = source_chain_client::<Source>(source).await?;
			let source_sign = Source::source_signing_params(source_sign)?;
			let target_client = target_chain_client::<Target>(target).await?;
			let target_sign = Target::target_signing_params(target_sign)?;

			millau_messages_to_rialto::run(
				source_client,
				source_sign,
				target_client,
				target_sign,
				lane.into(),
				prometheus_params.into(),
			)
			.await
		}
		cli::RelayMessages::RialtoToMillau {
			source,
			source_sign,
			target,
			target_sign,
			prometheus_params,
			lane,
		} => {
			type Source = Rialto;
			type Target = Millau;

			let source_client = source_chain_client::<Source>(source).await?;
			let source_sign = Source::source_signing_params(source_sign)?;
			let target_client = target_chain_client::<Target>(target).await?;
			let target_sign = Target::target_signing_params(target_sign)?;

			rialto_messages_to_millau::run(
				source_client,
				source_sign,
				target_client,
				target_sign,
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
			source,
			source_sign,
			target_sign,
			lane,
			message,
			dispatch_weight,
			fee,
			origin,
			..
		} => {
			type Source = Millau;
			type Target = Rialto;

			let account_ownership_digest = |target_call, source_account_id| {
				millau_runtime::rialto_account_ownership_digest(
					&target_call,
					source_account_id,
					Target::RUNTIME_VERSION.spec_version,
				)
			};
			let estimate_message_fee_method = bp_rialto::TO_RIALTO_ESTIMATE_MESSAGE_FEE_METHOD;
			let fee = fee.map(|x| x.cast());
			let send_message_call = |lane, payload, fee| {
				millau_runtime::Call::BridgeRialtoMessages(millau_runtime::MessagesCall::send_message(
					lane, payload, fee,
				))
			};

			let source_client = source_chain_client::<Source>(source).await?;
			let source_sign = Source::source_signing_params(source_sign)?;
			let target_sign = Target::target_signing_params(target_sign)?;
			let target_call = Target::encode_call(message)?;

			let payload = {
				let target_call_weight = prepare_call_dispatch_weight(
					dispatch_weight,
					ExplicitOrMaximal::Explicit(target_call.get_dispatch_info().weight),
					compute_maximal_message_dispatch_weight(Target::max_extrinsic_weight()),
				);
				let source_sender_public: MultiSigner = source_sign.public().into();
				let source_account_id = source_sender_public.into_account();

				message_payload(
					Target::RUNTIME_VERSION.spec_version,
					target_call_weight,
					match origin {
						Origins::Source => CallOrigin::SourceAccount(source_account_id),
						Origins::Target => {
							let digest = account_ownership_digest(&target_call, source_account_id.clone());
							let target_origin_public = target_sign.public();
							let digest_signature = target_sign.sign(&digest);
							CallOrigin::TargetAccount(
								source_account_id,
								target_origin_public.into(),
								digest_signature.into(),
							)
						}
					},
					&target_call,
				)
			};
			let dispatch_weight = payload.weight;

			let lane = lane.into();
			let fee = get_fee(fee, || {
				estimate_message_delivery_and_dispatch_fee(
					&source_client,
					estimate_message_fee_method,
					lane,
					payload.clone(),
				)
			})
			.await?;

			source_client
				.submit_signed_extrinsic(source_sign.public().into(), |transaction_nonce| {
					let send_message_call = send_message_call(lane, payload, fee);

					let signed_source_call = Source::sign_transaction(
						*source_client.genesis_hash(),
						&source_sign,
						transaction_nonce,
						send_message_call,
					)
					.encode();

					log::info!(
						target: "bridge",
						"Sending message to {}. Size: {}. Dispatch weight: {}. Fee: {}",
						Target::NAME,
						signed_source_call.len(),
						dispatch_weight,
						fee,
					);
					log::info!(
						target: "bridge",
						"Signed {} Call: {:?}",
						Source::NAME,
						HexBytes::encode(&signed_source_call)
					);

					Bytes(signed_source_call)
				})
				.await?;
		}
		cli::SendMessage::RialtoToMillau {
			source,
			source_sign,
			target_sign,
			lane,
			message,
			dispatch_weight,
			fee,
			origin,
			..
		} => {
			type Source = Rialto;
			type Target = Millau;

			let account_ownership_digest = |target_call, source_account_id| {
				rialto_runtime::millau_account_ownership_digest(
					&target_call,
					source_account_id,
					Target::RUNTIME_VERSION.spec_version,
				)
			};
			let estimate_message_fee_method = bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD;
			let fee = fee.map(|x| x.0);
			let send_message_call = |lane, payload, fee| {
				rialto_runtime::Call::BridgeMillauMessages(rialto_runtime::MessagesCall::send_message(
					lane, payload, fee,
				))
			};

			let source_client = source_chain_client::<Source>(source).await?;
			let source_sign = Source::source_signing_params(source_sign)?;
			let target_sign = Target::target_signing_params(target_sign)?;
			let target_call = Target::encode_call(message)?;

			let payload = {
				let target_call_weight = prepare_call_dispatch_weight(
					dispatch_weight,
					ExplicitOrMaximal::Explicit(target_call.get_dispatch_info().weight),
					compute_maximal_message_dispatch_weight(Target::max_extrinsic_weight()),
				);
				let source_sender_public: MultiSigner = source_sign.public().into();
				let source_account_id = source_sender_public.into_account();

				message_payload(
					Target::RUNTIME_VERSION.spec_version,
					target_call_weight,
					match origin {
						Origins::Source => CallOrigin::SourceAccount(source_account_id),
						Origins::Target => {
							let digest = account_ownership_digest(&target_call, source_account_id.clone());
							let target_origin_public = target_sign.public();
							let digest_signature = target_sign.sign(&digest);
							CallOrigin::TargetAccount(
								source_account_id,
								target_origin_public.into(),
								digest_signature.into(),
							)
						}
					},
					&target_call,
				)
			};
			let dispatch_weight = payload.weight;

			let lane = lane.into();
			let fee = get_fee(fee, || {
				estimate_message_delivery_and_dispatch_fee(
					&source_client,
					estimate_message_fee_method,
					lane,
					payload.clone(),
				)
			})
			.await?;

			source_client
				.submit_signed_extrinsic(source_sign.public().into(), |transaction_nonce| {
					let send_message_call = send_message_call(lane, payload, fee);

					let signed_source_call = Source::sign_transaction(
						*source_client.genesis_hash(),
						&source_sign,
						transaction_nonce,
						send_message_call,
					)
					.encode();

					log::info!(
						target: "bridge",
						"Sending message to {}. Size: {}. Dispatch weight: {}. Fee: {}",
						Target::NAME,
						signed_source_call.len(),
						dispatch_weight,
						fee,
					);
					log::info!(
						target: "bridge",
						"Signed {} Call: {:?}",
						Source::NAME,
						HexBytes::encode(&signed_source_call)
					);

					Bytes(signed_source_call)
				})
				.await?;
		}
	}
	Ok(())
}

async fn run_encode_call(call: cli::EncodeCall) -> Result<(), String> {
	match call {
		cli::EncodeCall::Rialto { call } => {
			type Source = Rialto;

			let call = Source::encode_call(call)?;
			println!("{:?}", HexBytes::encode(&call));
		}
		cli::EncodeCall::Millau { call } => {
			type Source = Millau;

			let call = Source::encode_call(call)?;
			println!("{:?}", HexBytes::encode(&call));
		}
	}
	Ok(())
}

async fn run_encode_message_payload(call: cli::EncodeMessagePayload) -> Result<(), String> {
	match call {
		cli::EncodeMessagePayload::RialtoToMillau { payload } => {
			type Source = Rialto;

			let payload = Source::encode_message(payload)?;
			println!("{:?}", HexBytes::encode(&payload));
		}
		cli::EncodeMessagePayload::MillauToRialto { payload } => {
			type Source = Millau;

			let payload = Source::encode_message(payload)?;
			println!("{:?}", HexBytes::encode(&payload));
		}
	}
	Ok(())
}

async fn run_estimate_fee(cmd: cli::EstimateFee) -> Result<(), String> {
	match cmd {
		cli::EstimateFee::RialtoToMillau { source, lane, payload } => {
			type Source = Rialto;
			type SourceBalance = bp_rialto::Balance;

			let estimate_message_fee_method = bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD;

			let source_client = source_chain_client::<Source>(source).await?;
			let lane = lane.into();
			let payload = Source::encode_message(payload)?;

			let fee: Option<SourceBalance> =
				estimate_message_delivery_and_dispatch_fee(&source_client, estimate_message_fee_method, lane, payload)
					.await?;

			println!("Fee: {:?}", fee);
		}
		cli::EstimateFee::MillauToRialto { source, lane, payload } => {
			type Source = Millau;
			type SourceBalance = bp_millau::Balance;

			let estimate_message_fee_method = bp_rialto::TO_RIALTO_ESTIMATE_MESSAGE_FEE_METHOD;

			let source_client = source_chain_client::<Source>(source).await?;
			let lane = lane.into();
			let payload = Source::encode_message(payload)?;

			let fee: Option<SourceBalance> =
				estimate_message_delivery_and_dispatch_fee(&source_client, estimate_message_fee_method, lane, payload)
					.await?;

			println!("Fee: {:?}", fee);
		}
	}

	Ok(())
}

async fn run_derive_account(cmd: cli::DeriveAccount) -> Result<(), String> {
	match cmd {
		cli::DeriveAccount::RialtoToMillau { mut account } => {
			type Source = Rialto;
			type Target = Millau;

			account.enforce_chain::<Source>();
			let acc = bp_runtime::SourceAccount::Account(account.raw_id());
			let id = bp_millau::derive_account_from_rialto_id(acc);
			let derived_account = AccountId::from_raw::<Target>(id);
			println!("Source address:\n{} ({})", account, Source::NAME);
			println!(
				"->Corresponding (derived) address:\n{} ({})",
				derived_account,
				Target::NAME,
			);
		}
		cli::DeriveAccount::MillauToRialto { mut account } => {
			type Source = Millau;
			type Target = Rialto;

			account.enforce_chain::<Source>();
			let acc = bp_runtime::SourceAccount::Account(account.raw_id());
			let id = bp_rialto::derive_account_from_millau_id(acc);
			let derived_account = AccountId::from_raw::<Target>(id);
			println!("Source address:\n{} ({})", account, Source::NAME);
			println!(
				"->Corresponding (derived) address:\n{} ({})",
				derived_account,
				Target::NAME,
			);
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

impl CliChain for Millau {
	const RUNTIME_VERSION: RuntimeVersion = millau_runtime::VERSION;

	type KeyPair = sp_core::sr25519::Pair;
	type MessagePayload = MessagePayload<bp_millau::AccountId, bp_rialto::AccountSigner, bp_rialto::Signature, Vec<u8>>;

	fn ss58_format() -> u16 {
		millau_runtime::SS58Prefix::get() as u16
	}

	fn max_extrinsic_weight() -> Weight {
		bp_millau::max_extrinsic_weight()
	}

	fn encode_call(call: cli::Call) -> Result<Self::Call, String> {
		let call = match call {
			cli::Call::Raw { data } => {
				Decode::decode(&mut &*data.0).map_err(|e| format!("Unable to decode message: {:#?}", e))?
			}
			cli::Call::Remark { remark_size } => {
				millau_runtime::Call::System(millau_runtime::SystemCall::remark(remark_payload(
					remark_size,
					compute_maximal_message_arguments_size(
						bp_rialto::max_extrinsic_size(),
						bp_millau::max_extrinsic_size(),
					),
				)))
			}
			cli::Call::Transfer { mut recipient, amount } => {
				recipient.enforce_chain::<Millau>();
				let amount = amount.cast();
				millau_runtime::Call::Balances(millau_runtime::BalancesCall::transfer(recipient.raw_id(), amount))
			}
			cli::Call::BridgeSendMessage { lane, payload, fee } => {
				type Target = Rialto;

				let payload = Target::encode_message(cli::MessagePayload::Raw { data: payload })?;
				let lane = lane.into();
				millau_runtime::Call::BridgeRialtoMessages(millau_runtime::MessagesCall::send_message(
					lane,
					payload,
					fee.cast(),
				))
			}
		};

		log::info!(target: "bridge", "Generated Millau call: {:#?}", call);
		log::info!(target: "bridge", "Weight of Millau call: {}", call.get_dispatch_info().weight);
		log::info!(target: "bridge", "Encoded Millau call: {:?}", HexBytes::encode(&call));

		Ok(call)
	}

	// TODO [#854|#843] support multiple bridges?
	fn encode_message(message: cli::MessagePayload) -> Result<Self::MessagePayload, String> {
		match message {
			cli::MessagePayload::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Millau's MessagePayload: {:?}", e)),
			cli::MessagePayload::Call { call, mut sender } => {
				type Source = Millau;
				type Target = Rialto;

				sender.enforce_chain::<Source>();
				let spec_version = Target::RUNTIME_VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.raw_id());
				let call = Target::encode_call(call)?;
				let weight = call.get_dispatch_info().weight;

				Ok(message_payload(spec_version, weight, origin, &call))
			}
		}
	}
}

impl CliChain for Rialto {
	const RUNTIME_VERSION: RuntimeVersion = rialto_runtime::VERSION;

	type KeyPair = sp_core::sr25519::Pair;
	type MessagePayload = MessagePayload<bp_rialto::AccountId, bp_millau::AccountSigner, bp_millau::Signature, Vec<u8>>;

	fn ss58_format() -> u16 {
		rialto_runtime::SS58Prefix::get() as u16
	}

	fn max_extrinsic_weight() -> Weight {
		bp_rialto::max_extrinsic_weight()
	}

	fn encode_call(call: cli::Call) -> Result<Self::Call, String> {
		let call = match call {
			cli::Call::Raw { data } => {
				Decode::decode(&mut &*data.0).map_err(|e| format!("Unable to decode message: {:#?}", e))?
			}
			cli::Call::Remark { remark_size } => {
				rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(remark_payload(
					remark_size,
					compute_maximal_message_arguments_size(
						bp_millau::max_extrinsic_size(),
						bp_rialto::max_extrinsic_size(),
					),
				)))
			}
			cli::Call::Transfer { mut recipient, amount } => {
				type Source = Rialto;

				recipient.enforce_chain::<Source>();
				let amount = amount.0;
				rialto_runtime::Call::Balances(rialto_runtime::BalancesCall::transfer(recipient.raw_id(), amount))
			}
			cli::Call::BridgeSendMessage { lane, payload, fee } => {
				type Target = Millau;

				let payload = Target::encode_message(cli::MessagePayload::Raw { data: payload })?;
				let lane = lane.into();
				rialto_runtime::Call::BridgeMillauMessages(rialto_runtime::MessagesCall::send_message(
					lane, payload, fee.0,
				))
			}
		};

		log::info!(target: "bridge", "Generated Rialto call: {:#?}", call);
		log::info!(target: "bridge", "Weight of Rialto call: {}", call.get_dispatch_info().weight);
		log::info!(target: "bridge", "Encoded Rialto call: {:?}", HexBytes::encode(&call));

		Ok(call)
	}

	fn encode_message(message: cli::MessagePayload) -> Result<Self::MessagePayload, String> {
		match message {
			cli::MessagePayload::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Rialto's MessagePayload: {:?}", e)),
			cli::MessagePayload::Call { call, mut sender } => {
				type Source = Rialto;
				type Target = Millau;

				sender.enforce_chain::<Source>();
				let spec_version = Target::RUNTIME_VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.raw_id());
				let call = Target::encode_call(call)?;
				let weight = call.get_dispatch_info().weight;

				Ok(message_payload(spec_version, weight, origin, &call))
			}
		}
	}

	fn source_signing_params(params: SourceSigningParams) -> Result<Self::KeyPair, String> {
		Self::KeyPair::from_string(&params.source_signer, params.source_signer_password.as_deref())
			.map_err(|e| format!("Failed to parse source-signer: {:?}", e))
	}

	fn target_signing_params(params: TargetSigningParams) -> Result<Self::KeyPair, String> {
		Self::KeyPair::from_string(&params.target_signer, params.target_signer_password.as_deref())
			.map_err(|e| format!("Failed to parse target-signer: {:?}", e))
	}
}

impl CliChain for Westend {
	const RUNTIME_VERSION: RuntimeVersion = bp_westend::VERSION;

	type KeyPair = sp_core::sr25519::Pair;
	type MessagePayload = ();

	fn ss58_format() -> u16 {
		42
	}

	fn max_extrinsic_weight() -> Weight {
		0
	}

	fn encode_call(_: cli::Call) -> Result<Self::Call, String> {
		Err("Calling into Westend is not supported yet.".into())
	}

	fn encode_message(_message: cli::MessagePayload) -> Result<Self::MessagePayload, String> {
		Err("Sending messages from Westend is not yet supported.".into())
	}
}

pub async fn source_chain_client<Chain: CliChain>(
	params: SourceConnectionParams,
) -> relay_substrate_client::Result<relay_substrate_client::Client<Chain>> {
	relay_substrate_client::Client::new(ConnectionParams {
		host: params.source_host,
		port: params.source_port,
		secure: params.source_secure,
	})
	.await
}

pub async fn target_chain_client<Chain: CliChain>(
	params: TargetConnectionParams,
) -> relay_substrate_client::Result<relay_substrate_client::Client<Chain>> {
	relay_substrate_client::Client::new(ConnectionParams {
		host: params.target_host,
		port: params.target_port,
		secure: params.target_secure,
	})
	.await
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_messages::source_chain::TargetHeaderChain;
	use sp_core::Pair;
	use sp_runtime::traits::{IdentifyAccount, Verify};

	#[test]
	fn millau_signature_is_valid_on_rialto() {
		let millau_sign = relay_millau_client::SigningParams::from_string("//Dave", None).unwrap();

		let call = rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(vec![]));

		let millau_public: bp_millau::AccountSigner = millau_sign.public().into();
		let millau_account_id: bp_millau::AccountId = millau_public.into_account();

		let digest = millau_runtime::rialto_account_ownership_digest(
			&call,
			millau_account_id,
			rialto_runtime::VERSION.spec_version,
		);

		let rialto_signer = relay_rialto_client::SigningParams::from_string("//Dave", None).unwrap();
		let signature = rialto_signer.sign(&digest);

		assert!(signature.verify(&digest[..], &rialto_signer.public()));
	}

	#[test]
	fn rialto_signature_is_valid_on_millau() {
		let rialto_sign = relay_rialto_client::SigningParams::from_string("//Dave", None).unwrap();

		let call = millau_runtime::Call::System(millau_runtime::SystemCall::remark(vec![]));

		let rialto_public: bp_rialto::AccountSigner = rialto_sign.public().into();
		let rialto_account_id: bp_rialto::AccountId = rialto_public.into_account();

		let digest = rialto_runtime::millau_account_ownership_digest(
			&call,
			rialto_account_id,
			millau_runtime::VERSION.spec_version,
		);

		let millau_signer = relay_millau_client::SigningParams::from_string("//Dave", None).unwrap();
		let signature = millau_signer.sign(&digest);

		assert!(signature.verify(&digest[..], &millau_signer.public()));
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

	#[test]
	fn should_reformat_addresses() {
		// given
		let mut rialto1: AccountId = "5sauUXUfPjmwxSgmb3tZ5d6yx24eZX4wWJ2JtVUBaQqFbvEU".parse().unwrap();
		let mut millau1: AccountId = "752paRyW1EGfq9YLTSSqcSJ5hqnBDidBmaftGhBo8fy6ypW9".parse().unwrap();

		// when
		rialto1.enforce_chain::<Millau>();
		millau1.enforce_chain::<Rialto>();

		// then
		assert_eq!(
			&format!("{}", rialto1),
			"752paRyW1EGfq9YLTSSqcSJ5hqnBDidBmaftGhBo8fy6ypW9"
		);
		assert_eq!(
			&format!("{}", millau1),
			"5sauUXUfPjmwxSgmb3tZ5d6yx24eZX4wWJ2JtVUBaQqFbvEU"
		);
	}
}
