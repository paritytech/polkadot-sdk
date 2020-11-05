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

//! Message receiving race delivers proof-of-messages-delivery from lane.target to lane.source.

use crate::message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf};
use crate::message_lane_loop::{
	SourceClient as MessageLaneSourceClient, SourceClientState, TargetClient as MessageLaneTargetClient,
	TargetClientState,
};
use crate::message_race_loop::{ClientNonces, MessageRace, SourceClient, TargetClient};
use crate::message_race_strategy::BasicStrategy;
use crate::metrics::MessageLaneLoopMetrics;

use async_trait::async_trait;
use futures::stream::FusedStream;
use relay_utils::FailedClient;
use std::{marker::PhantomData, ops::RangeInclusive, time::Duration};

/// Message receiving confirmations delivery strategy.
type ReceivingConfirmationsBasicStrategy<P> = BasicStrategy<
	<P as MessageLane>::TargetHeaderNumber,
	<P as MessageLane>::TargetHeaderHash,
	<P as MessageLane>::SourceHeaderNumber,
	<P as MessageLane>::SourceHeaderHash,
	<P as MessageLane>::MessageNonce,
	<P as MessageLane>::MessagesReceivingProof,
>;

/// Run receiving confirmations race.
pub async fn run<P: MessageLane>(
	source_client: impl MessageLaneSourceClient<P>,
	source_state_updates: impl FusedStream<Item = SourceClientState<P>>,
	target_client: impl MessageLaneTargetClient<P>,
	target_state_updates: impl FusedStream<Item = TargetClientState<P>>,
	stall_timeout: Duration,
	metrics_msg: Option<MessageLaneLoopMetrics>,
) -> Result<(), FailedClient> {
	crate::message_race_loop::run(
		ReceivingConfirmationsRaceSource {
			client: target_client,
			metrics_msg: metrics_msg.clone(),
			_phantom: Default::default(),
		},
		target_state_updates,
		ReceivingConfirmationsRaceTarget {
			client: source_client,
			metrics_msg,
			_phantom: Default::default(),
		},
		source_state_updates,
		stall_timeout,
		ReceivingConfirmationsBasicStrategy::<P>::new(std::u32::MAX.into()),
	)
	.await
}

/// Messages receiving confirmations race.
struct ReceivingConfirmationsRace<P>(std::marker::PhantomData<P>);

impl<P: MessageLane> MessageRace for ReceivingConfirmationsRace<P> {
	type SourceHeaderId = TargetHeaderIdOf<P>;
	type TargetHeaderId = SourceHeaderIdOf<P>;

	type MessageNonce = P::MessageNonce;
	type Proof = P::MessagesReceivingProof;

	fn source_name() -> String {
		format!("{}::ReceivingConfirmationsDelivery", P::SOURCE_NAME)
	}

	fn target_name() -> String {
		format!("{}::ReceivingConfirmationsDelivery", P::TARGET_NAME)
	}
}

/// Message receiving confirmations race source, which is a target of the lane.
struct ReceivingConfirmationsRaceSource<P: MessageLane, C> {
	client: C,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	_phantom: PhantomData<P>,
}

#[async_trait]
impl<P, C> SourceClient<ReceivingConfirmationsRace<P>> for ReceivingConfirmationsRaceSource<P, C>
where
	P: MessageLane,
	C: MessageLaneTargetClient<P>,
{
	type Error = C::Error;
	type ProofParameters = ();

	async fn nonces(
		&self,
		at_block: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, ClientNonces<P::MessageNonce>), Self::Error> {
		let (at_block, latest_received_nonce) = self.client.latest_received_nonce(at_block).await?;
		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_target_latest_received_nonce::<P>(latest_received_nonce);
		}
		Ok((
			at_block,
			ClientNonces {
				latest_nonce: latest_received_nonce,
				confirmed_nonce: None,
			},
		))
	}

	#[allow(clippy::unit_arg)]
	async fn generate_proof(
		&self,
		at_block: TargetHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		_proof_parameters: Self::ProofParameters,
	) -> Result<
		(
			TargetHeaderIdOf<P>,
			RangeInclusive<P::MessageNonce>,
			P::MessagesReceivingProof,
		),
		Self::Error,
	> {
		self.client
			.prove_messages_receiving(at_block)
			.await
			.map(|(at_block, proof)| (at_block, nonces, proof))
	}
}

/// Message receiving confirmations race target, which is a source of the lane.
struct ReceivingConfirmationsRaceTarget<P: MessageLane, C> {
	client: C,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	_phantom: PhantomData<P>,
}

#[async_trait]
impl<P, C> TargetClient<ReceivingConfirmationsRace<P>> for ReceivingConfirmationsRaceTarget<P, C>
where
	P: MessageLane,
	C: MessageLaneSourceClient<P>,
{
	type Error = C::Error;

	async fn nonces(
		&self,
		at_block: SourceHeaderIdOf<P>,
	) -> Result<(SourceHeaderIdOf<P>, ClientNonces<P::MessageNonce>), Self::Error> {
		let (at_block, latest_confirmed_nonce) = self.client.latest_confirmed_received_nonce(at_block).await?;
		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_source_latest_confirmed_nonce::<P>(latest_confirmed_nonce);
		}
		Ok((
			at_block,
			ClientNonces {
				latest_nonce: latest_confirmed_nonce,
				confirmed_nonce: None,
			},
		))
	}

	async fn submit_proof(
		&self,
		generated_at_block: TargetHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		proof: P::MessagesReceivingProof,
	) -> Result<RangeInclusive<P::MessageNonce>, Self::Error> {
		self.client
			.submit_messages_receiving_proof(generated_at_block, proof)
			.await?;
		Ok(nonces)
	}
}
