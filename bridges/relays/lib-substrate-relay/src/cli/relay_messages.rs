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

//! Primitives for exposing the messages relaying functionality in the CLI.

use crate::{
	cli::{bridge::*, chain_schema::*, HexLaneId, PrometheusParams},
	messages::MessagesRelayParams,
	TransactionParams,
};

use async_trait::async_trait;
use clap::Parser;
use sp_core::Pair;

use bp_messages::MessageNonce;
use bp_runtime::HeaderIdProvider;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BalanceOf, Chain, ChainWithRuntimeVersion,
	ChainWithTransactions, Client,
};
use relay_utils::UniqueSaturatedInto;
use sp_runtime::traits::TryConvert;

/// Messages relaying params.
#[derive(Parser)]
pub struct RelayMessagesParams {
	/// Hex-encoded lane id that should be served by the relay.
	#[arg(long)]
	lane: HexLaneId,
	#[command(flatten)]
	source: SourceConnectionParams,
	#[command(flatten)]
	source_sign: SourceSigningParams,
	#[command(flatten)]
	target: TargetConnectionParams,
	#[command(flatten)]
	target_sign: TargetSigningParams,
	#[command(flatten)]
	prometheus_params: PrometheusParams,
}

/// Messages range relaying params.
#[derive(Parser)]
pub struct RelayMessagesRangeParams {
	/// Number of the source chain header that we will use to prepare a messages proof.
	/// This header must be previously proved to the target chain.
	#[arg(long)]
	at_source_block: u128,
	/// Hex-encoded lane id that should be served by the relay.
	#[arg(long)]
	lane: HexLaneId,
	/// Nonce (inclusive) of the first message to relay.
	#[arg(long)]
	messages_start: MessageNonce,
	/// Nonce (inclusive) of the last message to relay.
	#[arg(long)]
	messages_end: MessageNonce,
	/// Whether the outbound lane state proof should be included into transaction.
	#[arg(long)]
	outbound_state_proof_required: bool,
	#[command(flatten)]
	source: SourceConnectionParams,
	#[command(flatten)]
	source_sign: SourceSigningParams,
	#[command(flatten)]
	target: TargetConnectionParams,
	#[command(flatten)]
	target_sign: TargetSigningParams,
}

/// Messages delivery confirmation relaying params.
#[derive(Parser)]
pub struct RelayMessagesDeliveryConfirmationParams {
	/// Number of the target chain header that we will use to prepare a messages
	/// delivery proof. This header must be previously proved to the source chain.
	#[arg(long)]
	at_target_block: u128,
	/// Hex-encoded lane id that should be served by the relay.
	#[arg(long)]
	lane: HexLaneId,
	#[command(flatten)]
	source: SourceConnectionParams,
	#[command(flatten)]
	source_sign: SourceSigningParams,
	#[command(flatten)]
	target: TargetConnectionParams,
}

/// Trait used for relaying messages between 2 chains.
#[async_trait]
pub trait MessagesRelayer: MessagesCliBridge
where
	Self::Source: ChainWithTransactions + ChainWithRuntimeVersion,
	AccountIdOf<Self::Source>: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
	AccountIdOf<Self::Target>: From<<AccountKeyPairOf<Self::Target> as Pair>::Public>,
	BalanceOf<Self::Source>: TryFrom<BalanceOf<Self::Target>>,
{
	/// Start relaying messages.
	async fn relay_messages(data: RelayMessagesParams) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;
		let source_transactions_mortality = data.source_sign.transactions_mortality()?;
		let target_client = data.target.into_client::<Self::Target>().await?;
		let target_sign = data.target_sign.to_keypair::<Self::Target>()?;
		let target_transactions_mortality = data.target_sign.transactions_mortality()?;
		let lane_id = HexLaneId::try_convert(data.lane).map_err(|invalid_lane_id| {
			anyhow::format_err!("Invalid laneId: {:?}!", invalid_lane_id)
		})?;

		Self::start_relay_guards(&target_client, target_client.can_start_version_guard()).await?;

		crate::messages::run::<Self::MessagesLane, _, _>(MessagesRelayParams {
			source_client,
			source_transaction_params: TransactionParams {
				signer: source_sign,
				mortality: source_transactions_mortality,
			},
			target_client,
			target_transaction_params: TransactionParams {
				signer: target_sign,
				mortality: target_transactions_mortality,
			},
			source_to_target_headers_relay: None,
			target_to_source_headers_relay: None,
			lane_id,
			limits: Self::maybe_messages_limits(),
			metrics_params: data.prometheus_params.into_metrics_params()?,
		})
		.await
		.map_err(|e| anyhow::format_err!("{}", e))
	}

	/// Relay a consequitive range of messages.
	async fn relay_messages_range(data: RelayMessagesRangeParams) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let target_client = data.target.into_client::<Self::Target>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;
		let source_transactions_mortality = data.source_sign.transactions_mortality()?;
		let target_sign = data.target_sign.to_keypair::<Self::Target>()?;
		let target_transactions_mortality = data.target_sign.transactions_mortality()?;
		let lane_id = HexLaneId::try_convert(data.lane).map_err(|invalid_lane_id| {
			anyhow::format_err!("Invalid laneId: {:?}!", invalid_lane_id)
		})?;

		let at_source_block = source_client
			.header_by_number(data.at_source_block.unique_saturated_into())
			.await
			.map_err(|e| {
				log::trace!(
					target: "bridge",
					"Failed to read {} header with number {}: {e:?}",
					Self::Source::NAME,
					data.at_source_block,
				);
				anyhow::format_err!("The command has failed")
			})?
			.id();

		crate::messages::relay_messages_range::<Self::MessagesLane>(
			source_client,
			target_client,
			TransactionParams { signer: source_sign, mortality: source_transactions_mortality },
			TransactionParams { signer: target_sign, mortality: target_transactions_mortality },
			at_source_block,
			lane_id,
			data.messages_start..=data.messages_end,
			data.outbound_state_proof_required,
		)
		.await
	}

	/// Relay a messages delivery confirmation.
	async fn relay_messages_delivery_confirmation(
		data: RelayMessagesDeliveryConfirmationParams,
	) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let target_client = data.target.into_client::<Self::Target>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;
		let source_transactions_mortality = data.source_sign.transactions_mortality()?;
		let lane_id = HexLaneId::try_convert(data.lane).map_err(|invalid_lane_id| {
			anyhow::format_err!("Invalid laneId: {:?}!", invalid_lane_id)
		})?;

		let at_target_block = target_client
			.header_by_number(data.at_target_block.unique_saturated_into())
			.await
			.map_err(|e| {
				log::trace!(
					target: "bridge",
					"Failed to read {} header with number {}: {e:?}",
					Self::Target::NAME,
					data.at_target_block,
				);
				anyhow::format_err!("The command has failed")
			})?
			.id();

		crate::messages::relay_messages_delivery_confirmation::<Self::MessagesLane>(
			source_client,
			target_client,
			TransactionParams { signer: source_sign, mortality: source_transactions_mortality },
			at_target_block,
			lane_id,
		)
		.await
	}

	/// Add relay guards if required.
	async fn start_relay_guards(
		target_client: &impl Client<Self::Target>,
		enable_version_guard: bool,
	) -> relay_substrate_client::Result<()> {
		if enable_version_guard {
			relay_substrate_client::guard::abort_on_spec_version_change(
				target_client.clone(),
				target_client.simple_runtime_version().await?.spec_version,
			);
		}
		Ok(())
	}
}
