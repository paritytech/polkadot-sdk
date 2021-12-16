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

//! Millau-to-Rialto messages sync entrypoint.

use codec::Encode;
use sp_core::{Bytes, Pair};

use messages_relay::relay_strategy::MixStrategy;
use relay_millau_client::Millau;
use relay_rialto_client::Rialto;
use relay_substrate_client::{Client, SignParam, TransactionSignScheme, UnsignedTransaction};
use substrate_relay_helper::messages_lane::{
	DirectReceiveMessagesDeliveryProofCallBuilder, DirectReceiveMessagesProofCallBuilder,
	SubstrateMessageLane,
};

/// Description of Millau -> Rialto messages bridge.
#[derive(Clone, Debug)]
pub struct MillauMessagesToRialto;

impl SubstrateMessageLane for MillauMessagesToRialto {
	const SOURCE_TO_TARGET_CONVERSION_RATE_PARAMETER_NAME: Option<&'static str> =
		Some(bp_rialto::MILLAU_TO_RIALTO_CONVERSION_RATE_PARAMETER_NAME);
	const TARGET_TO_SOURCE_CONVERSION_RATE_PARAMETER_NAME: Option<&'static str> =
		Some(bp_millau::RIALTO_TO_MILLAU_CONVERSION_RATE_PARAMETER_NAME);

	type SourceChain = Millau;
	type TargetChain = Rialto;

	type SourceTransactionSignScheme = Millau;
	type TargetTransactionSignScheme = Rialto;

	type ReceiveMessagesProofCallBuilder = DirectReceiveMessagesProofCallBuilder<
		Self,
		rialto_runtime::Runtime,
		rialto_runtime::WithMillauMessagesInstance,
	>;
	type ReceiveMessagesDeliveryProofCallBuilder = DirectReceiveMessagesDeliveryProofCallBuilder<
		Self,
		millau_runtime::Runtime,
		millau_runtime::WithRialtoMessagesInstance,
	>;

	type RelayStrategy = MixStrategy;
}

/// Update Rialto -> Millau conversion rate, stored in Millau runtime storage.
pub(crate) async fn update_rialto_to_millau_conversion_rate(
	client: Client<Millau>,
	signer: <Millau as TransactionSignScheme>::AccountKeyPair,
	updated_rate: f64,
) -> anyhow::Result<()> {
	let genesis_hash = *client.genesis_hash();
	let signer_id = (*signer.public().as_array_ref()).into();
	let (spec_version, transaction_version) = client.simple_runtime_version().await?;
	client
		.submit_signed_extrinsic(signer_id, move |_, transaction_nonce| {
			Bytes(
				Millau::sign_transaction(SignParam {
					spec_version,
					transaction_version,
					genesis_hash,
					signer,
					era: relay_substrate_client::TransactionEra::immortal(),
					unsigned: UnsignedTransaction::new(
						millau_runtime::MessagesCall::update_pallet_parameter {
							parameter: millau_runtime::rialto_messages::MillauToRialtoMessagesParameter::RialtoToMillauConversionRate(
								sp_runtime::FixedU128::from_float(updated_rate),
							),
						}
							.into(),
						transaction_nonce,
					),
				})
				.encode(),
			)
		})
		.await
		.map(drop)
		.map_err(|err| anyhow::format_err!("{:?}", err))
}
