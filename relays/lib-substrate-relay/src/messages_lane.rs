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

//! Tools for supporting message lanes between two Substrate-based chains.

use crate::{
	messages_source::{SubstrateMessagesProof, SubstrateMessagesSource},
	messages_target::{SubstrateMessagesDeliveryProof, SubstrateMessagesTarget},
	on_demand::OnDemandRelay,
	BatchCallBuilder, BatchCallBuilderConstructor, TransactionParams,
};

use async_std::sync::Arc;
use bp_messages::{LaneId, MessageNonce};
use bp_runtime::{
	AccountIdOf, Chain as _, EncodedOrDecodedCall, HeaderIdOf, TransactionEra, WeightExtraOps,
};
use bridge_runtime_common::messages::{
	source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
};
use codec::Encode;
use frame_support::{dispatch::GetDispatchInfo, weights::Weight};
use messages_relay::{message_lane::MessageLane, message_lane_loop::BatchTransaction};
use pallet_bridge_messages::{Call as BridgeMessagesCall, Config as BridgeMessagesConfig};
use relay_substrate_client::{
	transaction_stall_timeout, AccountKeyPairOf, BalanceOf, BlockNumberOf, CallOf, Chain,
	ChainWithMessages, ChainWithTransactions, Client, Error as SubstrateError, HashOf, SignParam,
	UnsignedTransaction,
};
use relay_utils::{
	metrics::{GlobalMetrics, MetricsParams, StandaloneMetric},
	STALL_TIMEOUT,
};
use sp_core::Pair;
use sp_runtime::traits::Zero;
use std::{convert::TryFrom, fmt::Debug, marker::PhantomData};

/// Substrate -> Substrate messages synchronization pipeline.
pub trait SubstrateMessageLane: 'static + Clone + Debug + Send + Sync {
	/// Messages of this chain are relayed to the `TargetChain`.
	type SourceChain: ChainWithMessages + ChainWithTransactions;
	/// Messages from the `SourceChain` are dispatched on this chain.
	type TargetChain: ChainWithMessages + ChainWithTransactions;

	/// How receive messages proof call is built?
	type ReceiveMessagesProofCallBuilder: ReceiveMessagesProofCallBuilder<Self>;
	/// How receive messages delivery proof call is built?
	type ReceiveMessagesDeliveryProofCallBuilder: ReceiveMessagesDeliveryProofCallBuilder<Self>;

	/// How batch calls are built at the source chain?
	type SourceBatchCallBuilder: BatchCallBuilderConstructor<CallOf<Self::SourceChain>>;
	/// How batch calls are built at the target chain?
	type TargetBatchCallBuilder: BatchCallBuilderConstructor<CallOf<Self::TargetChain>>;
}

/// Adapter that allows all `SubstrateMessageLane` to act as `MessageLane`.
#[derive(Clone, Debug)]
pub struct MessageLaneAdapter<P: SubstrateMessageLane> {
	_phantom: PhantomData<P>,
}

impl<P: SubstrateMessageLane> MessageLane for MessageLaneAdapter<P> {
	const SOURCE_NAME: &'static str = P::SourceChain::NAME;
	const TARGET_NAME: &'static str = P::TargetChain::NAME;

	type MessagesProof = SubstrateMessagesProof<P::SourceChain>;
	type MessagesReceivingProof = SubstrateMessagesDeliveryProof<P::TargetChain>;

	type SourceChainBalance = BalanceOf<P::SourceChain>;
	type SourceHeaderNumber = BlockNumberOf<P::SourceChain>;
	type SourceHeaderHash = HashOf<P::SourceChain>;

	type TargetHeaderNumber = BlockNumberOf<P::TargetChain>;
	type TargetHeaderHash = HashOf<P::TargetChain>;
}

/// Substrate <-> Substrate messages relay parameters.
pub struct MessagesRelayParams<P: SubstrateMessageLane> {
	/// Messages source client.
	pub source_client: Client<P::SourceChain>,
	/// Source transaction params.
	pub source_transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	/// Messages target client.
	pub target_client: Client<P::TargetChain>,
	/// Target transaction params.
	pub target_transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	/// Optional on-demand source to target headers relay.
	pub source_to_target_headers_relay:
		Option<Arc<dyn OnDemandRelay<P::SourceChain, P::TargetChain>>>,
	/// Optional on-demand target to source headers relay.
	pub target_to_source_headers_relay:
		Option<Arc<dyn OnDemandRelay<P::TargetChain, P::SourceChain>>>,
	/// Identifier of lane that needs to be served.
	pub lane_id: LaneId,
	/// Metrics parameters.
	pub metrics_params: MetricsParams,
}

