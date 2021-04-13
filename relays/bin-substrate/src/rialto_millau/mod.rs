// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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
	bridge::{MILLAU_TO_RIALTO_INDEX, RIALTO_TO_MILLAU_INDEX},
	encode_call::{self, Call, CliEncodeCall},
	encode_message, CliChain, ExplicitOrMaximal, HexBytes, Origins,
};
use codec::{Decode, Encode};
use frame_support::weights::{GetDispatchInfo, Weight};
use pallet_bridge_dispatch::{CallOrigin, MessagePayload};
use relay_millau_client::Millau;
use relay_rialto_client::Rialto;
use relay_substrate_client::{Chain, TransactionSignScheme};
use relay_westend_client::Westend;
use sp_core::{Bytes, Pair};
use sp_runtime::{traits::IdentifyAccount, MultiSigner};
use sp_version::RuntimeVersion;
use std::fmt::Debug;

async fn run_send_message(command: cli::SendMessage) -> Result<(), String> {
	match command {
		cli::SendMessage::MillauToRialto {
			source,
			source_sign,
			target_sign,
			lane,
			mut message,
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

			let source_client = source.into_client::<Source>().await.map_err(format_err)?;
			let source_sign = source_sign.into_keypair::<Source>().map_err(format_err)?;
			let target_sign = target_sign.into_keypair::<Target>().map_err(format_err)?;

			encode_call::preprocess_call::<Source, Target>(&mut message, MILLAU_TO_RIALTO_INDEX);
			let target_call = Target::encode_call(&message).map_err(|e| e.to_string())?;

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
			mut message,
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

			let source_client = source.into_client::<Source>().await.map_err(format_err)?;
			let source_sign = source_sign.into_keypair::<Source>().map_err(format_err)?;
			let target_sign = target_sign.into_keypair::<Target>().map_err(format_err)?;

			encode_call::preprocess_call::<Source, Target>(&mut message, RIALTO_TO_MILLAU_INDEX);
			let target_call = Target::encode_call(&message).map_err(|e| e.to_string())?;

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

async fn run_estimate_fee(cmd: cli::EstimateFee) -> Result<(), String> {
	match cmd {
		cli::EstimateFee::RialtoToMillau { source, lane, payload } => {
			type Source = Rialto;
			type SourceBalance = bp_rialto::Balance;

			let estimate_message_fee_method = bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD;

			let source_client = source.into_client::<Source>().await.map_err(format_err)?;
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

			let source_client = source.into_client::<Source>().await.map_err(format_err)?;
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

impl CliEncodeCall for Millau {
	fn max_extrinsic_size() -> u32 {
		bp_millau::max_extrinsic_size()
	}

	fn encode_call(call: &Call) -> anyhow::Result<Self::Call> {
		Ok(match call {
			Call::Raw { data } => Decode::decode(&mut &*data.0)?,
			Call::Remark { remark_payload, .. } => millau_runtime::Call::System(millau_runtime::SystemCall::remark(
				remark_payload.as_ref().map(|x| x.0.clone()).unwrap_or_default(),
			)),
			Call::Transfer { recipient, amount } => millau_runtime::Call::Balances(
				millau_runtime::BalancesCall::transfer(recipient.raw_id(), amount.cast()),
			),
			Call::BridgeSendMessage {
				lane,
				payload,
				fee,
				bridge_instance_index,
			} => match *bridge_instance_index {
				MILLAU_TO_RIALTO_INDEX => {
					let payload = Decode::decode(&mut &*payload.0)?;
					millau_runtime::Call::BridgeRialtoMessages(millau_runtime::MessagesCall::send_message(
						lane.0,
						payload,
						fee.cast(),
					))
				}
				_ => anyhow::bail!(
					"Unsupported target bridge pallet with instance index: {}",
					bridge_instance_index
				),
			},
		})
	}
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

	// TODO [#854|#843] support multiple bridges?
	fn encode_message(message: encode_message::MessagePayload) -> Result<Self::MessagePayload, String> {
		match message {
			encode_message::MessagePayload::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Millau's MessagePayload: {:?}", e)),
			encode_message::MessagePayload::Call { mut call, mut sender } => {
				type Source = Millau;
				type Target = Rialto;

				sender.enforce_chain::<Source>();
				let spec_version = Target::RUNTIME_VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.raw_id());
				encode_call::preprocess_call::<Source, Target>(&mut call, MILLAU_TO_RIALTO_INDEX);
				let call = Target::encode_call(&call).map_err(|e| e.to_string())?;
				let weight = call.get_dispatch_info().weight;

				Ok(message_payload(spec_version, weight, origin, &call))
			}
		}
	}
}

impl CliEncodeCall for Rialto {
	fn max_extrinsic_size() -> u32 {
		bp_rialto::max_extrinsic_size()
	}

	fn encode_call(call: &Call) -> anyhow::Result<Self::Call> {
		Ok(match call {
			Call::Raw { data } => Decode::decode(&mut &*data.0)?,
			Call::Remark { remark_payload, .. } => rialto_runtime::Call::System(rialto_runtime::SystemCall::remark(
				remark_payload.as_ref().map(|x| x.0.clone()).unwrap_or_default(),
			)),
			Call::Transfer { recipient, amount } => {
				rialto_runtime::Call::Balances(rialto_runtime::BalancesCall::transfer(recipient.raw_id(), amount.0))
			}
			Call::BridgeSendMessage {
				lane,
				payload,
				fee,
				bridge_instance_index,
			} => match *bridge_instance_index {
				RIALTO_TO_MILLAU_INDEX => {
					let payload = Decode::decode(&mut &*payload.0)?;
					rialto_runtime::Call::BridgeMillauMessages(rialto_runtime::MessagesCall::send_message(
						lane.0, payload, fee.0,
					))
				}
				_ => anyhow::bail!(
					"Unsupported target bridge pallet with instance index: {}",
					bridge_instance_index
				),
			},
		})
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

	fn encode_message(message: encode_message::MessagePayload) -> Result<Self::MessagePayload, String> {
		match message {
			encode_message::MessagePayload::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Rialto's MessagePayload: {:?}", e)),
			encode_message::MessagePayload::Call { mut call, mut sender } => {
				type Source = Rialto;
				type Target = Millau;

				sender.enforce_chain::<Source>();
				let spec_version = Target::RUNTIME_VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.raw_id());
				encode_call::preprocess_call::<Source, Target>(&mut call, RIALTO_TO_MILLAU_INDEX);
				let call = Target::encode_call(&call).map_err(|e| e.to_string())?;
				let weight = call.get_dispatch_info().weight;

				Ok(message_payload(spec_version, weight, origin, &call))
			}
		}
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

	fn encode_message(_message: encode_message::MessagePayload) -> Result<Self::MessagePayload, String> {
		Err("Sending messages from Westend is not yet supported.".into())
	}
}

fn format_err(e: anyhow::Error) -> String {
	e.to_string()
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

		let maximal_remark_size = encode_call::compute_maximal_message_arguments_size(
			bp_rialto::max_extrinsic_size(),
			bp_millau::max_extrinsic_size(),
		);

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
