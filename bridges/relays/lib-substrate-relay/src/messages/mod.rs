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
	messages::{
		source::{SubstrateMessagesProof, SubstrateMessagesSource},
		target::{SubstrateMessagesDeliveryProof, SubstrateMessagesTarget},
	},
	on_demand::OnDemandRelay,
	BatchCallBuilder, BatchCallBuilderConstructor, TransactionParams,
};

use async_std::sync::Arc;
use bp_messages::{
	target_chain::FromBridgedChainMessagesProof, ChainWithMessages as _, MessageNonce,
};
use bp_runtime::{AccountIdOf, EncodedOrDecodedCall, HeaderIdOf, TransactionEra, WeightExtraOps};
use codec::{Codec, Encode, EncodeLike};
use frame_support::{dispatch::GetDispatchInfo, weights::Weight};
use messages_relay::{message_lane::MessageLane, message_lane_loop::BatchTransaction, Labeled};
use pallet_bridge_messages::{Call as BridgeMessagesCall, Config as BridgeMessagesConfig};
use relay_substrate_client::{
	transaction_stall_timeout, AccountKeyPairOf, BalanceOf, BlockNumberOf, CallOf, Chain,
	ChainBase, ChainWithMessages, ChainWithTransactions, Client, Error as SubstrateError, HashOf,
	SignParam, UnsignedTransaction,
};
use relay_utils::{
	metrics::{GlobalMetrics, MetricsParams, StandaloneMetric},
	STALL_TIMEOUT,
};
use sp_core::Pair;
use sp_runtime::traits::Zero;
use std::{fmt::Debug, marker::PhantomData, ops::RangeInclusive};

pub mod metrics;
pub mod source;
pub mod target;

/// Substrate -> Substrate messages synchronization pipeline.
pub trait SubstrateMessageLane: 'static + Clone + Debug + Send + Sync {
	/// Messages of this chain are relayed to the `TargetChain`.
	type SourceChain: ChainWithMessages + ChainWithTransactions;
	/// Messages from the `SourceChain` are dispatched on this chain.
	type TargetChain: ChainWithMessages + ChainWithTransactions;

	/// Lane identifier type.
	type LaneId: Clone
		+ Copy
		+ Debug
		+ Codec
		+ EncodeLike
		+ Send
		+ Sync
		+ Labeled
		+ TryFrom<Vec<u8>>
		+ Default;

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

	type LaneId = P::LaneId;

	type MessagesProof = SubstrateMessagesProof<P::SourceChain, P::LaneId>;
	type MessagesReceivingProof = SubstrateMessagesDeliveryProof<P::TargetChain, P::LaneId>;

	type SourceChainBalance = BalanceOf<P::SourceChain>;
	type SourceHeaderNumber = BlockNumberOf<P::SourceChain>;
	type SourceHeaderHash = HashOf<P::SourceChain>;

	type TargetHeaderNumber = BlockNumberOf<P::TargetChain>;
	type TargetHeaderHash = HashOf<P::TargetChain>;
}

/// Substrate <-> Substrate messages relay parameters.
pub struct MessagesRelayParams<P: SubstrateMessageLane, SourceClnt, TargetClnt> {
	/// Messages source client.
	pub source_client: SourceClnt,
	/// Source transaction params.
	pub source_transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	/// Messages target client.
	pub target_client: TargetClnt,
	/// Target transaction params.
	pub target_transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	/// Optional on-demand source to target headers relay.
	pub source_to_target_headers_relay:
		Option<Arc<dyn OnDemandRelay<P::SourceChain, P::TargetChain>>>,
	/// Optional on-demand target to source headers relay.
	pub target_to_source_headers_relay:
		Option<Arc<dyn OnDemandRelay<P::TargetChain, P::SourceChain>>>,
	/// Identifier of lane that needs to be served.
	pub lane_id: P::LaneId,
	/// Messages relay limits. If not provided, the relay tries to determine it automatically,
	/// using `TransactionPayment` pallet runtime API.
	pub limits: Option<MessagesRelayLimits>,
	/// Metrics parameters.
	pub metrics_params: MetricsParams,
}

