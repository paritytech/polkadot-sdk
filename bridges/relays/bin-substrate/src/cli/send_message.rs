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

use crate::{
	chains::{
		millau_headers_to_rialto::MillauToRialtoCliBridge,
		millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
		rialto_headers_to_millau::RialtoToMillauCliBridge,
		rialto_parachains_to_millau::RialtoParachainToMillauCliBridge,
	},
	cli::{
		bridge::{FullBridge, MessagesCliBridge},
		chain_schema::*,
		encode_message::{self, CliEncodeMessage, RawMessage},
		estimate_fee::{estimate_message_delivery_and_dispatch_fee, ConversionRateOverride},
		Balance, CliChain, HexBytes, HexLaneId,
	},
};
use async_trait::async_trait;
use codec::{Decode, Encode};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainBase, SignParam, TransactionSignScheme,
	UnsignedTransaction,
};
use sp_core::{Bytes, Pair};
use sp_runtime::AccountId32;
use std::fmt::{Debug, Display};
use structopt::StructOpt;
use strum::{EnumString, EnumVariantNames, VariantNames};

/// Relayer operating mode.
#[derive(Debug, EnumString, EnumVariantNames, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "kebab_case")]
pub enum DispatchFeePayment {
	/// The dispatch fee is paid at the source chain.
	AtSourceChain,
	/// The dispatch fee is paid at the target chain.
	AtTargetChain,
}

impl From<DispatchFeePayment> for bp_runtime::messages::DispatchFeePayment {
	fn from(dispatch_fee_payment: DispatchFeePayment) -> Self {
		match dispatch_fee_payment {
			DispatchFeePayment::AtSourceChain => Self::AtSourceChain,
			DispatchFeePayment::AtTargetChain => Self::AtTargetChain,
		}
	}
}

/// Send bridge message.
#[derive(StructOpt)]
pub struct SendMessage {
	/// A bridge instance to encode call for.
	#[structopt(possible_values = FullBridge::VARIANTS, case_insensitive = true)]
	bridge: FullBridge,
	#[structopt(flatten)]
	source: SourceConnectionParams,
	#[structopt(flatten)]
	source_sign: SourceSigningParams,
	/// Send message using XCM pallet instead. By default message is sent using
	/// bridge messages pallet.
	#[structopt(long)]
	use_xcm_pallet: bool,
	/// Hex-encoded lane id. Defaults to `00000000`.
	#[structopt(long, default_value = "00000000")]
	lane: HexLaneId,
	/// A way to override conversion rate between bridge tokens.
	///
	/// If not specified, conversion rate from runtime storage is used. It may be obsolete and
	/// your message won't be relayed.
	#[structopt(long)]
	conversion_rate_override: Option<ConversionRateOverride>,
	/// Delivery and dispatch fee in source chain base currency units. If not passed, determined
	/// automatically.
	#[structopt(long)]
	fee: Option<Balance>,
	/// Message type.
	#[structopt(subcommand)]
	message: crate::cli::encode_message::Message,
}