/// Batch transaction that brings headers + and messages delivery/receiving confirmations to the
/// source node.
pub struct BatchProofTransaction<SC: Chain, TC: Chain, B: BatchCallBuilderConstructor<CallOf<SC>>> {
	builder: Box<dyn BatchCallBuilder<CallOf<SC>>>,
	proved_header: HeaderIdOf<TC>,
	prove_calls: Vec<CallOf<SC>>,

	/// Using `fn() -> B` in order to avoid implementing `Send` for `B`.
	_phantom: PhantomData<fn() -> B>,
}

impl<SC: Chain, TC: Chain, B: BatchCallBuilderConstructor<CallOf<SC>>>
	BatchProofTransaction<SC, TC, B>
{
	/// Creates a new instance of `BatchProofTransaction`.
	pub async fn new(
		relay: Arc<dyn OnDemandRelay<TC, SC>>,
		block_num: BlockNumberOf<TC>,
	) -> Result<Option<Self>, SubstrateError> {
		if let Some(builder) = B::new_builder() {
			let (proved_header, prove_calls) = relay.prove_header(block_num).await?;
			return Ok(Some(Self {
				builder,
				proved_header,
				prove_calls,
				_phantom: Default::default(),
			}))
		}

		Ok(None)
	}

	/// Return a batch call that includes the provided call.
	pub fn append_call_and_build(mut self, call: CallOf<SC>) -> CallOf<SC> {
		self.prove_calls.push(call);
		self.builder.build_batch_call(self.prove_calls)
	}
}

impl<SC: Chain, TC: Chain, B: BatchCallBuilderConstructor<CallOf<SC>>>
	BatchTransaction<HeaderIdOf<TC>> for BatchProofTransaction<SC, TC, B>
{
	fn required_header_id(&self) -> HeaderIdOf<TC> {
		self.proved_header
	}
}

/// Run Substrate-to-Substrate messages sync loop.
pub async fn run<P: SubstrateMessageLane>(params: MessagesRelayParams<P>) -> anyhow::Result<()>
where
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
	BalanceOf<P::SourceChain>: TryFrom<BalanceOf<P::TargetChain>>,
{
	// 2/3 is reserved for proofs and tx overhead
	let max_messages_size_in_single_batch = P::TargetChain::max_extrinsic_size() / 3;
	// we don't know exact weights of the Polkadot runtime. So to guess weights we'll be using
	// weights from Rialto and then simply dividing it by x2.
	let (max_messages_in_single_batch, max_messages_weight_in_single_batch) =
		select_delivery_transaction_limits_rpc::<P>(
			&params,
			P::TargetChain::max_extrinsic_weight(),
			P::SourceChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
		)
		.await?;
	let (max_messages_in_single_batch, max_messages_weight_in_single_batch) =
		(max_messages_in_single_batch / 2, max_messages_weight_in_single_batch / 2);

	let source_client = params.source_client;
	let target_client = params.target_client;
	let relayer_id_at_source: AccountIdOf<P::SourceChain> =
		params.source_transaction_params.signer.public().into();

	log::info!(
		target: "bridge",
		"Starting {} -> {} messages relay.\n\t\
			{} relayer account id: {:?}\n\t\
			Max messages in single transaction: {}\n\t\
			Max messages size in single transaction: {}\n\t\
			Max messages weight in single transaction: {}\n\t\
			Tx mortality: {:?} (~{}m)/{:?} (~{}m)",
		P::SourceChain::NAME,
		P::TargetChain::NAME,
		P::SourceChain::NAME,
		relayer_id_at_source,
		max_messages_in_single_batch,
		max_messages_size_in_single_batch,
		max_messages_weight_in_single_batch,
		params.source_transaction_params.mortality,
		transaction_stall_timeout(
			params.source_transaction_params.mortality,
			P::SourceChain::AVERAGE_BLOCK_INTERVAL,
			STALL_TIMEOUT,
		).as_secs_f64() / 60.0f64,
		params.target_transaction_params.mortality,
		transaction_stall_timeout(
			params.target_transaction_params.mortality,
			P::TargetChain::AVERAGE_BLOCK_INTERVAL,
			STALL_TIMEOUT,
		).as_secs_f64() / 60.0f64,
	);

	messages_relay::message_lane_loop::run(
		messages_relay::message_lane_loop::Params {
			lane: params.lane_id,
			source_tick: P::SourceChain::AVERAGE_BLOCK_INTERVAL,
			target_tick: P::TargetChain::AVERAGE_BLOCK_INTERVAL,
			reconnect_delay: relay_utils::relay_loop::RECONNECT_DELAY,
			delivery_params: messages_relay::message_lane_loop::MessageDeliveryParams {
				max_unrewarded_relayer_entries_at_target:
					P::SourceChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_nonces_at_target:
					P::SourceChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				max_messages_in_single_batch,
				max_messages_weight_in_single_batch,
				max_messages_size_in_single_batch,
			},
		},
		SubstrateMessagesSource::<P>::new(
			source_client.clone(),
			target_client.clone(),
			params.lane_id,
			params.source_transaction_params,
			params.target_to_source_headers_relay,
		),
		SubstrateMessagesTarget::<P>::new(
			target_client,
			source_client,
			params.lane_id,
			relayer_id_at_source,
			params.target_transaction_params,
			params.source_to_target_headers_relay,
		),
		{
			GlobalMetrics::new()?.register_and_spawn(&params.metrics_params.registry)?;
			params.metrics_params
		},
		futures::future::pending(),
	)
	.await
	.map_err(Into::into)
}

