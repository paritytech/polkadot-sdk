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

//! Substrate client as Substrate messages target. The chain we connect to should have
//! runtime that implements `<BridgedChainName>HeaderApi` to allow bridging with
//! <BridgedName> chain.

use crate::messages_lane::{StandaloneMessagesMetrics, SubstrateMessageLane};
use crate::messages_source::{read_client_state, SubstrateMessagesProof};
use crate::on_demand_headers::OnDemandHeadersRelay;

use async_trait::async_trait;
use bp_messages::{LaneId, MessageNonce, UnrewardedRelayersState};
use bridge_runtime_common::messages::{
	source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
};
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use messages_relay::message_lane::MessageLane;
use messages_relay::{
	message_lane::{SourceHeaderIdOf, TargetHeaderIdOf},
	message_lane_loop::{TargetClient, TargetClientState},
};
use num_traits::{Bounded, Zero};
use relay_substrate_client::{Chain, Client, Error as SubstrateError, HashOf};
use relay_utils::{relay_loop::Client as RelayClient, BlockNumberBase, HeaderId};
use sp_core::Bytes;
use sp_runtime::{traits::Header as HeaderT, DeserializeOwned, FixedPointNumber, FixedU128};
use std::{convert::TryFrom, ops::RangeInclusive};

/// Message receiving proof returned by the target Substrate node.
pub type SubstrateMessagesReceivingProof<C> = (
	UnrewardedRelayersState,
	FromBridgedChainMessagesDeliveryProof<HashOf<C>>,
);

/// Substrate client as Substrate messages target.
pub struct SubstrateMessagesTarget<SC: Chain, TC: Chain, P: SubstrateMessageLane> {
	client: Client<TC>,
	lane: P,
	lane_id: LaneId,
	metric_values: StandaloneMessagesMetrics,
	source_to_target_headers_relay: Option<OnDemandHeadersRelay<SC>>,
}

impl<SC: Chain, TC: Chain, P: SubstrateMessageLane> SubstrateMessagesTarget<SC, TC, P> {
	/// Create new Substrate headers target.
	pub fn new(
		client: Client<TC>,
		lane: P,
		lane_id: LaneId,
		metric_values: StandaloneMessagesMetrics,
		source_to_target_headers_relay: Option<OnDemandHeadersRelay<SC>>,
	) -> Self {
		SubstrateMessagesTarget {
			client,
			lane,
			lane_id,
			metric_values,
			source_to_target_headers_relay,
		}
	}
}

impl<SC: Chain, TC: Chain, P: SubstrateMessageLane> Clone for SubstrateMessagesTarget<SC, TC, P> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			lane: self.lane.clone(),
			lane_id: self.lane_id,
			metric_values: self.metric_values.clone(),
			source_to_target_headers_relay: self.source_to_target_headers_relay.clone(),
		}
	}
}

