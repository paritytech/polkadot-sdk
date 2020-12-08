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

use crate::messages_source::SubstrateMessagesProof;
use crate::messages_target::SubstrateMessagesReceivingProof;

use async_trait::async_trait;
use bp_message_lane::MessageNonce;
use codec::Encode;
use messages_relay::message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf};
use relay_substrate_client::{BlockNumberOf, Chain, Client, Error as SubstrateError, HashOf};
use relay_utils::BlockNumberBase;
use std::ops::RangeInclusive;

/// Message sync pipeline for Substrate <-> Substrate relays.
#[async_trait]
pub trait SubstrateMessageLane: MessageLane {
	/// Name of the runtime method that returns dispatch weight of outbound messages at the source chain.
	const OUTBOUND_LANE_MESSAGES_DISPATCH_WEIGHT_METHOD: &'static str;
	/// Name of the runtime method that returns latest generated nonce at the source chain.
	const OUTBOUND_LANE_LATEST_GENERATED_NONCE_METHOD: &'static str;
	/// Name of the runtime method that returns latest received (confirmed) nonce at the the source chain.
	const OUTBOUND_LANE_LATEST_RECEIVED_NONCE_METHOD: &'static str;

	/// Name of the runtime method that returns latest received nonce at the target chain.
	const INBOUND_LANE_LATEST_RECEIVED_NONCE_METHOD: &'static str;
	/// Name of the runtime method that returns latest confirmed (reward-paid) nonce at the target chain.
	const INBOUND_LANE_LATEST_CONFIRMED_NONCE_METHOD: &'static str;
	/// Numebr of the runtime method that returns state of "unrewarded relayers" set at the target chain.
	const INBOUND_LANE_UNREWARDED_RELAYERS_STATE: &'static str;

	/// Name of the runtime method that returns id of best finalized source header at target chain.
	const BEST_FINALIZED_SOURCE_HEADER_ID_AT_TARGET: &'static str;
	/// Name of the runtime method that returns id of best finalized target header at source chain.
	const BEST_FINALIZED_TARGET_HEADER_ID_AT_SOURCE: &'static str;

	/// Signed transaction type of the source chain.
	type SourceSignedTransaction: Send + Sync + Encode;
	/// Signed transaction type of the target chain.
	type TargetSignedTransaction: Send + Sync + Encode;

	/// Make messages delivery transaction.
	async fn make_messages_delivery_transaction(
		&self,
		generated_at_header: SourceHeaderIdOf<Self>,
		nonces: RangeInclusive<MessageNonce>,
		proof: Self::MessagesProof,
	) -> Result<Self::TargetSignedTransaction, SubstrateError>;

	/// Make messages receiving proof transaction.
	async fn make_messages_receiving_proof_transaction(
		&self,
		generated_at_header: TargetHeaderIdOf<Self>,
		proof: Self::MessagesReceivingProof,
	) -> Result<Self::SourceSignedTransaction, SubstrateError>;
}

/// Substrate-to-Substrate message lane.
#[derive(Debug)]
pub struct SubstrateMessageLaneToSubstrate<Source: Chain, SourceSignParams, Target: Chain, TargetSignParams> {
	/// Client for the source Substrate chain.
	pub(crate) source_client: Client<Source>,
	/// Parameters required to sign transactions for source chain.
	pub(crate) source_sign: SourceSignParams,
	/// Client for the target Substrate chain.
	pub(crate) target_client: Client<Target>,
	/// Parameters required to sign transactions for target chain.
	pub(crate) target_sign: TargetSignParams,
	/// Account id of relayer at the source chain.
	pub(crate) relayer_id_at_source: Source::AccountId,
}

impl<Source: Chain, SourceSignParams: Clone, Target: Chain, TargetSignParams: Clone> Clone
	for SubstrateMessageLaneToSubstrate<Source, SourceSignParams, Target, TargetSignParams>
{
	fn clone(&self) -> Self {
		Self {
			source_client: self.source_client.clone(),
			source_sign: self.source_sign.clone(),
			target_client: self.target_client.clone(),
			target_sign: self.target_sign.clone(),
			relayer_id_at_source: self.relayer_id_at_source.clone(),
		}
	}
}

impl<Source: Chain, SourceSignParams, Target: Chain, TargetSignParams> MessageLane
	for SubstrateMessageLaneToSubstrate<Source, SourceSignParams, Target, TargetSignParams>
where
	SourceSignParams: Clone + Send + Sync + 'static,
	TargetSignParams: Clone + Send + Sync + 'static,
	BlockNumberOf<Source>: BlockNumberBase,
	BlockNumberOf<Target>: BlockNumberBase,
{
	const SOURCE_NAME: &'static str = Source::NAME;
	const TARGET_NAME: &'static str = Target::NAME;

	type MessagesProof = SubstrateMessagesProof<Source>;
	type MessagesReceivingProof = SubstrateMessagesReceivingProof<Target>;

	type SourceHeaderNumber = BlockNumberOf<Source>;
	type SourceHeaderHash = HashOf<Source>;

	type TargetHeaderNumber = BlockNumberOf<Target>;
	type TargetHeaderHash = HashOf<Target>;
}