/// Different ways of building `receive_messages_proof` calls.
pub trait ReceiveMessagesProofCallBuilder<P: SubstrateMessageLane> {
	/// Given messages proof, build call of `receive_messages_proof` function of bridge
	/// messages module at the target chain.
	fn build_receive_messages_proof_call(
		relayer_id_at_source: AccountIdOf<P::SourceChain>,
		proof: SubstrateMessagesProof<P::SourceChain>,
		messages_count: u32,
		dispatch_weight: Weight,
		trace_call: bool,
	) -> CallOf<P::TargetChain>;
}

/// Building `receive_messages_proof` call when you have direct access to the target
/// chain runtime.
pub struct DirectReceiveMessagesProofCallBuilder<P, R, I> {
	_phantom: PhantomData<(P, R, I)>,
}

impl<P, R, I> ReceiveMessagesProofCallBuilder<P> for DirectReceiveMessagesProofCallBuilder<P, R, I>
where
	P: SubstrateMessageLane,
	R: BridgeMessagesConfig<I, InboundRelayer = AccountIdOf<P::SourceChain>>,
	I: 'static,
	R::SourceHeaderChain: bp_messages::target_chain::SourceHeaderChain<
		MessagesProof = FromBridgedChainMessagesProof<HashOf<P::SourceChain>>,
	>,
	CallOf<P::TargetChain>: From<BridgeMessagesCall<R, I>> + GetDispatchInfo,
{
	fn build_receive_messages_proof_call(
		relayer_id_at_source: AccountIdOf<P::SourceChain>,
		proof: SubstrateMessagesProof<P::SourceChain>,
		messages_count: u32,
		dispatch_weight: Weight,
		trace_call: bool,
	) -> CallOf<P::TargetChain> {
		let call: CallOf<P::TargetChain> = BridgeMessagesCall::<R, I>::receive_messages_proof {
			relayer_id_at_bridged_chain: relayer_id_at_source,
			proof: proof.1,
			messages_count,
			dispatch_weight,
		}
		.into();
		if trace_call {
			// this trace isn't super-accurate, because limits are for transactions and we
			// have a call here, but it provides required information
			log::trace!(
				target: "bridge",
				"Prepared {} -> {} messages delivery call. Weight: {}/{}, size: {}/{}",
				P::SourceChain::NAME,
				P::TargetChain::NAME,
				call.get_dispatch_info().weight,
				P::TargetChain::max_extrinsic_weight(),
				call.encode().len(),
				P::TargetChain::max_extrinsic_size(),
			);
		}
		call
	}
}

/// Macro that generates `ReceiveMessagesProofCallBuilder` implementation for the case when
/// you only have an access to the mocked version of target chain runtime. In this case you
/// should provide "name" of the call variant for the bridge messages calls and the "name" of
/// the variant for the `receive_messages_proof` call within that first option.
#[rustfmt::skip]
#[macro_export]
macro_rules! generate_receive_message_proof_call_builder {
	($pipeline:ident, $mocked_builder:ident, $bridge_messages:path, $receive_messages_proof:path) => {
		pub struct $mocked_builder;

		impl $crate::messages_lane::ReceiveMessagesProofCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_receive_messages_proof_call(
				relayer_id_at_source: relay_substrate_client::AccountIdOf<
					<$pipeline as $crate::messages_lane::SubstrateMessageLane>::SourceChain
				>,
				proof: $crate::messages_source::SubstrateMessagesProof<
					<$pipeline as $crate::messages_lane::SubstrateMessageLane>::SourceChain
				>,
				messages_count: u32,
				dispatch_weight: bp_messages::Weight,
				_trace_call: bool,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::messages_lane::SubstrateMessageLane>::TargetChain
			> {
				bp_runtime::paste::item! {
					$bridge_messages($receive_messages_proof {
						relayer_id_at_bridged_chain: relayer_id_at_source,
						proof: proof.1,
						messages_count: messages_count,
						dispatch_weight: dispatch_weight,
					})
				}
			}
		}
	};
}

