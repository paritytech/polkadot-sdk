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

//! Rialto-to-Millau messages sync entrypoint.

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

/// Description of Rialto -> Millau messages bridge.
#[derive(Clone, Debug)]
pub struct RialtoMessagesToMillau;

impl SubstrateMessageLane for RialtoMessagesToMillau {
	const SOURCE_TO_TARGET_CONVERSION_RATE_PARAMETER_NAME: Option<&'static str> =
		Some(bp_millau::RIALTO_TO_MILLAU_CONVERSION_RATE_PARAMETER_NAME);
	const TARGET_TO_SOURCE_CONVERSION_RATE_PARAMETER_NAME: Option<&'static str> =
		Some(bp_rialto::MILLAU_TO_RIALTO_CONVERSION_RATE_PARAMETER_NAME);

	type SourceChain = Rialto;
	type TargetChain = Millau;

	type SourceTransactionSignScheme = Rialto;
	type TargetTransactionSignScheme = Millau;

	type ReceiveMessagesProofCallBuilder = DirectReceiveMessagesProofCallBuilder<
		Self,
		millau_runtime::Runtime,
		millau_runtime::WithRialtoMessagesInstance,
	>;
	type ReceiveMessagesDeliveryProofCallBuilder = DirectReceiveMessagesDeliveryProofCallBuilder<
		Self,
		rialto_runtime::Runtime,
		rialto_runtime::WithMillauMessagesInstance,
	>;

	type RelayStrategy = MixStrategy;
}

/// Update Millau -> Rialto conversion rate, stored in Rialto runtime storage.
pub(crate) async fn update_millau_to_rialto_conversion_rate(
	client: Client<Rialto>,
	signer: <Rialto as TransactionSignScheme>::AccountKeyPair,
	updated_rate: f64,
) -> anyhow::Result<()> {
	let genesis_hash = *client.genesis_hash();
	let signer_id = (*signer.public().as_array_ref()).into();
	let (spec_version, transaction_version) = client.simple_runtime_version().await?;
	client
		.submit_signed_extrinsic(signer_id, move |_, transaction_nonce| {
			Bytes(
				Rialto::sign_transaction(SignParam {
					spec_version,
					transaction_version,
					genesis_hash,
					signer,
					era: relay_substrate_client::TransactionEra::immortal(),
					unsigned: UnsignedTransaction::new(
						rialto_runtime::MessagesCall::update_pallet_parameter {
							parameter: rialto_runtime::millau_messages::RialtoToMillauMessagesParameter::MillauToRialtoConversionRate(
								sp_runtime::FixedU128::from_float(updated_rate),
							),
						}
						.into(),
						transaction_nonce,
					)
				})
				.encode(),
			)
		})
		.await
		.map(drop)
		.map_err(|err| anyhow::format_err!("{:?}", err))
}