#[async_trait]
impl<SC, TC, P> RelayClient for SubstrateMessagesTarget<SC, TC, P>
where
	SC: Chain,
	TC: Chain,
	P: SubstrateMessageLane,
{
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<SC, TC, P> TargetClient<P::MessageLane> for SubstrateMessagesTarget<SC, TC, P>
where
	SC: Chain<
		Hash = <P::MessageLane as MessageLane>::SourceHeaderHash,
		BlockNumber = <P::MessageLane as MessageLane>::SourceHeaderNumber,
		Balance = <P::MessageLane as MessageLane>::SourceChainBalance,
	>,
	SC::Balance: TryFrom<TC::Balance> + Bounded,
	TC: Chain<
		Hash = <P::MessageLane as MessageLane>::TargetHeaderHash,
		BlockNumber = <P::MessageLane as MessageLane>::TargetHeaderNumber,
	>,
	TC::Hash: Copy,
	TC::BlockNumber: Copy,
	TC::Header: DeserializeOwned,
	TC::Index: DeserializeOwned,
	<TC::Header as HeaderT>::Number: BlockNumberBase,
	P: SubstrateMessageLane<SourceChain = SC, TargetChain = TC>,
	P::MessageLane: MessageLane<
		MessagesProof = SubstrateMessagesProof<SC>,
		MessagesReceivingProof = SubstrateMessagesReceivingProof<TC>,
	>,
	<P::MessageLane as MessageLane>::SourceHeaderNumber: Decode,
	<P::MessageLane as MessageLane>::SourceHeaderHash: Decode,
{
	async fn state(&self) -> Result<TargetClientState<P::MessageLane>, SubstrateError> {
		// we can't continue to deliver messages if target node is out of sync, because
		// it may have already received (some of) messages that we're going to deliver
		self.client.ensure_synced().await?;

		read_client_state::<
			_,
			<P::MessageLane as MessageLane>::SourceHeaderHash,
			<P::MessageLane as MessageLane>::SourceHeaderNumber,
		>(&self.client, P::BEST_FINALIZED_SOURCE_HEADER_ID_AT_TARGET)
		.await
	}

	async fn latest_received_nonce(
		&self,
		id: TargetHeaderIdOf<P::MessageLane>,
	) -> Result<(TargetHeaderIdOf<P::MessageLane>, MessageNonce), SubstrateError> {
		let encoded_response = self
			.client
			.state_call(
				P::INBOUND_LANE_LATEST_RECEIVED_NONCE_METHOD.into(),
				Bytes(self.lane_id.encode()),
				Some(id.1),
			)
			.await?;
		let latest_received_nonce: MessageNonce =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
		Ok((id, latest_received_nonce))
	}

	async fn latest_confirmed_received_nonce(
		&self,
		id: TargetHeaderIdOf<P::MessageLane>,
	) -> Result<(TargetHeaderIdOf<P::MessageLane>, MessageNonce), SubstrateError> {
		let encoded_response = self
			.client
			.state_call(
				P::INBOUND_LANE_LATEST_CONFIRMED_NONCE_METHOD.into(),
				Bytes(self.lane_id.encode()),
				Some(id.1),
			)
			.await?;
		let latest_received_nonce: MessageNonce =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
		Ok((id, latest_received_nonce))
	}

	async fn unrewarded_relayers_state(
		&self,
		id: TargetHeaderIdOf<P::MessageLane>,
	) -> Result<(TargetHeaderIdOf<P::MessageLane>, UnrewardedRelayersState), SubstrateError> {
		let encoded_response = self
			.client
			.state_call(
				P::INBOUND_LANE_UNREWARDED_RELAYERS_STATE.into(),
				Bytes(self.lane_id.encode()),
				Some(id.1),
			)
			.await?;
		let unrewarded_relayers_state: UnrewardedRelayersState =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
		Ok((id, unrewarded_relayers_state))
	}

	async fn prove_messages_receiving(
		&self,
		id: TargetHeaderIdOf<P::MessageLane>,
	) -> Result<
		(
			TargetHeaderIdOf<P::MessageLane>,
			<P::MessageLane as MessageLane>::MessagesReceivingProof,
		),
		SubstrateError,
	> {
		let (id, relayers_state) = self.unrewarded_relayers_state(id).await?;
		let inbound_data_key = pallet_bridge_messages::storage_keys::inbound_lane_data_key(
			P::MESSAGE_PALLET_NAME_AT_TARGET,
			&self.lane_id,
		);
		let proof = self
			.client
			.prove_storage(vec![inbound_data_key], id.1)
			.await?
			.iter_nodes()
			.collect();
		let proof = FromBridgedChainMessagesDeliveryProof {
			bridged_header_hash: id.1,
			storage_proof: proof,
			lane: self.lane_id,
		};
		Ok((id, (relayers_state, proof)))
	}

	async fn submit_messages_proof(
		&self,
		generated_at_header: SourceHeaderIdOf<P::MessageLane>,
		nonces: RangeInclusive<MessageNonce>,
		proof: <P::MessageLane as MessageLane>::MessagesProof,
	) -> Result<RangeInclusive<MessageNonce>, SubstrateError> {
		let lane = self.lane.clone();
		let nonces_clone = nonces.clone();
		self.client
			.submit_signed_extrinsic(self.lane.target_transactions_author(), move |transaction_nonce| {
				lane.make_messages_delivery_transaction(transaction_nonce, generated_at_header, nonces_clone, proof)
			})
			.await?;
		Ok(nonces)
	}

	async fn require_source_header_on_target(&self, id: SourceHeaderIdOf<P::MessageLane>) {
		if let Some(ref source_to_target_headers_relay) = self.source_to_target_headers_relay {
			source_to_target_headers_relay.require_finalized_header(id).await;
		}
	}

	async fn estimate_delivery_transaction_in_source_tokens(
		&self,
		nonces: RangeInclusive<MessageNonce>,
		total_dispatch_weight: Weight,
		total_size: u32,
	) -> Result<<P::MessageLane as MessageLane>::SourceChainBalance, SubstrateError> {
		let conversion_rate = self
			.metric_values
			.target_to_source_conversion_rate()
			.await
			.ok_or_else(|| {
				SubstrateError::Custom(format!(
					"Failed to compute conversion rate from {} to {}",
					TC::NAME,
					SC::NAME,
				))
			})?;
		log::trace!(
			target: "bridge",
			"Using conversion rate {} when converting from {} tokens to {} tokens",
			conversion_rate,
			TC::NAME,
			SC::NAME
		);
		Ok(convert_target_tokens_to_source_tokens::<SC, TC>(
			FixedU128::from_float(conversion_rate),
			self.client
				.estimate_extrinsic_fee(self.lane.make_messages_delivery_transaction(
					Zero::zero(),
					HeaderId(Default::default(), Default::default()),
					nonces.clone(),
					prepare_dummy_messages_proof::<SC>(nonces, total_dispatch_weight, total_size),
				))
				.await
				.unwrap_or_else(|_| TC::Balance::max_value()),
		))
	}
}

/// Prepare 'dummy' messages proof that will compose the delivery transaction.
///
/// We don't care about proof actually being the valid proof, because its validity doesn't
/// affect the call weight - we only care about its size.
fn prepare_dummy_messages_proof<SC: Chain>(
	nonces: RangeInclusive<MessageNonce>,
	total_dispatch_weight: Weight,
	total_size: u32,
) -> SubstrateMessagesProof<SC> {
	(
		total_dispatch_weight,
		FromBridgedChainMessagesProof {
			bridged_header_hash: Default::default(),
			storage_proof: vec![vec![0; SC::STORAGE_PROOF_OVERHEAD.saturating_add(total_size) as usize]],
			lane: Default::default(),
			nonces_start: *nonces.start(),
			nonces_end: *nonces.end(),
		},
	)
}

/// Given delivery transaction fee in target chain tokens and conversion rate to the source
/// chain tokens, compute transaction cost in source chain tokens.
fn convert_target_tokens_to_source_tokens<SC: Chain, TC: Chain>(
	target_to_source_conversion_rate: FixedU128,
	target_transaction_fee: TC::Balance,
) -> SC::Balance
where
	SC::Balance: TryFrom<TC::Balance>,
{
	SC::Balance::try_from(target_to_source_conversion_rate.saturating_mul_int(target_transaction_fee))
		.unwrap_or_else(|_| SC::Balance::max_value())
}

#[cfg(test)]
mod tests {
	use super::*;
	use relay_millau_client::Millau;
	use relay_rialto_client::Rialto;

	#[test]
	fn prepare_dummy_messages_proof_works() {
		const DISPATCH_WEIGHT: Weight = 1_000_000;
		const SIZE: u32 = 1_000;
		let dummy_proof = prepare_dummy_messages_proof::<Rialto>(1..=10, DISPATCH_WEIGHT, SIZE);
		assert_eq!(dummy_proof.0, DISPATCH_WEIGHT);
		assert!(
			dummy_proof.1.encode().len() as u32 > SIZE,
			"Expected proof size at least {}. Got: {}",
			SIZE,
			dummy_proof.1.encode().len(),
		);
	}

	#[test]
	fn convert_target_tokens_to_source_tokens_works() {
		assert_eq!(
			convert_target_tokens_to_source_tokens::<Rialto, Millau>((150, 100).into(), 1_000),
			1_500
		);
		assert_eq!(
			convert_target_tokens_to_source_tokens::<Rialto, Millau>((50, 100).into(), 1_000),
			500
		);
		assert_eq!(
			convert_target_tokens_to_source_tokens::<Rialto, Millau>((100, 100).into(), 1_000),
			1_000
		);
	}
}