/// Different ways of building `receive_messages_delivery_proof` calls.
pub trait ReceiveMessagesDeliveryProofCallBuilder<P: SubstrateMessageLane> {
	/// Given messages delivery proof, build call of `receive_messages_delivery_proof` function of
	/// bridge messages module at the source chain.
	fn build_receive_messages_delivery_proof_call(
		proof: SubstrateMessagesDeliveryProof<P::TargetChain>,
		trace_call: bool,
	) -> CallOf<P::SourceChain>;
}

/// Building `receive_messages_delivery_proof` call when you have direct access to the source
/// chain runtime.
pub struct DirectReceiveMessagesDeliveryProofCallBuilder<P, R, I> {
	_phantom: PhantomData<(P, R, I)>,
}

impl<P, R, I> ReceiveMessagesDeliveryProofCallBuilder<P>
	for DirectReceiveMessagesDeliveryProofCallBuilder<P, R, I>
where
	P: SubstrateMessageLane,
	R: BridgeMessagesConfig<I>,
	I: 'static,
	R::TargetHeaderChain: bp_messages::source_chain::TargetHeaderChain<
		R::OutboundPayload,
		R::AccountId,
		MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<HashOf<P::TargetChain>>,
	>,
	CallOf<P::SourceChain>: From<BridgeMessagesCall<R, I>> + GetDispatchInfo,
{
	fn build_receive_messages_delivery_proof_call(
		proof: SubstrateMessagesDeliveryProof<P::TargetChain>,
		trace_call: bool,
	) -> CallOf<P::SourceChain> {
		let call: CallOf<P::SourceChain> =
			BridgeMessagesCall::<R, I>::receive_messages_delivery_proof {
				proof: proof.1,
				relayers_state: proof.0,
			}
			.into();
		if trace_call {
			// this trace isn't super-accurate, because limits are for transactions and we
			// have a call here, but it provides required information
			log::trace!(
				target: "bridge",
				"Prepared {} -> {} delivery confirmation transaction. Weight: {}/{}, size: {}/{}",
				P::TargetChain::NAME,
				P::SourceChain::NAME,
				call.get_dispatch_info().weight,
				P::SourceChain::max_extrinsic_weight(),
				call.encode().len(),
				P::SourceChain::max_extrinsic_size(),
			);
		}
		call
	}
}

/// Macro that generates `ReceiveMessagesDeliveryProofCallBuilder` implementation for the case when
/// you only have an access to the mocked version of source chain runtime. In this case you
/// should provide "name" of the call variant for the bridge messages calls and the "name" of
/// the variant for the `receive_messages_delivery_proof` call within that first option.
#[rustfmt::skip]
#[macro_export]
macro_rules! generate_receive_message_delivery_proof_call_builder {
	($pipeline:ident, $mocked_builder:ident, $bridge_messages:path, $receive_messages_delivery_proof:path) => {
		pub struct $mocked_builder;

		impl $crate::messages_lane::ReceiveMessagesDeliveryProofCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_receive_messages_delivery_proof_call(
				proof: $crate::messages_target::SubstrateMessagesDeliveryProof<
					<$pipeline as $crate::messages_lane::SubstrateMessageLane>::TargetChain
				>,
				_trace_call: bool,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::messages_lane::SubstrateMessageLane>::SourceChain
			> {
				bp_runtime::paste::item! {
					$bridge_messages($receive_messages_delivery_proof {
						proof: proof.1,
						relayers_state: proof.0
					})
				}
			}
		}
	};
}

