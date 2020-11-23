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

//! Millau-to-Rialto messages sync entrypoint.

use crate::messages_lane::SubstrateMessageLane;
use crate::messages_source::SubstrateMessagesSource;
use crate::messages_target::SubstrateMessagesTarget;
use crate::{MillauClient, RialtoClient};

use async_trait::async_trait;
use bp_message_lane::{LaneId, MessageNonce};
use bp_runtime::{MILLAU_BRIDGE_INSTANCE, RIALTO_BRIDGE_INSTANCE};
use frame_support::weights::Weight;
use messages_relay::message_lane::MessageLane;
use relay_millau_client::{HeaderId as MillauHeaderId, Millau, SigningParams as MillauSigningParams};
use relay_rialto_client::{HeaderId as RialtoHeaderId, Rialto, SigningParams as RialtoSigningParams};
use relay_substrate_client::{BlockNumberOf, Error as SubstrateError, HashOf, TransactionSignScheme};
use relay_utils::metrics::MetricsParams;
use sp_core::Pair;
use sp_trie::StorageProof;
use std::{ops::RangeInclusive, time::Duration};

/// Millau -> Rialto messages proof:
///
/// - cumulative dispatch-weight of messages in the batch;
/// - proof that we'll actually submit to the Rialto node.
type FromMillauMessagesProof = (
	Weight,
	(HashOf<Millau>, StorageProof, LaneId, MessageNonce, MessageNonce),
);
/// Rialto -> Millau messages receiving proof.
type FromRialtoMessagesReceivingProof = (HashOf<Rialto>, StorageProof, LaneId);

/// Millau-to-Rialto messages pipeline.
#[derive(Debug, Clone)]
struct MillauMessagesToRialto {
	millau_client: MillauClient,
	millau_sign: MillauSigningParams,
	rialto_client: RialtoClient,
	rialto_sign: RialtoSigningParams,
	relayer_id: bp_millau::AccountId,
}

impl MessageLane for MillauMessagesToRialto {
	const SOURCE_NAME: &'static str = "Millau";
	const TARGET_NAME: &'static str = "Rialto";

	type MessagesProof = FromMillauMessagesProof;
	type MessagesReceivingProof = FromRialtoMessagesReceivingProof;

	type SourceHeaderNumber = BlockNumberOf<Millau>;
	type SourceHeaderHash = HashOf<Millau>;

	type TargetHeaderNumber = BlockNumberOf<Rialto>;
	type TargetHeaderHash = HashOf<Rialto>;
}

#[async_trait]
impl SubstrateMessageLane for MillauMessagesToRialto {
	const OUTBOUND_LANE_MESSAGES_DISPATCH_WEIGHT_METHOD: &'static str =
		bp_rialto::TO_RIALTO_MESSAGES_DISPATCH_WEIGHT_METHOD;
	const OUTBOUND_LANE_LATEST_GENERATED_NONCE_METHOD: &'static str =
		bp_rialto::TO_RIALTO_LATEST_GENERATED_NONCE_METHOD;
	const OUTBOUND_LANE_LATEST_RECEIVED_NONCE_METHOD: &'static str = bp_rialto::TO_RIALTO_LATEST_RECEIVED_NONCE_METHOD;

	const INBOUND_LANE_LATEST_RECEIVED_NONCE_METHOD: &'static str = bp_millau::FROM_MILLAU_LATEST_RECEIVED_NONCE_METHOD;
	const INBOUND_LANE_LATEST_CONFIRMED_NONCE_METHOD: &'static str =
		bp_millau::FROM_MILLAU_LATEST_CONFIRMED_NONCE_METHOD;

	const BEST_FINALIZED_SOURCE_HEADER_ID_AT_TARGET: &'static str = bp_millau::FINALIZED_MILLAU_BLOCK_METHOD;
	const BEST_FINALIZED_TARGET_HEADER_ID_AT_SOURCE: &'static str = bp_rialto::FINALIZED_RIALTO_BLOCK_METHOD;

	type SourceSignedTransaction = <Millau as TransactionSignScheme>::SignedTransaction;
	type TargetSignedTransaction = <Rialto as TransactionSignScheme>::SignedTransaction;

	async fn make_messages_receiving_proof_transaction(
		&self,
		_generated_at_block: RialtoHeaderId,
		proof: FromRialtoMessagesReceivingProof,
	) -> Result<Self::SourceSignedTransaction, SubstrateError> {
		let account_id = self.millau_sign.signer.public().as_array_ref().clone().into();
		let nonce = self.millau_client.next_account_index(account_id).await?;
		let call = millau_runtime::MessageLaneCall::receive_messages_delivery_proof(proof).into();
		let transaction = Millau::sign_transaction(&self.millau_client, &self.millau_sign.signer, nonce, call);
		Ok(transaction)
	}

	async fn make_messages_delivery_transaction(
		&self,
		_generated_at_header: MillauHeaderId,
		_nonces: RangeInclusive<MessageNonce>,
		proof: FromMillauMessagesProof,
	) -> Result<Self::TargetSignedTransaction, SubstrateError> {
		let (dispatch_weight, proof) = proof;
		let account_id = self.rialto_sign.signer.public().as_array_ref().clone().into();
		let nonce = self.rialto_client.next_account_index(account_id).await?;
		let call =
			rialto_runtime::MessageLaneCall::receive_messages_proof(self.relayer_id.clone(), proof, dispatch_weight)
				.into();
		let transaction = Rialto::sign_transaction(&self.rialto_client, &self.rialto_sign.signer, nonce, call);
		Ok(transaction)
	}
}

/// Millau node as messages source.
type MillauSourceClient = SubstrateMessagesSource<Millau, MillauMessagesToRialto>;

/// Rialto node as messages target.
type RialtoTargetClient = SubstrateMessagesTarget<Rialto, MillauMessagesToRialto>;

/// Run Millau-to-Rialto messages sync.
pub fn run(
	millau_client: MillauClient,
	millau_sign: MillauSigningParams,
	rialto_client: RialtoClient,
	rialto_sign: RialtoSigningParams,
	lane_id: LaneId,
	metrics_params: Option<MetricsParams>,
) {
	let millau_tick = Duration::from_secs(5);
	let rialto_tick = Duration::from_secs(5);
	let reconnect_delay = Duration::from_secs(10);
	let stall_timeout = Duration::from_secs(5 * 60);
	let relayer_id = millau_sign.signer.public().as_array_ref().clone().into();

	let lane = MillauMessagesToRialto {
		millau_client: millau_client.clone(),
		millau_sign,
		rialto_client: rialto_client.clone(),
		rialto_sign,
		relayer_id,
	};

	messages_relay::message_lane_loop::run(
		messages_relay::message_lane_loop::Params {
			lane: lane_id,
			source_tick: millau_tick,
			target_tick: rialto_tick,
			reconnect_delay,
			stall_timeout,
			delivery_params: messages_relay::message_lane_loop::MessageDeliveryParams {
				max_unconfirmed_nonces_at_target: bp_rialto::MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE,
				// TODO: subtract base weight of delivery from this when it'll be known
				// https://github.com/paritytech/parity-bridges-common/issues/78
				max_messages_weight_in_single_batch: bp_rialto::MAXIMUM_EXTRINSIC_WEIGHT,
			},
		},
		MillauSourceClient::new(millau_client, lane.clone(), lane_id, RIALTO_BRIDGE_INSTANCE),
		RialtoTargetClient::new(rialto_client, lane, lane_id, MILLAU_BRIDGE_INSTANCE),
		metrics_params,
		futures::future::pending(),
	);
}
