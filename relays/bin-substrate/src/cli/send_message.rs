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
	bridges::{
		rialto_millau::{
			millau_headers_to_rialto::MillauToRialtoCliBridge,
			rialto_headers_to_millau::RialtoToMillauCliBridge,
		},
		rialto_parachain_millau::{
			millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
			rialto_parachains_to_millau::RialtoParachainToMillauCliBridge,
		},
	},
	cli::{
		bridge::{FullBridge, MessagesCliBridge},
		chain_schema::*,
		encode_message::{self, CliEncodeMessage, RawMessage},
		CliChain,
	},
};
use async_trait::async_trait;
use codec::{Decode, Encode};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainBase, ChainWithTransactions, UnsignedTransaction,
};
use sp_core::Pair;
use sp_runtime::AccountId32;
use std::fmt::Display;
use structopt::StructOpt;
use strum::VariantNames;

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
	/// Message type.
	#[structopt(subcommand)]
	message: crate::cli::encode_message::Message,
}

#[async_trait]
trait MessageSender: MessagesCliBridge
where
	Self::Source: ChainBase<Nonce = u32> + ChainWithTransactions + CliChain + CliEncodeMessage,
	<Self::Source as ChainBase>::Balance: Display + From<u64> + Into<u128>,
	<Self::Source as Chain>::Call: Sync,
	<Self::Source as ChainWithTransactions>::SignedTransaction: Sync,
	AccountIdOf<Self::Source>: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
	AccountId32: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
{
	async fn send_message(data: SendMessage) -> anyhow::Result<()> {
		let payload = encode_message::encode_message::<Self::Source, Self::Target>(&data.message)?;

		let source_client = data.source.into_client::<Self::Source>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;

		let payload_len = payload.encoded_size();
		let send_message_call = Self::Source::encode_execute_xcm(decode_xcm(payload)?)?;

		source_client
			.submit_signed_extrinsic(&source_sign, move |_, transaction_nonce| {
				let unsigned = UnsignedTransaction::new(send_message_call, transaction_nonce);
				log::info!(
					target: "bridge",
					"Sending message to {}. Size: {}",
					Self::Target::NAME,
					payload_len,
				);
				Ok(unsigned)
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
			FullBridge::BridgeHubRococoToBridgeHubWococo => unimplemented!(
				"Sending message from BridgeHubRococo to BridgeHubWococo is not supported"
			),
			FullBridge::BridgeHubWococoToBridgeHubRococo => unimplemented!(
				"Sending message from BridgeHubWococo to BridgeHubRococo is not supported"
			),
			FullBridge::BridgeHubKusamaToBridgeHubPolkadot => unimplemented!(
				"Sending message from BridgeHubKusama to BridgeHubPolkadot is not supported"
			),
			FullBridge::BridgeHubPolkadotToBridgeHubKusama => unimplemented!(
				"Sending message from BridgeHubPolkadot to BridgeHubKusama is not supported"
			),
		}
		.await
	}
}

/// Decode SCALE encoded raw XCM message.
pub(crate) fn decode_xcm<Call>(message: RawMessage) -> anyhow::Result<xcm::VersionedXcm<Call>> {
	Decode::decode(&mut &message[..])
		.map_err(|e| anyhow::format_err!("Failed to decode XCM program: {:?}", e))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::{ExplicitOrMaximal, HexBytes};

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
			"raw",
			"dead",
		]);

		// then
		assert_eq!(send_message.bridge, FullBridge::RialtoToMillau);
		assert_eq!(send_message.source.source_port, 1234);
		assert_eq!(send_message.source_sign.source_signer, Some("//Alice".into()));
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
			"sized",
			"max",
		]);

		// then
		assert_eq!(send_message.bridge, FullBridge::RialtoToMillau);
		assert_eq!(send_message.source.source_port, 1234);
		assert_eq!(send_message.source_sign.source_signer, Some("//Alice".into()));
		assert_eq!(
			send_message.message,
			crate::cli::encode_message::Message::Sized { size: ExplicitOrMaximal::Maximal }
		);
	}
}