/// Delivery transaction limits.
pub struct MessagesRelayLimits {
	/// Maximal number of messages in the delivery transaction.
	pub max_messages_in_single_batch: MessageNonce,
	/// Maximal cumulative weight of messages in the delivery transaction.
	pub max_messages_weight_in_single_batch: Weight,
}

/// Batch transaction that brings headers + and messages delivery/receiving confirmations to the
/// source node.
#[derive(Clone)]
pub struct BatchProofTransaction<SC: Chain, TC: Chain, B: BatchCallBuilderConstructor<CallOf<SC>>> {
	builder: B::CallBuilder,
	proved_header: HeaderIdOf<TC>,
	prove_calls: Vec<CallOf<SC>>,

	/// Using `fn() -> B` in order to avoid implementing `Send` for `B`.
	_phantom: PhantomData<fn() -> B>,
}

impl<SC: Chain, TC: Chain, B: BatchCallBuilderConstructor<CallOf<SC>>> std::fmt::Debug
	for BatchProofTransaction<SC, TC, B>
{
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.debug_struct("BatchProofTransaction")
			.field("proved_header", &self.proved_header)
			.finish()
	}
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
pub async fn run<P, SourceClnt, TargetClnt>(
	params: MessagesRelayParams<P, SourceClnt, TargetClnt>,
) -> anyhow::Result<()>
where
	P: SubstrateMessageLane,
	SourceClnt: Client<P::SourceChain>,
	TargetClnt: Client<P::TargetChain>,
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
	BalanceOf<P::SourceChain>: TryFrom<BalanceOf<P::TargetChain>>,
{
	// 2/3 is reserved for proofs and tx overhead
	let max_messages_size_in_single_batch = P::TargetChain::max_extrinsic_size() / 3;
	let limits = match params.limits {
		Some(limits) => limits,
		None =>
			select_delivery_transaction_limits_rpc(
				&params,
				P::TargetChain::max_extrinsic_weight(),
				P::SourceChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			)
			.await?,
	};
	let (max_messages_in_single_batch, max_messages_weight_in_single_batch) =
		(limits.max_messages_in_single_batch / 2, limits.max_messages_weight_in_single_batch / 2);

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
		SubstrateMessagesSource::<P, _, _>::new(
			source_client.clone(),
			target_client.clone(),
			params.lane_id,
			params.source_transaction_params,
			params.target_to_source_headers_relay,
		),
		SubstrateMessagesTarget::<P, _, _>::new(
			target_client,
			source_client,
			params.lane_id,
			relayer_id_at_source,
			Some(params.target_transaction_params),
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

/// Deliver range of Substrate-to-Substrate messages. No checks are made to ensure that transaction
/// will succeed.
pub async fn relay_messages_range<P: SubstrateMessageLane>(
	source_client: impl Client<P::SourceChain>,
	target_client: impl Client<P::TargetChain>,
	source_transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	target_transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	at_source_block: HeaderIdOf<P::SourceChain>,
	lane_id: P::LaneId,
	range: RangeInclusive<MessageNonce>,
	outbound_state_proof_required: bool,
) -> anyhow::Result<()>
where
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
	BalanceOf<P::SourceChain>: TryFrom<BalanceOf<P::TargetChain>>,
{
	let relayer_id_at_source: AccountIdOf<P::SourceChain> =
		source_transaction_params.signer.public().into();
	messages_relay::relay_messages_range(
		SubstrateMessagesSource::<P, _, _>::new(
			source_client.clone(),
			target_client.clone(),
			lane_id,
			source_transaction_params,
			None,
		),
		SubstrateMessagesTarget::<P, _, _>::new(
			target_client,
			source_client,
			lane_id,
			relayer_id_at_source,
			Some(target_transaction_params),
			None,
		),
		at_source_block,
		range,
		outbound_state_proof_required,
	)
	.await
	.map_err(|_| anyhow::format_err!("The command has failed"))
}

/// Relay messages delivery confirmation of Substrate-to-Substrate messages.
/// No checks are made to ensure that transaction will succeed.
pub async fn relay_messages_delivery_confirmation<P: SubstrateMessageLane>(
	source_client: impl Client<P::SourceChain>,
	target_client: impl Client<P::TargetChain>,
	source_transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	at_target_block: HeaderIdOf<P::TargetChain>,
	lane_id: P::LaneId,
) -> anyhow::Result<()>
where
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
	BalanceOf<P::SourceChain>: TryFrom<BalanceOf<P::TargetChain>>,
{
	let relayer_id_at_source: AccountIdOf<P::SourceChain> =
		source_transaction_params.signer.public().into();
	messages_relay::relay_messages_delivery_confirmation(
		SubstrateMessagesSource::<P, _, _>::new(
			source_client.clone(),
			target_client.clone(),
			lane_id,
			source_transaction_params,
			None,
		),
		SubstrateMessagesTarget::<P, _, _>::new(
			target_client,
			source_client,
			lane_id,
			relayer_id_at_source,
			None,
			None,
		),
		at_target_block,
	)
	.await
	.map_err(|_| anyhow::format_err!("The command has failed"))
}

/// Different ways of building `receive_messages_proof` calls.
pub trait ReceiveMessagesProofCallBuilder<P: SubstrateMessageLane> {
	/// Given messages proof, build call of `receive_messages_proof` function of bridge
	/// messages module at the target chain.
	fn build_receive_messages_proof_call(
		relayer_id_at_source: AccountIdOf<P::SourceChain>,
		proof: SubstrateMessagesProof<P::SourceChain, P::LaneId>,
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
	R: BridgeMessagesConfig<I, LaneId = P::LaneId>,
	I: 'static,
	R::BridgedChain:
		bp_runtime::Chain<AccountId = AccountIdOf<P::SourceChain>, Hash = HashOf<P::SourceChain>>,
	CallOf<P::TargetChain>: From<BridgeMessagesCall<R, I>> + GetDispatchInfo,
{
	fn build_receive_messages_proof_call(
		relayer_id_at_source: AccountIdOf<P::SourceChain>,
		proof: SubstrateMessagesProof<P::SourceChain, P::LaneId>,
		messages_count: u32,
		dispatch_weight: Weight,
		trace_call: bool,
	) -> CallOf<P::TargetChain> {
		let call: CallOf<P::TargetChain> = BridgeMessagesCall::<R, I>::receive_messages_proof {
			relayer_id_at_bridged_chain: relayer_id_at_source,
			proof: proof.1.into(),
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
				call.get_dispatch_info().call_weight,
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

		impl $crate::messages::ReceiveMessagesProofCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_receive_messages_proof_call(
				relayer_id_at_source: relay_substrate_client::AccountIdOf<
					<$pipeline as $crate::messages::SubstrateMessageLane>::SourceChain
				>,
				proof: $crate::messages::source::SubstrateMessagesProof<
					<$pipeline as $crate::messages::SubstrateMessageLane>::SourceChain,
					<$pipeline as $crate::messages::SubstrateMessageLane>::LaneId
				>,
				messages_count: u32,
				dispatch_weight: bp_messages::Weight,
				_trace_call: bool,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::messages::SubstrateMessageLane>::TargetChain
			> {
				bp_runtime::paste::item! {
					$bridge_messages($receive_messages_proof {
						relayer_id_at_bridged_chain: relayer_id_at_source,
						proof: proof.1.into(),
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
		proof: SubstrateMessagesDeliveryProof<P::TargetChain, P::LaneId>,
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
	R: BridgeMessagesConfig<I, LaneId = P::LaneId>,
	I: 'static,
	R::BridgedChain: bp_runtime::Chain<Hash = HashOf<P::TargetChain>>,
	CallOf<P::SourceChain>: From<BridgeMessagesCall<R, I>> + GetDispatchInfo,
{
	fn build_receive_messages_delivery_proof_call(
		proof: SubstrateMessagesDeliveryProof<P::TargetChain, P::LaneId>,
		trace_call: bool,
	) -> CallOf<P::SourceChain> {
		let call: CallOf<P::SourceChain> =
			BridgeMessagesCall::<R, I>::receive_messages_delivery_proof {
				proof: proof.1.into(),
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
				call.get_dispatch_info().call_weight,
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

		impl $crate::messages::ReceiveMessagesDeliveryProofCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_receive_messages_delivery_proof_call(
				proof: $crate::messages::target::SubstrateMessagesDeliveryProof<
					<$pipeline as $crate::messages::SubstrateMessageLane>::TargetChain,
					<$pipeline as $crate::messages::SubstrateMessageLane>::LaneId
				>,
				_trace_call: bool,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::messages::SubstrateMessageLane>::SourceChain
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
async fn select_delivery_transaction_limits_rpc<P, SourceClnt, TargetClnt>(
	params: &MessagesRelayParams<P, SourceClnt, TargetClnt>,
	max_extrinsic_weight: Weight,
	max_unconfirmed_messages_at_inbound_lane: MessageNonce,
) -> anyhow::Result<MessagesRelayLimits>
where
	P: SubstrateMessageLane,
	SourceClnt: Client<P::SourceChain>,
	TargetClnt: Client<P::TargetChain>,
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
	let best_target_block_hash = params.target_client.best_header_hash().await?;
	let delivery_tx_with_zero_messages = dummy_messages_delivery_transaction::<P, _, _>(params, 0)?;
	let delivery_tx_with_zero_messages_weight = params
		.target_client
		.estimate_extrinsic_weight(best_target_block_hash, delivery_tx_with_zero_messages)
		.await
		.map_err(|e| {
			anyhow::format_err!("Failed to estimate delivery extrinsic weight: {:?}", e)
		})?;

	// weight of single message delivery with outbound lane state
	let delivery_tx_with_one_message = dummy_messages_delivery_transaction::<P, _, _>(params, 1)?;
	let delivery_tx_with_one_message_weight = params
		.target_client
		.estimate_extrinsic_weight(best_target_block_hash, delivery_tx_with_one_message)
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

	Ok(MessagesRelayLimits {
		max_messages_in_single_batch: max_number_of_messages,
		max_messages_weight_in_single_batch: weight_for_messages_dispatch,
	})
}

/// Returns dummy message delivery transaction with zero messages and `1kb` proof.
fn dummy_messages_delivery_transaction<P: SubstrateMessageLane, SourceClnt, TargetClnt>(
	params: &MessagesRelayParams<P, SourceClnt, TargetClnt>,
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
					storage_proof: Default::default(),
					lane: P::LaneId::default(),
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

#[cfg(test)]
mod tests {
	use super::*;
	use bp_messages::{
		source_chain::FromBridgedChainMessagesDeliveryProof, LaneIdType, UnrewardedRelayersState,
	};
	use relay_substrate_client::calls::{UtilityCall as MockUtilityCall, UtilityCall};

	#[derive(codec::Decode, codec::Encode, Clone, Debug, PartialEq)]
	pub enum RuntimeCall {
		#[codec(index = 53)]
		BridgeMessages(CodegenBridgeMessagesCall),
		#[codec(index = 123)]
		Utility(UtilityCall<RuntimeCall>),
	}
	pub type CodegenBridgeMessagesCall = bp_messages::BridgeMessagesCall<
		u64,
		Box<FromBridgedChainMessagesProof<mock::BridgedHeaderHash, mock::TestLaneIdType>>,
		FromBridgedChainMessagesDeliveryProof<mock::BridgedHeaderHash, mock::TestLaneIdType>,
	>;

	impl From<MockUtilityCall<RuntimeCall>> for RuntimeCall {
		fn from(value: MockUtilityCall<RuntimeCall>) -> RuntimeCall {
			match value {
				MockUtilityCall::batch_all(calls) =>
					RuntimeCall::Utility(UtilityCall::<RuntimeCall>::batch_all(calls)),
			}
		}
	}

	#[test]
	fn ensure_macro_compatibility_for_generate_receive_message_proof_call_builder() {
		// data
		let receive_messages_proof = FromBridgedChainMessagesProof {
			bridged_header_hash: Default::default(),
			storage_proof: Default::default(),
			lane: mock::TestLaneIdType::try_new(1, 2).unwrap(),
			nonces_start: 0,
			nonces_end: 0,
		};
		let account = 1234;
		let messages_count = 0;
		let dispatch_weight = Default::default();

		// construct pallet Call directly
		let pallet_receive_messages_proof =
			pallet_bridge_messages::Call::<mock::TestRuntime>::receive_messages_proof {
				relayer_id_at_bridged_chain: account,
				proof: receive_messages_proof.clone().into(),
				messages_count,
				dispatch_weight,
			};

		// construct mock enum Call
		let mock_enum_receive_messages_proof = CodegenBridgeMessagesCall::receive_messages_proof {
			relayer_id_at_bridged_chain: account,
			proof: receive_messages_proof.clone().into(),
			messages_count,
			dispatch_weight,
		};

		// now we should be able to use macro `generate_receive_message_proof_call_builder`
		let relayer_call_builder_receive_messages_proof = relayer::ThisChainToBridgedChainMessageLaneReceiveMessagesProofCallBuilder::build_receive_messages_proof_call(
			account,
			(Default::default(), receive_messages_proof),
			messages_count,
			dispatch_weight,
			false,
		);

		// ensure they are all equal
		assert_eq!(
			pallet_receive_messages_proof.encode(),
			mock_enum_receive_messages_proof.encode()
		);
		match relayer_call_builder_receive_messages_proof {
			RuntimeCall::BridgeMessages(call) => match call {
				call @ CodegenBridgeMessagesCall::receive_messages_proof { .. } =>
					assert_eq!(pallet_receive_messages_proof.encode(), call.encode()),
				_ => panic!("Unexpected CodegenBridgeMessagesCall type"),
			},
			_ => panic!("Unexpected RuntimeCall type"),
		};
	}

	#[test]
	fn ensure_macro_compatibility_for_generate_receive_message_delivery_proof_call_builder() {
		// data
		let receive_messages_delivery_proof = FromBridgedChainMessagesDeliveryProof {
			bridged_header_hash: Default::default(),
			storage_proof: Default::default(),
			lane: mock::TestLaneIdType::try_new(1, 2).unwrap(),
		};
		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 0,
			messages_in_oldest_entry: 0,
			total_messages: 0,
			last_delivered_nonce: 0,
		};

		// construct pallet Call directly
		let pallet_receive_messages_delivery_proof =
			pallet_bridge_messages::Call::<mock::TestRuntime>::receive_messages_delivery_proof {
				proof: receive_messages_delivery_proof.clone(),
				relayers_state: relayers_state.clone(),
			};

		// construct mock enum Call
		let mock_enum_receive_messages_delivery_proof =
			CodegenBridgeMessagesCall::receive_messages_delivery_proof {
				proof: receive_messages_delivery_proof.clone(),
				relayers_state: relayers_state.clone(),
			};

		// now we should be able to use macro `generate_receive_message_proof_call_builder`
		let relayer_call_builder_receive_messages_delivery_proof = relayer::ThisChainToBridgedChainMessageLaneReceiveMessagesDeliveryProofCallBuilder::build_receive_messages_delivery_proof_call(
			(relayers_state, receive_messages_delivery_proof),
			false,
		);

		// ensure they are all equal
		assert_eq!(
			pallet_receive_messages_delivery_proof.encode(),
			mock_enum_receive_messages_delivery_proof.encode()
		);
		match relayer_call_builder_receive_messages_delivery_proof {
			RuntimeCall::BridgeMessages(call) => match call {
				call @ CodegenBridgeMessagesCall::receive_messages_delivery_proof { .. } =>
					assert_eq!(pallet_receive_messages_delivery_proof.encode(), call.encode()),
				_ => panic!("Unexpected CodegenBridgeMessagesCall type"),
			},
			_ => panic!("Unexpected RuntimeCall type"),
		};
	}

	// mock runtime with `pallet_bridge_messages`
	mod mock {
		use super::super::*;
		use bp_messages::{target_chain::ForbidInboundMessages, HashedLaneId};
		use bp_runtime::ChainId;
		use frame_support::derive_impl;
		use sp_core::H256;
		use sp_runtime::{
			generic, testing::Header as SubstrateHeader, traits::BlakeTwo256, StateVersion,
		};

		type Block = frame_system::mocking::MockBlock<TestRuntime>;
		pub type SignedBlock = generic::SignedBlock<Block>;

		/// Lane identifier type used for tests.
		pub type TestLaneIdType = HashedLaneId;

		frame_support::construct_runtime! {
			pub enum TestRuntime
			{
				System: frame_system,
				Messages: pallet_bridge_messages,
			}
		}

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
		impl frame_system::Config for TestRuntime {
			type Block = Block;
		}

		impl pallet_bridge_messages::Config for TestRuntime {
			type RuntimeEvent = RuntimeEvent;
			type WeightInfo = ();
			type ThisChain = ThisUnderlyingChain;
			type BridgedChain = BridgedUnderlyingChain;
			type BridgedHeaderChain = BridgedHeaderChain;
			type OutboundPayload = Vec<u8>;
			type InboundPayload = Vec<u8>;
			type LaneId = TestLaneIdType;
			type DeliveryPayments = ();
			type DeliveryConfirmationPayments = ();
			type OnMessagesDelivered = ();
			type MessageDispatch = ForbidInboundMessages<Vec<u8>, Self::LaneId>;
		}

		pub struct ThisUnderlyingChain;

		impl bp_runtime::Chain for ThisUnderlyingChain {
			const ID: ChainId = *b"tuch";
			type BlockNumber = u64;
			type Hash = H256;
			type Hasher = BlakeTwo256;
			type Header = SubstrateHeader;
			type AccountId = u64;
			type Balance = u64;
			type Nonce = u64;
			type Signature = sp_runtime::MultiSignature;
			const STATE_VERSION: StateVersion = StateVersion::V1;
			fn max_extrinsic_size() -> u32 {
				u32::MAX
			}
			fn max_extrinsic_weight() -> Weight {
				Weight::MAX
			}
		}

		impl bp_messages::ChainWithMessages for ThisUnderlyingChain {
			const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "";
			const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
			const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1000;
		}

		pub struct BridgedUnderlyingChain;

		pub type BridgedHeaderHash = H256;
		pub type BridgedChainHeader = SubstrateHeader;

		impl bp_runtime::Chain for BridgedUnderlyingChain {
			const ID: ChainId = *b"bgdc";
			type BlockNumber = u64;
			type Hash = BridgedHeaderHash;
			type Hasher = BlakeTwo256;
			type Header = BridgedChainHeader;
			type AccountId = u64;
			type Balance = u64;
			type Nonce = u64;
			type Signature = sp_runtime::MultiSignature;
			const STATE_VERSION: StateVersion = StateVersion::V1;
			fn max_extrinsic_size() -> u32 {
				4096
			}
			fn max_extrinsic_weight() -> Weight {
				Weight::MAX
			}
		}

		impl bp_messages::ChainWithMessages for BridgedUnderlyingChain {
			const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "";
			const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
			const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1000;
		}

		pub struct BridgedHeaderChain;

		impl bp_header_chain::HeaderChain<BridgedUnderlyingChain> for BridgedHeaderChain {
			fn finalized_header_state_root(
				_hash: HashOf<BridgedUnderlyingChain>,
			) -> Option<HashOf<BridgedUnderlyingChain>> {
				unreachable!()
			}
		}
	}

	// relayer configuration
	mod relayer {
		use super::*;
		use crate::{
			messages::{
				tests::{mock, RuntimeCall},
				SubstrateMessageLane,
			},
			UtilityPalletBatchCallBuilder,
		};
		use bp_runtime::UnderlyingChainProvider;
		use relay_substrate_client::{MockedRuntimeUtilityPallet, SignParam, UnsignedTransaction};
		use std::time::Duration;

		#[derive(Clone)]
		pub struct ThisChain;
		impl UnderlyingChainProvider for ThisChain {
			type Chain = mock::ThisUnderlyingChain;
		}
		impl relay_substrate_client::Chain for ThisChain {
			const NAME: &'static str = "";
			const BEST_FINALIZED_HEADER_ID_METHOD: &'static str = "";
			const FREE_HEADERS_INTERVAL_METHOD: &'static str = "";
			const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_millis(0);
			type SignedBlock = mock::SignedBlock;
			type Call = RuntimeCall;
		}
		impl relay_substrate_client::ChainWithTransactions for ThisChain {
			type AccountKeyPair = sp_core::sr25519::Pair;
			type SignedTransaction = ();

			fn sign_transaction(
				_: SignParam<Self>,
				_: UnsignedTransaction<Self>,
			) -> Result<Self::SignedTransaction, SubstrateError>
			where
				Self: Sized,
			{
				todo!()
			}
		}
		impl relay_substrate_client::ChainWithMessages for ThisChain {
			const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> = None;
			const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "";
			const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "";
		}
		impl relay_substrate_client::ChainWithUtilityPallet for ThisChain {
			type UtilityPallet = MockedRuntimeUtilityPallet<RuntimeCall>;
		}

		#[derive(Clone)]
		pub struct BridgedChain;
		impl UnderlyingChainProvider for BridgedChain {
			type Chain = mock::BridgedUnderlyingChain;
		}
		impl relay_substrate_client::Chain for BridgedChain {
			const NAME: &'static str = "";
			const BEST_FINALIZED_HEADER_ID_METHOD: &'static str = "";
			const FREE_HEADERS_INTERVAL_METHOD: &'static str = "";
			const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_millis(0);
			type SignedBlock = mock::SignedBlock;
			type Call = RuntimeCall;
		}
		impl relay_substrate_client::ChainWithTransactions for BridgedChain {
			type AccountKeyPair = sp_core::sr25519::Pair;
			type SignedTransaction = ();

			fn sign_transaction(
				_: SignParam<Self>,
				_: UnsignedTransaction<Self>,
			) -> Result<Self::SignedTransaction, SubstrateError>
			where
				Self: Sized,
			{
				todo!()
			}
		}
		impl relay_substrate_client::ChainWithMessages for BridgedChain {
			const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> = None;
			const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "";
			const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str = "";
		}
		impl relay_substrate_client::ChainWithUtilityPallet for BridgedChain {
			type UtilityPallet = MockedRuntimeUtilityPallet<RuntimeCall>;
		}

		#[derive(Clone, Debug)]
		pub struct ThisChainToBridgedChainMessageLane;
		impl SubstrateMessageLane for ThisChainToBridgedChainMessageLane {
			type SourceChain = ThisChain;
			type TargetChain = BridgedChain;
			type LaneId = mock::TestLaneIdType;
			type ReceiveMessagesProofCallBuilder =
				ThisChainToBridgedChainMessageLaneReceiveMessagesProofCallBuilder;
			type ReceiveMessagesDeliveryProofCallBuilder =
				ThisChainToBridgedChainMessageLaneReceiveMessagesDeliveryProofCallBuilder;
			type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<ThisChain>;
			type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgedChain>;
		}

		generate_receive_message_proof_call_builder!(
			ThisChainToBridgedChainMessageLane,
			ThisChainToBridgedChainMessageLaneReceiveMessagesProofCallBuilder,
			RuntimeCall::BridgeMessages,
			CodegenBridgeMessagesCall::receive_messages_proof
		);
		generate_receive_message_delivery_proof_call_builder!(
			ThisChainToBridgedChainMessageLane,
			ThisChainToBridgedChainMessageLaneReceiveMessagesDeliveryProofCallBuilder,
			RuntimeCall::BridgeMessages,
			CodegenBridgeMessagesCall::receive_messages_delivery_proof
		);
	}
}
