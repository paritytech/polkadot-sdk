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

use async_trait::async_trait;
use bp_message_lane::MessageNonce;
use codec::Encode;
use messages_relay::message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf};
use relay_substrate_client::Error as SubstrateError;
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

	/// Name of the runtime method that returns latest received nonce at the source chain.
	const INBOUND_LANE_LATEST_RECEIVED_NONCE_METHOD: &'static str;
	/// Name of the runtime method that returns latest confirmed (reward-paid) nonce at the source chain.
	const INBOUND_LANE_LATEST_CONFIRMED_NONCE_METHOD: &'static str;

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