/// Returns maximal number of messages and their maximal cumulative dispatch weight.
async fn select_delivery_transaction_limits_rpc<P: SubstrateMessageLane>(
	params: &MessagesRelayParams<P>,
	max_extrinsic_weight: Weight,
	max_unconfirmed_messages_at_inbound_lane: MessageNonce,
) -> anyhow::Result<(MessageNonce, Weight)>
where
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
{
	// We may try to guess accurate value, based on maximal number of messages and per-message
	// weight overhead, but the relay loop isn't using this info in a super-accurate way anyway.
	// So just a rough guess: let's say 1/3 of max tx weight is for tx itself and the rest is
	// for messages dispatch.

	// Another thing to keep in mind is that our runtimes (when this code was written) accept
	// messages with dispatch weight <= max_extrinsic_weight/2. So we can't reserve less than
	// that for dispatch.

	let weight_for_delivery_tx = max_extrinsic_weight / 3;
	let weight_for_messages_dispatch = max_extrinsic_weight - weight_for_delivery_tx;

	// weight of empty message delivery with outbound lane state
	let delivery_tx_with_zero_messages = dummy_messages_delivery_transaction::<P>(params, 0)?;
	let delivery_tx_with_zero_messages_weight = params
		.target_client
		.extimate_extrinsic_weight(delivery_tx_with_zero_messages)
		.await
		.map_err(|e| {
			anyhow::format_err!("Failed to estimate delivery extrinsic weight: {:?}", e)
		})?;

	// weight of single message delivery with outbound lane state
	let delivery_tx_with_one_message = dummy_messages_delivery_transaction::<P>(params, 1)?;
	let delivery_tx_with_one_message_weight = params
		.target_client
		.extimate_extrinsic_weight(delivery_tx_with_one_message)
		.await
		.map_err(|e| {
			anyhow::format_err!("Failed to estimate delivery extrinsic weight: {:?}", e)
		})?;

	// message overhead is roughly `delivery_tx_with_one_message_weight -
	// delivery_tx_with_zero_messages_weight`
	let delivery_tx_weight_rest = weight_for_delivery_tx - delivery_tx_with_zero_messages_weight;
	let delivery_tx_message_overhead =
		delivery_tx_with_one_message_weight.saturating_sub(delivery_tx_with_zero_messages_weight);

	let max_number_of_messages = std::cmp::min(
		delivery_tx_weight_rest
			.min_components_checked_div(delivery_tx_message_overhead)
			.unwrap_or(u64::MAX),
		max_unconfirmed_messages_at_inbound_lane,
	);

	assert!(
		max_number_of_messages > 0,
		"Relay should fit at least one message in every delivery transaction",
	);
	assert!(
		weight_for_messages_dispatch.ref_time() >= max_extrinsic_weight.ref_time() / 2,
		"Relay shall be able to deliver messages with dispatch weight = max_extrinsic_weight / 2",
	);

	Ok((max_number_of_messages, weight_for_messages_dispatch))
}

/// Returns dummy message delivery transaction with zero messages and `1kb` proof.
fn dummy_messages_delivery_transaction<P: SubstrateMessageLane>(
	params: &MessagesRelayParams<P>,
	messages: u32,
) -> anyhow::Result<<P::TargetChain as ChainWithTransactions>::SignedTransaction>
where
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
{
	// we don't care about any call values here, because all that the estimation RPC does
	// is calls `GetDispatchInfo::get_dispatch_info` for the wrapped call. So we only are
	// interested in values that affect call weight - e.g. number of messages and the
	// storage proof size

	let dummy_messages_delivery_call =
		P::ReceiveMessagesProofCallBuilder::build_receive_messages_proof_call(
			params.source_transaction_params.signer.public().into(),
			(
				Weight::zero(),
				FromBridgedChainMessagesProof {
					bridged_header_hash: Default::default(),
					// we may use per-chain `EXTRA_STORAGE_PROOF_SIZE`, but since we don't need
					// exact values, this global estimation is fine
					storage_proof: vec![vec![
						42u8;
						pallet_bridge_messages::EXTRA_STORAGE_PROOF_SIZE
							as usize
					]],
					lane: Default::default(),
					nonces_start: 1,
					nonces_end: messages as u64,
				},
			),
			messages,
			Weight::zero(),
			false,
		);
	P::TargetChain::sign_transaction(
		SignParam {
			spec_version: 0,
			transaction_version: 0,
			genesis_hash: Default::default(),
			signer: params.target_transaction_params.signer.clone(),
		},
		UnsignedTransaction {
			call: EncodedOrDecodedCall::Decoded(dummy_messages_delivery_call),
			nonce: Zero::zero(),
			tip: Zero::zero(),
			era: TransactionEra::Immortal,
		},
	)
	.map_err(Into::into)
}