#[async_trait]
trait MessageSender: MessagesCliBridge
where
	Self::Source: ChainBase<Index = u32>
		+ TransactionSignScheme<Chain = Self::Source>
		+ CliChain<KeyPair = AccountKeyPairOf<Self::Source>>
		+ CliEncodeMessage,
	<Self::Source as ChainBase>::Balance: Display + From<u64> + Into<u128>,
	<Self::Source as Chain>::Call: Sync,
	<Self::Source as TransactionSignScheme>::SignedTransaction: Sync,
	AccountIdOf<Self::Source>: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
	AccountId32: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
{
	async fn send_message(data: SendMessage) -> anyhow::Result<()> {
		let payload = encode_message::encode_message::<Self::Source, Self::Target>(&data.message)?;

		let source_client = data.source.into_client::<Self::Source>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;

		let lane = data.lane.clone().into();
		let conversion_rate_override = data.conversion_rate_override;
		let fee = match data.fee {
			Some(fee) => fee,
			None => Balance(
				estimate_message_delivery_and_dispatch_fee::<Self::Source, Self::Target, _>(
					&source_client,
					conversion_rate_override,
					Self::ESTIMATE_MESSAGE_FEE_METHOD,
					lane,
					payload.clone(),
				)
				.await?
				.into(),
			),
		};
		let payload_len = payload.encode().len();
		let send_message_call = if data.use_xcm_pallet {
			Self::Source::encode_send_xcm(
				decode_xcm(payload)?,
				data.bridge.bridge_instance_index(),
			)?
		} else {
			Self::Source::encode_send_message_call(
				data.lane.0,
				payload,
				fee.cast().into(),
				data.bridge.bridge_instance_index(),
			)?
		};

		let source_genesis_hash = *source_client.genesis_hash();
		let (spec_version, transaction_version) = source_client.simple_runtime_version().await?;
		let estimated_transaction_fee = source_client
			.estimate_extrinsic_fee(Bytes(
				Self::Source::sign_transaction(SignParam {
					spec_version,
					transaction_version,
					genesis_hash: source_genesis_hash,
					signer: source_sign.clone(),
					era: relay_substrate_client::TransactionEra::immortal(),
					unsigned: UnsignedTransaction::new(send_message_call.clone(), 0),
				})?
				.encode(),
			))
			.await?;
		source_client
			.submit_signed_extrinsic(source_sign.public().into(), move |_, transaction_nonce| {
				let signed_source_call = Self::Source::sign_transaction(SignParam {
					spec_version,
					transaction_version,
					genesis_hash: source_genesis_hash,
					signer: source_sign.clone(),
					era: relay_substrate_client::TransactionEra::immortal(),
					unsigned: UnsignedTransaction::new(send_message_call, transaction_nonce),
				})?
				.encode();

				log::info!(
					target: "bridge",
					"Sending message to {}. Lane: {:?}. Size: {}. Fee: {}",
					Self::Target::NAME,
					lane,
					payload_len,
					fee,
				);
				log::info!(
					target: "bridge",
					"The source account ({:?}) balance will be reduced by (at most) {} (message fee)
				+ {} (tx fee	) = {} {} tokens", 				AccountId32::from(source_sign.public()),
					fee.0,
					estimated_transaction_fee.inclusion_fee(),
					fee.0.saturating_add(estimated_transaction_fee.inclusion_fee().into()),
					Self::Source::NAME,
				);
				log::info!(
					target: "bridge",
					"Signed {} Call: {:?}",
					Self::Source::NAME,
					HexBytes::encode(&signed_source_call)
				);

				Ok(Bytes(signed_source_call))
			})
			.await?;

		Ok(())
	}
}

impl MessageSender for MillauToRialtoCliBridge {}
impl MessageSender for RialtoToMillauCliBridge {}
impl MessageSender for MillauToRialtoParachainCliBridge {}
impl MessageSender for RialtoParachainToMillauCliBridge {}

impl SendMessage {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self.bridge {
			FullBridge::MillauToRialto => MillauToRialtoCliBridge::send_message(self),
			FullBridge::RialtoToMillau => RialtoToMillauCliBridge::send_message(self),
			FullBridge::MillauToRialtoParachain =>
				MillauToRialtoParachainCliBridge::send_message(self),
			FullBridge::RialtoParachainToMillau =>
				RialtoParachainToMillauCliBridge::send_message(self),
		}
		.await
	}
}

/// Decode SCALE encoded raw XCM message.
fn decode_xcm(message: RawMessage) -> anyhow::Result<xcm::VersionedXcm<()>> {
	Decode::decode(&mut &message[..])
		.map_err(|e| anyhow::format_err!("Failed to decode XCM program: {:?}", e))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::ExplicitOrMaximal;

	#[test]
	fn send_raw_rialto_to_millau() {
		// given
		let send_message = SendMessage::from_iter(vec![
			"send-message",
			"rialto-to-millau",
			"--source-port",
			"1234",
			"--source-signer",
			"//Alice",
			"--conversion-rate-override",
			"0.75",
			"raw",
			"dead",
		]);

		// then
		assert_eq!(send_message.bridge, FullBridge::RialtoToMillau);
		assert_eq!(send_message.source.source_port, 1234);
		assert_eq!(send_message.source_sign.source_signer, Some("//Alice".into()));
		assert_eq!(
			send_message.conversion_rate_override,
			Some(ConversionRateOverride::Explicit(0.75))
		);
		assert_eq!(
			send_message.message,
			crate::cli::encode_message::Message::Raw { data: HexBytes(vec![0xDE, 0xAD]) }
		);
	}

	#[test]
	fn send_sized_rialto_to_millau() {
		// given
		let send_message = SendMessage::from_iter(vec![
			"send-message",
			"rialto-to-millau",
			"--source-port",
			"1234",
			"--source-signer",
			"//Alice",
			"--conversion-rate-override",
			"metric",
			"sized",
			"max",
		]);

		// then
		assert_eq!(send_message.bridge, FullBridge::RialtoToMillau);
		assert_eq!(send_message.source.source_port, 1234);
		assert_eq!(send_message.source_sign.source_signer, Some("//Alice".into()));
		assert_eq!(send_message.conversion_rate_override, Some(ConversionRateOverride::Metric));
		assert_eq!(
			send_message.message,
			crate::cli::encode_message::Message::Sized { size: ExplicitOrMaximal::Maximal }
		);
	}
}
