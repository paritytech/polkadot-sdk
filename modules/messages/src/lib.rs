// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Runtime module that allows sending and receiving messages using lane concept:
//!
//! 1) the message is sent using `send_message()` call;
//! 2) every outbound message is assigned nonce;
//! 3) the messages are stored in the storage;
//! 4) external component (relay) delivers messages to bridged chain;
//! 5) messages are processed in order (ordered by assigned nonce);
//! 6) relay may send proof-of-delivery back to this chain.
//!
//! Once message is sent, its progress can be tracked by looking at module events.
//! The assigned nonce is reported using `MessageAccepted` event. When message is
//! delivered to the the bridged chain, it is reported using `MessagesDelivered` event.
//!
//! **IMPORTANT NOTE**: after generating weights (custom `WeighInfo` implementation) for
//! your runtime (where this module is plugged to), please add test for these weights.
//! The test should call the `ensure_weights_are_correct` function from this module.
//! If this test fails with your weights, then either weights are computed incorrectly,
//! or some benchmarks assumptions are broken for your runtime.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use inbound_lane::StoredInboundLaneData;
pub use outbound_lane::StoredMessagePayload;
pub use weights::WeightInfo;
pub use weights_ext::{
	ensure_able_to_receive_confirmation, ensure_able_to_receive_message,
	ensure_weights_are_correct, WeightInfoExt, EXPECTED_DEFAULT_MESSAGE_LENGTH,
	EXTRA_STORAGE_PROOF_SIZE,
};

use crate::{
	inbound_lane::{InboundLane, InboundLaneStorage},
	outbound_lane::{OutboundLane, OutboundLaneStorage, ReceivalConfirmationError},
};

use bp_messages::{
	source_chain::{
		DeliveryConfirmationPayments, OnMessagesDelivered, SendMessageArtifacts, TargetHeaderChain,
	},
	target_chain::{
		DeliveryPayments, DispatchMessage, MessageDispatch, ProvedLaneMessages, ProvedMessages,
		SourceHeaderChain,
	},
	DeliveredMessages, InboundLaneData, InboundMessageDetails, LaneId, MessageKey, MessageNonce,
	MessagePayload, MessagesOperatingMode, OutboundLaneData, OutboundMessageDetails,
	UnrewardedRelayersState, VerificationError,
};
use bp_runtime::{
	BasicOperatingMode, ChainId, OwnedBridgeModule, PreComputedSize, RangeInclusiveExt, Size,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::PostDispatchInfo, ensure, fail, traits::Get, DefaultNoBound};
use sp_runtime::traits::UniqueSaturatedFrom;
use sp_std::{marker::PhantomData, prelude::*};

mod inbound_lane;
mod outbound_lane;
mod weights_ext;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod mock;

pub use pallet::*;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-messages";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bp_messages::{ReceivalResult, ReceivedMessages};
	use bp_runtime::RangeInclusiveExt;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		// General types

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Benchmarks results from runtime we're plugged into.
		type WeightInfo: WeightInfoExt;

		/// Gets the chain id value from the instance.
		#[pallet::constant]
		type BridgedChainId: Get<ChainId>;

		/// Get all active outbound lanes that the message pallet is serving.
		type ActiveOutboundLanes: Get<&'static [LaneId]>;
		/// Maximal number of unrewarded relayer entries at inbound lane. Unrewarded means that the
		/// relayer has delivered messages, but either confirmations haven't been delivered back to
		/// the source chain, or we haven't received reward confirmations yet.
		///
		/// This constant limits maximal number of entries in the `InboundLaneData::relayers`. Keep
		/// in mind that the same relayer account may take several (non-consecutive) entries in this
		/// set.
		type MaxUnrewardedRelayerEntriesAtInboundLane: Get<MessageNonce>;
		/// Maximal number of unconfirmed messages at inbound lane. Unconfirmed means that the
		/// message has been delivered, but either confirmations haven't been delivered back to the
		/// source chain, or we haven't received reward confirmations for these messages yet.
		///
		/// This constant limits difference between last message from last entry of the
		/// `InboundLaneData::relayers` and first message at the first entry.
		///
		/// There is no point of making this parameter lesser than
		/// MaxUnrewardedRelayerEntriesAtInboundLane, because then maximal number of relayer entries
		/// will be limited by maximal number of messages.
		///
		/// This value also represents maximal number of messages in single delivery transaction.
		/// Transaction that is declaring more messages than this value, will be rejected. Even if
		/// these messages are from different lanes.
		type MaxUnconfirmedMessagesAtInboundLane: Get<MessageNonce>;

		/// Maximal encoded size of the outbound payload.
		#[pallet::constant]
		type MaximalOutboundPayloadSize: Get<u32>;
		/// Payload type of outbound messages. This payload is dispatched on the bridged chain.
		type OutboundPayload: Parameter + Size;

		/// Payload type of inbound messages. This payload is dispatched on this chain.
		type InboundPayload: Decode;
		/// Identifier of relayer that deliver messages to this chain. Relayer reward is paid on the
		/// bridged chain.
		type InboundRelayer: Parameter + MaxEncodedLen;
		/// Delivery payments.
		type DeliveryPayments: DeliveryPayments<Self::AccountId>;

		// Types that are used by outbound_lane (on source chain).

		/// Target header chain.
		type TargetHeaderChain: TargetHeaderChain<Self::OutboundPayload, Self::AccountId>;
		/// Delivery confirmation payments.
		type DeliveryConfirmationPayments: DeliveryConfirmationPayments<Self::AccountId>;
		/// Delivery confirmation callback.
		type OnMessagesDelivered: OnMessagesDelivered;

		// Types that are used by inbound_lane (on target chain).

		/// Source header chain, as it is represented on target chain.
		type SourceHeaderChain: SourceHeaderChain;
		/// Message dispatch.
		type MessageDispatch: MessageDispatch<DispatchPayload = Self::InboundPayload>;
	}

	/// Shortcut to messages proof type for Config.
	pub type MessagesProofOf<T, I> =
		<<T as Config<I>>::SourceHeaderChain as SourceHeaderChain>::MessagesProof;
	/// Shortcut to messages delivery proof type for Config.
	pub type MessagesDeliveryProofOf<T, I> =
		<<T as Config<I>>::TargetHeaderChain as TargetHeaderChain<
			<T as Config<I>>::OutboundPayload,
			<T as frame_system::Config>::AccountId,
		>>::MessagesDeliveryProof;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> OwnedBridgeModule<T> for Pallet<T, I> {
		const LOG_TARGET: &'static str = LOG_TARGET;
		type OwnerStorage = PalletOwner<T, I>;
		type OperatingMode = MessagesOperatingMode;
		type OperatingModeStorage = PalletOperatingMode<T, I>;
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I>
	where
		u32: TryFrom<BlockNumberFor<T>>,
	{
		fn on_idle(_block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// we'll need at least to read outbound lane state, kill a message and update lane state
			let db_weight = T::DbWeight::get();
			if !remaining_weight.all_gte(db_weight.reads_writes(1, 2)) {
				return Weight::zero()
			}

			// messages from lane with index `i` in `ActiveOutboundLanes` are pruned when
			// `System::block_number() % lanes.len() == i`. Otherwise we need to read lane states on
			// every block, wasting the whole `remaining_weight` for nothing and causing starvation
			// of the last lane pruning
			let active_lanes = T::ActiveOutboundLanes::get();
			let active_lanes_len = (active_lanes.len() as u32).into();
			let active_lane_index = u32::unique_saturated_from(
				frame_system::Pallet::<T>::block_number() % active_lanes_len,
			);
			let active_lane_id = active_lanes[active_lane_index as usize];

			// first db read - outbound lane state
			let mut active_lane = outbound_lane::<T, I>(active_lane_id);
			let mut used_weight = db_weight.reads(1);
			// and here we'll have writes
			used_weight += active_lane.prune_messages(db_weight, remaining_weight - used_weight);

			// we already checked we have enough `remaining_weight` to cover this `used_weight`
			used_weight
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Change `PalletOwner`.
		///
		/// May only be called either by root, or by `PalletOwner`.
		#[pallet::call_index(0)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_owner(origin: OriginFor<T>, new_owner: Option<T::AccountId>) -> DispatchResult {
			<Self as OwnedBridgeModule<_>>::set_owner(origin, new_owner)
		}

		/// Halt or resume all/some pallet operations.
		///
		/// May only be called either by root, or by `PalletOwner`.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(
			origin: OriginFor<T>,
			operating_mode: MessagesOperatingMode,
		) -> DispatchResult {
			<Self as OwnedBridgeModule<_>>::set_operating_mode(origin, operating_mode)
		}

		/// Receive messages proof from bridged chain.
		///
		/// The weight of the call assumes that the transaction always brings outbound lane
		/// state update. Because of that, the submitter (relayer) has no benefit of not including
		/// this data in the transaction, so reward confirmations lags should be minimal.
		///
		/// The call fails if:
		///
		/// - the pallet is halted;
		///
		/// - the call origin is not `Signed(_)`;
		///
		/// - there are too many messages in the proof;
		///
		/// - the proof verification procedure returns an error - e.g. because header used to craft
		///   proof is not imported by the associated finality pallet;
		///
		/// - the `dispatch_weight` argument is not sufficient to dispatch all bundled messages.
		///
		/// The call may succeed, but some messages may not be delivered e.g. if they are not fit
		/// into the unrewarded relayers vector.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::receive_messages_proof_weight(proof, *messages_count, *dispatch_weight))]
		pub fn receive_messages_proof(
			origin: OriginFor<T>,
			relayer_id_at_bridged_chain: T::InboundRelayer,
			proof: MessagesProofOf<T, I>,
			messages_count: u32,
			dispatch_weight: Weight,
		) -> DispatchResultWithPostInfo {
			Self::ensure_not_halted().map_err(Error::<T, I>::BridgeModule)?;
			let relayer_id_at_this_chain = ensure_signed(origin)?;

			// reject transactions that are declaring too many messages
			ensure!(
				MessageNonce::from(messages_count) <= T::MaxUnconfirmedMessagesAtInboundLane::get(),
				Error::<T, I>::TooManyMessagesInTheProof
			);

			// if message dispatcher is currently inactive, we won't accept any messages
			ensure!(T::MessageDispatch::is_active(), Error::<T, I>::MessageDispatchInactive);

			// why do we need to know the weight of this (`receive_messages_proof`) call? Because
			// we may want to return some funds for not-dispatching (or partially dispatching) some
			// messages to the call origin (relayer). And this is done by returning actual weight
			// from the call. But we only know dispatch weight of every messages. So to refund
			// relayer because we have not dispatched Message, we need to:
			//
			// ActualWeight = DeclaredWeight - Message.DispatchWeight
			//
			// The DeclaredWeight is exactly what's computed here. Unfortunately it is impossible
			// to get pre-computed value (and it has been already computed by the executive).
			let declared_weight = T::WeightInfo::receive_messages_proof_weight(
				&proof,
				messages_count,
				dispatch_weight,
			);
			let mut actual_weight = declared_weight;

			// verify messages proof && convert proof into messages
			let messages = verify_and_decode_messages_proof::<
				T::SourceHeaderChain,
				T::InboundPayload,
			>(proof, messages_count)
			.map_err(|err| {
				log::trace!(target: LOG_TARGET, "Rejecting invalid messages proof: {:?}", err,);

				Error::<T, I>::InvalidMessagesProof
			})?;

			// dispatch messages and (optionally) update lane(s) state(s)
			let mut total_messages = 0;
			let mut valid_messages = 0;
			let mut messages_received_status = Vec::with_capacity(messages.len());
			let mut dispatch_weight_left = dispatch_weight;
			for (lane_id, lane_data) in messages {
				let mut lane = inbound_lane::<T, I>(lane_id);

				// subtract extra storage proof bytes from the actual PoV size - there may be
				// less unrewarded relayers than the maximal configured value
				let lane_extra_proof_size_bytes = lane.storage_mut().extra_proof_size_bytes();
				actual_weight = actual_weight.set_proof_size(
					actual_weight.proof_size().saturating_sub(lane_extra_proof_size_bytes),
				);

				if let Some(lane_state) = lane_data.lane_state {
					let updated_latest_confirmed_nonce = lane.receive_state_update(lane_state);
					if let Some(updated_latest_confirmed_nonce) = updated_latest_confirmed_nonce {
						log::trace!(
							target: LOG_TARGET,
							"Received lane {:?} state update: latest_confirmed_nonce={}. Unrewarded relayers: {:?}",
							lane_id,
							updated_latest_confirmed_nonce,
							UnrewardedRelayersState::from(&lane.storage_mut().get_or_init_data()),
						);
					}
				}

				let mut lane_messages_received_status =
					ReceivedMessages::new(lane_id, Vec::with_capacity(lane_data.messages.len()));
				for mut message in lane_data.messages {
					debug_assert_eq!(message.key.lane_id, lane_id);
					total_messages += 1;

					// ensure that relayer has declared enough weight for dispatching next message
					// on this lane. We can't dispatch lane messages out-of-order, so if declared
					// weight is not enough, let's move to next lane
					let message_dispatch_weight = T::MessageDispatch::dispatch_weight(&mut message);
					if message_dispatch_weight.any_gt(dispatch_weight_left) {
						log::trace!(
							target: LOG_TARGET,
							"Cannot dispatch any more messages on lane {:?}. Weight: declared={}, left={}",
							lane_id,
							message_dispatch_weight,
							dispatch_weight_left,
						);

						fail!(Error::<T, I>::InsufficientDispatchWeight);
					}

					let receival_result = lane.receive_message::<T::MessageDispatch>(
						&relayer_id_at_bridged_chain,
						message.key.nonce,
						message.data,
					);

					// note that we're returning unspent weight to relayer even if message has been
					// rejected by the lane. This allows relayers to submit spam transactions with
					// e.g. the same set of already delivered messages over and over again, without
					// losing funds for messages dispatch. But keep in mind that relayer pays base
					// delivery transaction cost anyway. And base cost covers everything except
					// dispatch, so we have a balance here.
					let unspent_weight = match &receival_result {
						ReceivalResult::Dispatched(dispatch_result) => {
							valid_messages += 1;
							dispatch_result.unspent_weight
						},
						ReceivalResult::InvalidNonce |
						ReceivalResult::TooManyUnrewardedRelayers |
						ReceivalResult::TooManyUnconfirmedMessages => message_dispatch_weight,
					};
					lane_messages_received_status.push(message.key.nonce, receival_result);

					let unspent_weight = unspent_weight.min(message_dispatch_weight);
					dispatch_weight_left -= message_dispatch_weight - unspent_weight;
					actual_weight = actual_weight.saturating_sub(unspent_weight);
				}

				messages_received_status.push(lane_messages_received_status);
			}

			// let's now deal with relayer payments
			T::DeliveryPayments::pay_reward(
				relayer_id_at_this_chain,
				total_messages,
				valid_messages,
				actual_weight,
			);

			log::debug!(
				target: LOG_TARGET,
				"Received messages: total={}, valid={}. Weight used: {}/{}.",
				total_messages,
				valid_messages,
				actual_weight,
				declared_weight,
			);

			Self::deposit_event(Event::MessagesReceived(messages_received_status));

			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::Yes })
		}

		/// Receive messages delivery proof from bridged chain.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::receive_messages_delivery_proof_weight(
			proof,
			relayers_state,
		))]
		pub fn receive_messages_delivery_proof(
			origin: OriginFor<T>,
			proof: MessagesDeliveryProofOf<T, I>,
			mut relayers_state: UnrewardedRelayersState,
		) -> DispatchResultWithPostInfo {
			Self::ensure_not_halted().map_err(Error::<T, I>::BridgeModule)?;

			let proof_size = proof.size();
			let confirmation_relayer = ensure_signed(origin)?;
			let (lane_id, lane_data) = T::TargetHeaderChain::verify_messages_delivery_proof(proof)
				.map_err(|err| {
					log::trace!(
						target: LOG_TARGET,
						"Rejecting invalid messages delivery proof: {:?}",
						err,
					);

					Error::<T, I>::InvalidMessagesDeliveryProof
				})?;
			ensure!(
				relayers_state.is_valid(&lane_data),
				Error::<T, I>::InvalidUnrewardedRelayersState
			);

			// mark messages as delivered
			let mut lane = outbound_lane::<T, I>(lane_id);
			let last_delivered_nonce = lane_data.last_delivered_nonce();
			let confirmed_messages = lane
				.confirm_delivery(
					relayers_state.total_messages,
					last_delivered_nonce,
					&lane_data.relayers,
				)
				.map_err(Error::<T, I>::ReceivalConfirmation)?;

			if let Some(confirmed_messages) = confirmed_messages {
				// emit 'delivered' event
				let received_range = confirmed_messages.begin..=confirmed_messages.end;
				Self::deposit_event(Event::MessagesDelivered {
					lane_id,
					messages: confirmed_messages,
				});

				// if some new messages have been confirmed, reward relayers
				let actually_rewarded_relayers = T::DeliveryConfirmationPayments::pay_reward(
					lane_id,
					lane_data.relayers,
					&confirmation_relayer,
					&received_range,
				);

				// update relayers state with actual numbers to compute actual weight below
				relayers_state.unrewarded_relayer_entries = sp_std::cmp::min(
					relayers_state.unrewarded_relayer_entries,
					actually_rewarded_relayers,
				);
				relayers_state.total_messages = sp_std::cmp::min(
					relayers_state.total_messages,
					received_range.checked_len().unwrap_or(MessageNonce::MAX),
				);
			};

			log::trace!(
				target: LOG_TARGET,
				"Received messages delivery proof up to (and including) {} at lane {:?}",
				last_delivered_nonce,
				lane_id,
			);

			// notify others about messages delivery
			T::OnMessagesDelivered::on_messages_delivered(
				lane_id,
				lane.data().queued_messages().saturating_len(),
			);

			// because of lags, the inbound lane state (`lane_data`) may have entries for
			// already rewarded relayers and messages (if all entries are duplicated, then
			// this transaction must be filtered out by our signed extension)
			let actual_weight = T::WeightInfo::receive_messages_delivery_proof_weight(
				&PreComputedSize(proof_size as usize),
				&relayers_state,
			);

			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::Yes })
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Message has been accepted and is waiting to be delivered.
		MessageAccepted {
			/// Lane, which has accepted the message.
			lane_id: LaneId,
			/// Nonce of accepted message.
			nonce: MessageNonce,
		},
		/// Messages have been received from the bridged chain.
		MessagesReceived(
			/// Result of received messages dispatch.
			Vec<ReceivedMessages<<T::MessageDispatch as MessageDispatch>::DispatchLevelResult>>,
		),
		/// Messages in the inclusive range have been delivered to the bridged chain.
		MessagesDelivered {
			/// Lane for which the delivery has been confirmed.
			lane_id: LaneId,
			/// Delivered messages.
			messages: DeliveredMessages,
		},
	}

	#[pallet::error]
	#[derive(PartialEq, Eq)]
	pub enum Error<T, I = ()> {
		/// Pallet is not in Normal operating mode.
		NotOperatingNormally,
		/// The outbound lane is inactive.
		InactiveOutboundLane,
		/// The inbound message dispatcher is inactive.
		MessageDispatchInactive,
		/// Message has been treated as invalid by chain verifier.
		MessageRejectedByChainVerifier(VerificationError),
		/// Message has been treated as invalid by the pallet logic.
		MessageRejectedByPallet(VerificationError),
		/// Submitter has failed to pay fee for delivering and dispatching messages.
		FailedToWithdrawMessageFee,
		/// The transaction brings too many messages.
		TooManyMessagesInTheProof,
		/// Invalid messages has been submitted.
		InvalidMessagesProof,
		/// Invalid messages delivery proof has been submitted.
		InvalidMessagesDeliveryProof,
		/// The relayer has declared invalid unrewarded relayers state in the
		/// `receive_messages_delivery_proof` call.
		InvalidUnrewardedRelayersState,
		/// The cumulative dispatch weight, passed by relayer is not enough to cover dispatch
		/// of all bundled messages.
		InsufficientDispatchWeight,
		/// The message someone is trying to work with (i.e. increase fee) is not yet sent.
		MessageIsNotYetSent,
		/// Error confirming messages receival.
		ReceivalConfirmation(ReceivalConfirmationError),
		/// Error generated by the `OwnedBridgeModule` trait.
		BridgeModule(bp_runtime::OwnedBridgeModuleError),
	}

	/// Optional pallet owner.
	///
	/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
	/// `None`, then there are no direct ways to halt/resume pallet operations, but other
	/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
	/// flag directly or call the `halt_operations`).
	#[pallet::storage]
	#[pallet::getter(fn module_owner)]
	pub type PalletOwner<T: Config<I>, I: 'static = ()> = StorageValue<_, T::AccountId>;

	/// The current operating mode of the pallet.
	///
	/// Depending on the mode either all, some, or no transactions will be allowed.
	#[pallet::storage]
	#[pallet::getter(fn operating_mode)]
	pub type PalletOperatingMode<T: Config<I>, I: 'static = ()> =
		StorageValue<_, MessagesOperatingMode, ValueQuery>;

	/// Map of lane id => inbound lane data.
	#[pallet::storage]
	pub type InboundLanes<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, LaneId, StoredInboundLaneData<T, I>, ValueQuery>;

	/// Map of lane id => outbound lane data.
	#[pallet::storage]
	pub type OutboundLanes<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = Blake2_128Concat,
		Key = LaneId,
		Value = OutboundLaneData,
		QueryKind = ValueQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeOutboundLanesCount<T, I>,
	>;

	/// Map of lane id => is congested signal sent. It is managed by the
	/// `bridge_runtime_common::LocalXcmQueueManager`.
	///
	/// **bridges-v1**: this map is a temporary hack and will be dropped in the `v2`. We can emulate
	/// a storage map using `sp_io::unhashed` storage functions, but then benchmarks are not
	/// accounting its `proof_size`, so it is missing from the final weights. So we need to make it
	/// a map inside some pallet. We could use a simply value instead of map here, because
	/// in `v1` we'll only have a single lane. But in the case of adding another lane before `v2`,
	/// it'll be easier to deal with the isolated storage map instead.
	#[pallet::storage]
	pub type OutboundLanesCongestedSignals<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = Blake2_128Concat,
		Key = LaneId,
		Value = bool,
		QueryKind = ValueQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeOutboundLanesCount<T, I>,
	>;

	/// All queued outbound messages.
	#[pallet::storage]
	pub type OutboundMessages<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, MessageKey, StoredMessagePayload<T, I>>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// Initial pallet operating mode.
		pub operating_mode: MessagesOperatingMode,
		/// Initial pallet owner.
		pub owner: Option<T::AccountId>,
		/// Dummy marker.
		pub phantom: sp_std::marker::PhantomData<I>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			PalletOperatingMode::<T, I>::put(self.operating_mode);
			if let Some(ref owner) = self.owner {
				PalletOwner::<T, I>::put(owner);
			}
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Get stored data of the outbound message with given nonce.
		pub fn outbound_message_data(lane: LaneId, nonce: MessageNonce) -> Option<MessagePayload> {
			OutboundMessages::<T, I>::get(MessageKey { lane_id: lane, nonce }).map(Into::into)
		}

		/// Prepare data, related to given inbound message.
		pub fn inbound_message_data(
			lane: LaneId,
			payload: MessagePayload,
			outbound_details: OutboundMessageDetails,
		) -> InboundMessageDetails {
			let mut dispatch_message = DispatchMessage {
				key: MessageKey { lane_id: lane, nonce: outbound_details.nonce },
				data: payload.into(),
			};
			InboundMessageDetails {
				dispatch_weight: T::MessageDispatch::dispatch_weight(&mut dispatch_message),
			}
		}

		/// Return outbound lane data.
		pub fn outbound_lane_data(lane: LaneId) -> OutboundLaneData {
			OutboundLanes::<T, I>::get(lane)
		}

		/// Return inbound lane data.
		pub fn inbound_lane_data(lane: LaneId) -> InboundLaneData<T::InboundRelayer> {
			InboundLanes::<T, I>::get(lane).0
		}
	}

	/// Get-parameter that returns number of active outbound lanes that the pallet maintains.
	pub struct MaybeOutboundLanesCount<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> Get<Option<u32>> for MaybeOutboundLanesCount<T, I> {
		fn get() -> Option<u32> {
			Some(T::ActiveOutboundLanes::get().len() as u32)
		}
	}
}

/// Structure, containing a validated message payload and all the info required
/// to send it on the bridge.
#[derive(Debug, PartialEq, Eq)]
pub struct SendMessageArgs<T: Config<I>, I: 'static> {
	lane_id: LaneId,
	payload: StoredMessagePayload<T, I>,
}

impl<T, I> bp_messages::source_chain::MessagesBridge<T::OutboundPayload> for Pallet<T, I>
where
	T: Config<I>,
	I: 'static,
{
	type Error = Error<T, I>;
	type SendMessageArgs = SendMessageArgs<T, I>;

	fn validate_message(
		lane: LaneId,
		message: &T::OutboundPayload,
	) -> Result<SendMessageArgs<T, I>, Self::Error> {
		ensure_normal_operating_mode::<T, I>()?;

		// let's check if outbound lane is active
		ensure!(T::ActiveOutboundLanes::get().contains(&lane), Error::<T, I>::InactiveOutboundLane);

		// let's first check if message can be delivered to target chain
		T::TargetHeaderChain::verify_message(message).map_err(|err| {
			log::trace!(
				target: LOG_TARGET,
				"Message to lane {:?} is rejected by target chain: {:?}",
				lane,
				err,
			);

			Error::<T, I>::MessageRejectedByChainVerifier(err)
		})?;

		Ok(SendMessageArgs {
			lane_id: lane,
			payload: StoredMessagePayload::<T, I>::try_from(message.encode()).map_err(|_| {
				Error::<T, I>::MessageRejectedByPallet(VerificationError::MessageTooLarge)
			})?,
		})
	}

	fn send_message(args: SendMessageArgs<T, I>) -> SendMessageArtifacts {
		// save message in outbound storage and emit event
		let mut lane = outbound_lane::<T, I>(args.lane_id);
		let message_len = args.payload.len();
		let nonce = lane.send_message(args.payload);

		// return number of messages in the queue to let sender know about its state
		let enqueued_messages = lane.data().queued_messages().saturating_len();

		log::trace!(
			target: LOG_TARGET,
			"Accepted message {} to lane {:?}. Message size: {:?}",
			nonce,
			args.lane_id,
			message_len,
		);

		Pallet::<T, I>::deposit_event(Event::MessageAccepted { lane_id: args.lane_id, nonce });

		SendMessageArtifacts { nonce, enqueued_messages }
	}
}

/// Ensure that the pallet is in normal operational mode.
fn ensure_normal_operating_mode<T: Config<I>, I: 'static>() -> Result<(), Error<T, I>> {
	if PalletOperatingMode::<T, I>::get() ==
		MessagesOperatingMode::Basic(BasicOperatingMode::Normal)
	{
		return Ok(())
	}

	Err(Error::<T, I>::NotOperatingNormally)
}

/// Creates new inbound lane object, backed by runtime storage.
fn inbound_lane<T: Config<I>, I: 'static>(
	lane_id: LaneId,
) -> InboundLane<RuntimeInboundLaneStorage<T, I>> {
	InboundLane::new(RuntimeInboundLaneStorage::from_lane_id(lane_id))
}

/// Creates new outbound lane object, backed by runtime storage.
fn outbound_lane<T: Config<I>, I: 'static>(
	lane_id: LaneId,
) -> OutboundLane<RuntimeOutboundLaneStorage<T, I>> {
	OutboundLane::new(RuntimeOutboundLaneStorage { lane_id, _phantom: Default::default() })
}

/// Runtime inbound lane storage.
struct RuntimeInboundLaneStorage<T: Config<I>, I: 'static = ()> {
	lane_id: LaneId,
	cached_data: Option<InboundLaneData<T::InboundRelayer>>,
	_phantom: PhantomData<I>,
}

impl<T: Config<I>, I: 'static> RuntimeInboundLaneStorage<T, I> {
	/// Creates new runtime inbound lane storage.
	fn from_lane_id(lane_id: LaneId) -> RuntimeInboundLaneStorage<T, I> {
		RuntimeInboundLaneStorage { lane_id, cached_data: None, _phantom: Default::default() }
	}
}

impl<T: Config<I>, I: 'static> RuntimeInboundLaneStorage<T, I> {
	/// Returns number of bytes that may be subtracted from the PoV component of
	/// `receive_messages_proof` call, because the actual inbound lane state is smaller than the
	/// maximal configured.
	///
	/// Maximal inbound lane state set size is configured by the
	/// `MaxUnrewardedRelayerEntriesAtInboundLane` constant from the pallet configuration. The PoV
	/// of the call includes the maximal size of inbound lane state. If the actual size is smaller,
	/// we may subtract extra bytes from this component.
	pub fn extra_proof_size_bytes(&mut self) -> u64 {
		let max_encoded_len = StoredInboundLaneData::<T, I>::max_encoded_len();
		let relayers_count = self.get_or_init_data().relayers.len();
		let actual_encoded_len =
			InboundLaneData::<T::InboundRelayer>::encoded_size_hint(relayers_count)
				.unwrap_or(usize::MAX);
		max_encoded_len.saturating_sub(actual_encoded_len) as _
	}
}

impl<T: Config<I>, I: 'static> InboundLaneStorage for RuntimeInboundLaneStorage<T, I> {
	type Relayer = T::InboundRelayer;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn max_unrewarded_relayer_entries(&self) -> MessageNonce {
		T::MaxUnrewardedRelayerEntriesAtInboundLane::get()
	}

	fn max_unconfirmed_messages(&self) -> MessageNonce {
		T::MaxUnconfirmedMessagesAtInboundLane::get()
	}

	fn get_or_init_data(&mut self) -> InboundLaneData<T::InboundRelayer> {
		match self.cached_data {
			Some(ref data) => data.clone(),
			None => {
				let data: InboundLaneData<T::InboundRelayer> =
					InboundLanes::<T, I>::get(self.lane_id).into();
				self.cached_data = Some(data.clone());
				data
			},
		}
	}

	fn set_data(&mut self, data: InboundLaneData<T::InboundRelayer>) {
		self.cached_data = Some(data.clone());
		InboundLanes::<T, I>::insert(self.lane_id, StoredInboundLaneData::<T, I>(data))
	}
}

/// Runtime outbound lane storage.
struct RuntimeOutboundLaneStorage<T, I = ()> {
	lane_id: LaneId,
	_phantom: PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> OutboundLaneStorage for RuntimeOutboundLaneStorage<T, I> {
	type StoredMessagePayload = StoredMessagePayload<T, I>;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn data(&self) -> OutboundLaneData {
		OutboundLanes::<T, I>::get(self.lane_id)
	}

	fn set_data(&mut self, data: OutboundLaneData) {
		OutboundLanes::<T, I>::insert(self.lane_id, data)
	}

	#[cfg(test)]
	fn message(&self, nonce: &MessageNonce) -> Option<Self::StoredMessagePayload> {
		OutboundMessages::<T, I>::get(MessageKey { lane_id: self.lane_id, nonce: *nonce })
	}

	fn save_message(&mut self, nonce: MessageNonce, message_payload: Self::StoredMessagePayload) {
		OutboundMessages::<T, I>::insert(
			MessageKey { lane_id: self.lane_id, nonce },
			message_payload,
		);
	}

	fn remove_message(&mut self, nonce: &MessageNonce) {
		OutboundMessages::<T, I>::remove(MessageKey { lane_id: self.lane_id, nonce: *nonce });
	}
}

/// Verify messages proof and return proved messages with decoded payload.
fn verify_and_decode_messages_proof<Chain: SourceHeaderChain, DispatchPayload: Decode>(
	proof: Chain::MessagesProof,
	messages_count: u32,
) -> Result<ProvedMessages<DispatchMessage<DispatchPayload>>, VerificationError> {
	// `receive_messages_proof` weight formula and `MaxUnconfirmedMessagesAtInboundLane` check
	// guarantees that the `message_count` is sane and Vec<Message> may be allocated.
	// (tx with too many messages will either be rejected from the pool, or will fail earlier)
	Chain::verify_messages_proof(proof, messages_count).map(|messages_by_lane| {
		messages_by_lane
			.into_iter()
			.map(|(lane, lane_data)| {
				(
					lane,
					ProvedLaneMessages {
						lane_state: lane_data.lane_state,
						messages: lane_data.messages.into_iter().map(Into::into).collect(),
					},
				)
			})
			.collect()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{
			inbound_unrewarded_relayers_state, message, message_payload, run_test,
			unrewarded_relayer, AccountId, DbWeight, RuntimeEvent as TestEvent, RuntimeOrigin,
			TestDeliveryConfirmationPayments, TestDeliveryPayments, TestMessageDispatch,
			TestMessagesDeliveryProof, TestMessagesProof, TestOnMessagesDelivered, TestRelayer,
			TestRuntime, TestWeightInfo, MAX_OUTBOUND_PAYLOAD_SIZE,
			PAYLOAD_REJECTED_BY_TARGET_CHAIN, REGULAR_PAYLOAD, TEST_LANE_ID, TEST_LANE_ID_2,
			TEST_LANE_ID_3, TEST_RELAYER_A, TEST_RELAYER_B,
		},
		outbound_lane::ReceivalConfirmationError,
	};
	use bp_messages::{
		source_chain::MessagesBridge, BridgeMessagesCall, UnrewardedRelayer,
		UnrewardedRelayersState,
	};
	use bp_test_utils::generate_owned_bridge_module_tests;
	use frame_support::{
		assert_noop, assert_ok,
		dispatch::Pays,
		storage::generator::{StorageMap, StorageValue},
		traits::Hooks,
		weights::Weight,
	};
	use frame_system::{EventRecord, Pallet as System, Phase};
	use sp_runtime::DispatchError;

	fn get_ready_for_events() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();
	}

	fn send_regular_message(lane_id: LaneId) {
		get_ready_for_events();

		let outbound_lane = outbound_lane::<TestRuntime, ()>(lane_id);
		let message_nonce = outbound_lane.data().latest_generated_nonce + 1;
		let prev_enqueud_messages = outbound_lane.data().queued_messages().saturating_len();
		let valid_message = Pallet::<TestRuntime, ()>::validate_message(lane_id, &REGULAR_PAYLOAD)
			.expect("validate_message has failed");
		let artifacts = Pallet::<TestRuntime, ()>::send_message(valid_message);
		assert_eq!(artifacts.enqueued_messages, prev_enqueud_messages + 1);

		// check event with assigned nonce
		assert_eq!(
			System::<TestRuntime>::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: TestEvent::Messages(Event::MessageAccepted {
					lane_id,
					nonce: message_nonce
				}),
				topics: vec![],
			}],
		);
	}

	fn receive_messages_delivery_proof() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();

		assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
			RuntimeOrigin::signed(1),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					last_confirmed_nonce: 1,
					relayers: vec![UnrewardedRelayer {
						relayer: 0,
						messages: DeliveredMessages::new(1),
					}]
					.into_iter()
					.collect(),
				},
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				messages_in_oldest_entry: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
			},
		));

		assert_eq!(
			System::<TestRuntime>::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: TestEvent::Messages(Event::MessagesDelivered {
					lane_id: TEST_LANE_ID,
					messages: DeliveredMessages::new(1),
				}),
				topics: vec![],
			}],
		);
	}

	#[test]
	fn pallet_rejects_transactions_if_halted() {
		run_test(|| {
			// send message first to be able to check that delivery_proof fails later
			send_regular_message(TEST_LANE_ID);

			PalletOperatingMode::<TestRuntime, ()>::put(MessagesOperatingMode::Basic(
				BasicOperatingMode::Halted,
			));

			assert_noop!(
				Pallet::<TestRuntime, ()>::validate_message(TEST_LANE_ID, &REGULAR_PAYLOAD),
				Error::<TestRuntime, ()>::NotOperatingNormally,
			);

			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					Ok(vec![message(2, REGULAR_PAYLOAD)]).into(),
					1,
					REGULAR_PAYLOAD.declared_weight,
				),
				Error::<TestRuntime, ()>::BridgeModule(bp_runtime::OwnedBridgeModuleError::Halted),
			);

			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_delivery_proof(
					RuntimeOrigin::signed(1),
					TestMessagesDeliveryProof(Ok((
						TEST_LANE_ID,
						InboundLaneData {
							last_confirmed_nonce: 1,
							relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
								.into_iter()
								.collect(),
						},
					))),
					UnrewardedRelayersState {
						unrewarded_relayer_entries: 1,
						messages_in_oldest_entry: 1,
						total_messages: 1,
						last_delivered_nonce: 1,
					},
				),
				Error::<TestRuntime, ()>::BridgeModule(bp_runtime::OwnedBridgeModuleError::Halted),
			);
		});
	}

	#[test]
	fn pallet_rejects_new_messages_in_rejecting_outbound_messages_operating_mode() {
		run_test(|| {
			// send message first to be able to check that delivery_proof fails later
			send_regular_message(TEST_LANE_ID);

			PalletOperatingMode::<TestRuntime, ()>::put(
				MessagesOperatingMode::RejectingOutboundMessages,
			);

			assert_noop!(
				Pallet::<TestRuntime, ()>::validate_message(TEST_LANE_ID, &REGULAR_PAYLOAD),
				Error::<TestRuntime, ()>::NotOperatingNormally,
			);

			assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
				1,
				REGULAR_PAYLOAD.declared_weight,
			),);

			assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						last_confirmed_nonce: 1,
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
					},
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 1,
					total_messages: 1,
					last_delivered_nonce: 1,
				},
			));
		});
	}

	#[test]
	fn send_message_works() {
		run_test(|| {
			send_regular_message(TEST_LANE_ID);
		});
	}

	#[test]
	fn send_message_rejects_too_large_message() {
		run_test(|| {
			let mut message_payload = message_payload(1, 0);
			// the payload isn't simply extra, so it'll definitely overflow
			// `MAX_OUTBOUND_PAYLOAD_SIZE` if we add `MAX_OUTBOUND_PAYLOAD_SIZE` bytes to extra
			message_payload
				.extra
				.extend_from_slice(&[0u8; MAX_OUTBOUND_PAYLOAD_SIZE as usize]);
			assert_noop!(
				Pallet::<TestRuntime, ()>::validate_message(TEST_LANE_ID, &message_payload.clone(),),
				Error::<TestRuntime, ()>::MessageRejectedByPallet(
					VerificationError::MessageTooLarge
				),
			);

			// let's check that we're able to send `MAX_OUTBOUND_PAYLOAD_SIZE` messages
			while message_payload.encoded_size() as u32 > MAX_OUTBOUND_PAYLOAD_SIZE {
				message_payload.extra.pop();
			}
			assert_eq!(message_payload.encoded_size() as u32, MAX_OUTBOUND_PAYLOAD_SIZE);

			let valid_message =
				Pallet::<TestRuntime, ()>::validate_message(TEST_LANE_ID, &message_payload)
					.expect("validate_message has failed");
			Pallet::<TestRuntime, ()>::send_message(valid_message);
		})
	}

	#[test]
	fn chain_verifier_rejects_invalid_message_in_send_message() {
		run_test(|| {
			// messages with this payload are rejected by target chain verifier
			assert_noop!(
				Pallet::<TestRuntime, ()>::validate_message(
					TEST_LANE_ID,
					&PAYLOAD_REJECTED_BY_TARGET_CHAIN,
				),
				Error::<TestRuntime, ()>::MessageRejectedByChainVerifier(VerificationError::Other(
					mock::TEST_ERROR
				)),
			);
		});
	}

	#[test]
	fn receive_messages_proof_works() {
		run_test(|| {
			assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
				1,
				REGULAR_PAYLOAD.declared_weight,
			));

			assert_eq!(InboundLanes::<TestRuntime>::get(TEST_LANE_ID).0.last_delivered_nonce(), 1);

			assert!(TestDeliveryPayments::is_reward_paid(1));
		});
	}

	#[test]
	fn receive_messages_proof_updates_confirmed_message_nonce() {
		run_test(|| {
			// say we have received 10 messages && last confirmed message is 8
			InboundLanes::<TestRuntime, ()>::insert(
				TEST_LANE_ID,
				InboundLaneData {
					last_confirmed_nonce: 8,
					relayers: vec![
						unrewarded_relayer(9, 9, TEST_RELAYER_A),
						unrewarded_relayer(10, 10, TEST_RELAYER_B),
					]
					.into_iter()
					.collect(),
				},
			);
			assert_eq!(
				inbound_unrewarded_relayers_state(TEST_LANE_ID),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					messages_in_oldest_entry: 1,
					total_messages: 2,
					last_delivered_nonce: 10,
				},
			);

			// message proof includes outbound lane state with latest confirmed message updated to 9
			let mut message_proof: TestMessagesProof =
				Ok(vec![message(11, REGULAR_PAYLOAD)]).into();
			message_proof.result.as_mut().unwrap()[0].1.lane_state =
				Some(OutboundLaneData { latest_received_nonce: 9, ..Default::default() });

			assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				message_proof,
				1,
				REGULAR_PAYLOAD.declared_weight,
			));

			assert_eq!(
				InboundLanes::<TestRuntime>::get(TEST_LANE_ID).0,
				InboundLaneData {
					last_confirmed_nonce: 9,
					relayers: vec![
						unrewarded_relayer(10, 10, TEST_RELAYER_B),
						unrewarded_relayer(11, 11, TEST_RELAYER_A)
					]
					.into_iter()
					.collect(),
				},
			);
			assert_eq!(
				inbound_unrewarded_relayers_state(TEST_LANE_ID),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					messages_in_oldest_entry: 1,
					total_messages: 2,
					last_delivered_nonce: 11,
				},
			);
		});
	}

	#[test]
	fn receive_messages_fails_if_dispatcher_is_inactive() {
		run_test(|| {
			TestMessageDispatch::deactivate();
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
					1,
					REGULAR_PAYLOAD.declared_weight,
				),
				Error::<TestRuntime, ()>::MessageDispatchInactive,
			);
		});
	}

	#[test]
	fn receive_messages_proof_does_not_accept_message_if_dispatch_weight_is_not_enough() {
		run_test(|| {
			let mut declared_weight = REGULAR_PAYLOAD.declared_weight;
			*declared_weight.ref_time_mut() -= 1;
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
					1,
					declared_weight,
				),
				Error::<TestRuntime, ()>::InsufficientDispatchWeight
			);
			assert_eq!(InboundLanes::<TestRuntime>::get(TEST_LANE_ID).last_delivered_nonce(), 0);
		});
	}

	#[test]
	fn receive_messages_proof_rejects_invalid_proof() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime, ()>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					Err(()).into(),
					1,
					Weight::zero(),
				),
				Error::<TestRuntime, ()>::InvalidMessagesProof,
			);
		});
	}

	#[test]
	fn receive_messages_proof_rejects_proof_with_too_many_messages() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime, ()>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
					u32::MAX,
					Weight::zero(),
				),
				Error::<TestRuntime, ()>::TooManyMessagesInTheProof,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_works() {
		run_test(|| {
			send_regular_message(TEST_LANE_ID);
			receive_messages_delivery_proof();

			assert_eq!(
				OutboundLanes::<TestRuntime, ()>::get(TEST_LANE_ID).latest_received_nonce,
				1,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rewards_relayers() {
		run_test(|| {
			send_regular_message(TEST_LANE_ID);
			send_regular_message(TEST_LANE_ID);

			// this reports delivery of message 1 => reward is paid to TEST_RELAYER_A
			let single_message_delivery_proof = TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				},
			)));
			let single_message_delivery_proof_size = single_message_delivery_proof.size();
			let result = Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				single_message_delivery_proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 1,
					total_messages: 1,
					last_delivered_nonce: 1,
				},
			);
			assert_ok!(result);
			assert_eq!(
				result.unwrap().actual_weight.unwrap(),
				TestWeightInfo::receive_messages_delivery_proof_weight(
					&PreComputedSize(single_message_delivery_proof_size as _),
					&UnrewardedRelayersState {
						unrewarded_relayer_entries: 1,
						total_messages: 1,
						..Default::default()
					},
				)
			);
			assert!(TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_A, 1));
			assert!(!TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_B, 1));
			assert_eq!(TestOnMessagesDelivered::call_arguments(), Some((TEST_LANE_ID, 1)));

			// this reports delivery of both message 1 and message 2 => reward is paid only to
			// TEST_RELAYER_B
			let two_messages_delivery_proof = TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![
						unrewarded_relayer(1, 1, TEST_RELAYER_A),
						unrewarded_relayer(2, 2, TEST_RELAYER_B),
					]
					.into_iter()
					.collect(),
					..Default::default()
				},
			)));
			let two_messages_delivery_proof_size = two_messages_delivery_proof.size();
			let result = Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				two_messages_delivery_proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					messages_in_oldest_entry: 1,
					total_messages: 2,
					last_delivered_nonce: 2,
				},
			);
			assert_ok!(result);
			// even though the pre-dispatch weight was for two messages, the actual weight is
			// for single message only
			assert_eq!(
				result.unwrap().actual_weight.unwrap(),
				TestWeightInfo::receive_messages_delivery_proof_weight(
					&PreComputedSize(two_messages_delivery_proof_size as _),
					&UnrewardedRelayersState {
						unrewarded_relayer_entries: 1,
						total_messages: 1,
						..Default::default()
					},
				)
			);
			assert!(!TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_A, 1));
			assert!(TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_B, 1));
			assert_eq!(TestOnMessagesDelivered::call_arguments(), Some((TEST_LANE_ID, 0)));
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rejects_invalid_proof() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_delivery_proof(
					RuntimeOrigin::signed(1),
					TestMessagesDeliveryProof(Err(())),
					Default::default(),
				),
				Error::<TestRuntime, ()>::InvalidMessagesDeliveryProof,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rejects_proof_if_declared_relayers_state_is_invalid() {
		run_test(|| {
			// when number of relayers entries is invalid
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_delivery_proof(
					RuntimeOrigin::signed(1),
					TestMessagesDeliveryProof(Ok((
						TEST_LANE_ID,
						InboundLaneData {
							relayers: vec![
								unrewarded_relayer(1, 1, TEST_RELAYER_A),
								unrewarded_relayer(2, 2, TEST_RELAYER_B)
							]
							.into_iter()
							.collect(),
							..Default::default()
						}
					))),
					UnrewardedRelayersState {
						unrewarded_relayer_entries: 1,
						total_messages: 2,
						last_delivered_nonce: 2,
						..Default::default()
					},
				),
				Error::<TestRuntime, ()>::InvalidUnrewardedRelayersState,
			);

			// when number of messages is invalid
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_delivery_proof(
					RuntimeOrigin::signed(1),
					TestMessagesDeliveryProof(Ok((
						TEST_LANE_ID,
						InboundLaneData {
							relayers: vec![
								unrewarded_relayer(1, 1, TEST_RELAYER_A),
								unrewarded_relayer(2, 2, TEST_RELAYER_B)
							]
							.into_iter()
							.collect(),
							..Default::default()
						}
					))),
					UnrewardedRelayersState {
						unrewarded_relayer_entries: 2,
						total_messages: 1,
						last_delivered_nonce: 2,
						..Default::default()
					},
				),
				Error::<TestRuntime, ()>::InvalidUnrewardedRelayersState,
			);

			// when last delivered nonce is invalid
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_delivery_proof(
					RuntimeOrigin::signed(1),
					TestMessagesDeliveryProof(Ok((
						TEST_LANE_ID,
						InboundLaneData {
							relayers: vec![
								unrewarded_relayer(1, 1, TEST_RELAYER_A),
								unrewarded_relayer(2, 2, TEST_RELAYER_B)
							]
							.into_iter()
							.collect(),
							..Default::default()
						}
					))),
					UnrewardedRelayersState {
						unrewarded_relayer_entries: 2,
						total_messages: 2,
						last_delivered_nonce: 8,
						..Default::default()
					},
				),
				Error::<TestRuntime, ()>::InvalidUnrewardedRelayersState,
			);
		});
	}

	#[test]
	fn receive_messages_accepts_single_message_with_invalid_payload() {
		run_test(|| {
			let mut invalid_message = message(1, REGULAR_PAYLOAD);
			invalid_message.payload = Vec::new();

			assert_ok!(Pallet::<TestRuntime, ()>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				Ok(vec![invalid_message]).into(),
				1,
				Weight::zero(), /* weight may be zero in this case (all messages are
				                 * improperly encoded) */
			),);

			assert_eq!(InboundLanes::<TestRuntime>::get(TEST_LANE_ID).last_delivered_nonce(), 1,);
		});
	}

	#[test]
	fn receive_messages_accepts_batch_with_message_with_invalid_payload() {
		run_test(|| {
			let mut invalid_message = message(2, REGULAR_PAYLOAD);
			invalid_message.payload = Vec::new();

			assert_ok!(Pallet::<TestRuntime, ()>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				Ok(
					vec![message(1, REGULAR_PAYLOAD), invalid_message, message(3, REGULAR_PAYLOAD),]
				)
				.into(),
				3,
				REGULAR_PAYLOAD.declared_weight + REGULAR_PAYLOAD.declared_weight,
			),);

			assert_eq!(InboundLanes::<TestRuntime>::get(TEST_LANE_ID).last_delivered_nonce(), 3,);
		});
	}

	#[test]
	fn actual_dispatch_weight_does_not_overlow() {
		run_test(|| {
			let message1 = message(1, message_payload(0, u64::MAX / 2));
			let message2 = message(2, message_payload(0, u64::MAX / 2));
			let message3 = message(3, message_payload(0, u64::MAX / 2));

			assert_noop!(
				Pallet::<TestRuntime, ()>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					// this may cause overflow if source chain storage is invalid
					Ok(vec![message1, message2, message3]).into(),
					3,
					Weight::MAX,
				),
				Error::<TestRuntime, ()>::InsufficientDispatchWeight
			);
			assert_eq!(InboundLanes::<TestRuntime>::get(TEST_LANE_ID).last_delivered_nonce(), 0);
		});
	}

	#[test]
	fn ref_time_refund_from_receive_messages_proof_works() {
		run_test(|| {
			fn submit_with_unspent_weight(
				nonce: MessageNonce,
				unspent_weight: u64,
			) -> (Weight, Weight) {
				let mut payload = REGULAR_PAYLOAD;
				*payload.dispatch_result.unspent_weight.ref_time_mut() = unspent_weight;
				let proof = Ok(vec![message(nonce, payload)]).into();
				let messages_count = 1;
				let pre_dispatch_weight =
					<TestRuntime as Config>::WeightInfo::receive_messages_proof_weight(
						&proof,
						messages_count,
						REGULAR_PAYLOAD.declared_weight,
					);
				let result = Pallet::<TestRuntime>::receive_messages_proof(
					RuntimeOrigin::signed(1),
					TEST_RELAYER_A,
					proof,
					messages_count,
					REGULAR_PAYLOAD.declared_weight,
				)
				.expect("delivery has failed");
				let post_dispatch_weight =
					result.actual_weight.expect("receive_messages_proof always returns Some");

				// message delivery transactions are never free
				assert_eq!(result.pays_fee, Pays::Yes);

				(pre_dispatch_weight, post_dispatch_weight)
			}

			// when dispatch is returning `unspent_weight < declared_weight`
			let (pre, post) = submit_with_unspent_weight(1, 1);
			assert_eq!(post.ref_time(), pre.ref_time() - 1);

			// when dispatch is returning `unspent_weight = declared_weight`
			let (pre, post) =
				submit_with_unspent_weight(2, REGULAR_PAYLOAD.declared_weight.ref_time());
			assert_eq!(
				post.ref_time(),
				pre.ref_time() - REGULAR_PAYLOAD.declared_weight.ref_time()
			);

			// when dispatch is returning `unspent_weight > declared_weight`
			let (pre, post) =
				submit_with_unspent_weight(3, REGULAR_PAYLOAD.declared_weight.ref_time() + 1);
			assert_eq!(
				post.ref_time(),
				pre.ref_time() - REGULAR_PAYLOAD.declared_weight.ref_time()
			);

			// when there's no unspent weight
			let (pre, post) = submit_with_unspent_weight(4, 0);
			assert_eq!(post.ref_time(), pre.ref_time());

			// when dispatch is returning `unspent_weight < declared_weight`
			let (pre, post) = submit_with_unspent_weight(5, 1);
			assert_eq!(post.ref_time(), pre.ref_time() - 1);
		});
	}

	#[test]
	fn proof_size_refund_from_receive_messages_proof_works() {
		run_test(|| {
			let max_entries = crate::mock::MaxUnrewardedRelayerEntriesAtInboundLane::get() as usize;

			// if there's maximal number of unrewarded relayer entries at the inbound lane, then
			// `proof_size` is unchanged in post-dispatch weight
			let proof: TestMessagesProof = Ok(vec![message(101, REGULAR_PAYLOAD)]).into();
			let messages_count = 1;
			let pre_dispatch_weight =
				<TestRuntime as Config>::WeightInfo::receive_messages_proof_weight(
					&proof,
					messages_count,
					REGULAR_PAYLOAD.declared_weight,
				);
			InboundLanes::<TestRuntime>::insert(
				TEST_LANE_ID,
				StoredInboundLaneData(InboundLaneData {
					relayers: vec![
						UnrewardedRelayer {
							relayer: 42,
							messages: DeliveredMessages { begin: 0, end: 100 }
						};
						max_entries
					]
					.into_iter()
					.collect(),
					last_confirmed_nonce: 0,
				}),
			);
			let post_dispatch_weight = Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof.clone(),
				messages_count,
				REGULAR_PAYLOAD.declared_weight,
			)
			.unwrap()
			.actual_weight
			.unwrap();
			assert_eq!(post_dispatch_weight.proof_size(), pre_dispatch_weight.proof_size());

			// if count of unrewarded relayer entries is less than maximal, then some `proof_size`
			// must be refunded
			InboundLanes::<TestRuntime>::insert(
				TEST_LANE_ID,
				StoredInboundLaneData(InboundLaneData {
					relayers: vec![
						UnrewardedRelayer {
							relayer: 42,
							messages: DeliveredMessages { begin: 0, end: 100 }
						};
						max_entries - 1
					]
					.into_iter()
					.collect(),
					last_confirmed_nonce: 0,
				}),
			);
			let post_dispatch_weight = Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				messages_count,
				REGULAR_PAYLOAD.declared_weight,
			)
			.unwrap()
			.actual_weight
			.unwrap();
			assert!(
				post_dispatch_weight.proof_size() < pre_dispatch_weight.proof_size(),
				"Expected post-dispatch PoV {} to be less than pre-dispatch PoV {}",
				post_dispatch_weight.proof_size(),
				pre_dispatch_weight.proof_size(),
			);
		});
	}

	#[test]
	fn messages_delivered_callbacks_are_called() {
		run_test(|| {
			send_regular_message(TEST_LANE_ID);
			send_regular_message(TEST_LANE_ID);
			send_regular_message(TEST_LANE_ID);

			// messages 1+2 are confirmed in 1 tx, message 3 in a separate tx
			// dispatch of message 2 has failed
			let mut delivered_messages_1_and_2 = DeliveredMessages::new(1);
			delivered_messages_1_and_2.note_dispatched_message();
			let messages_1_and_2_proof = Ok((
				TEST_LANE_ID,
				InboundLaneData {
					last_confirmed_nonce: 0,
					relayers: vec![UnrewardedRelayer {
						relayer: 0,
						messages: delivered_messages_1_and_2.clone(),
					}]
					.into_iter()
					.collect(),
				},
			));
			let delivered_message_3 = DeliveredMessages::new(3);
			let messages_3_proof = Ok((
				TEST_LANE_ID,
				InboundLaneData {
					last_confirmed_nonce: 0,
					relayers: vec![UnrewardedRelayer { relayer: 0, messages: delivered_message_3 }]
						.into_iter()
						.collect(),
				},
			));

			// first tx with messages 1+2
			assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				TestMessagesDeliveryProof(messages_1_and_2_proof),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 2,
					total_messages: 2,
					last_delivered_nonce: 2,
				},
			));
			// second tx with message 3
			assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				TestMessagesDeliveryProof(messages_3_proof),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 1,
					total_messages: 1,
					last_delivered_nonce: 3,
				},
			));
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rejects_proof_if_trying_to_confirm_more_messages_than_expected(
	) {
		run_test(|| {
			// send message first to be able to check that delivery_proof fails later
			send_regular_message(TEST_LANE_ID);

			// 1) InboundLaneData declares that the `last_confirmed_nonce` is 1;
			// 2) InboundLaneData has no entries => `InboundLaneData::last_delivered_nonce()`
			//    returns `last_confirmed_nonce`;
			// 3) it means that we're going to confirm delivery of messages 1..=1;
			// 4) so the number of declared messages (see `UnrewardedRelayersState`) is `0` and
			//    numer of actually confirmed messages is `1`.
			assert_noop!(
				Pallet::<TestRuntime>::receive_messages_delivery_proof(
					RuntimeOrigin::signed(1),
					TestMessagesDeliveryProof(Ok((
						TEST_LANE_ID,
						InboundLaneData { last_confirmed_nonce: 1, relayers: Default::default() },
					))),
					UnrewardedRelayersState { last_delivered_nonce: 1, ..Default::default() },
				),
				Error::<TestRuntime, ()>::ReceivalConfirmation(
					ReceivalConfirmationError::TryingToConfirmMoreMessagesThanExpected
				),
			);
		});
	}

	#[test]
	fn storage_keys_computed_properly() {
		assert_eq!(
			PalletOperatingMode::<TestRuntime>::storage_value_final_key().to_vec(),
			bp_messages::storage_keys::operating_mode_key("Messages").0,
		);

		assert_eq!(
			OutboundMessages::<TestRuntime>::storage_map_final_key(MessageKey {
				lane_id: TEST_LANE_ID,
				nonce: 42
			}),
			bp_messages::storage_keys::message_key("Messages", &TEST_LANE_ID, 42).0,
		);

		assert_eq!(
			OutboundLanes::<TestRuntime>::storage_map_final_key(TEST_LANE_ID),
			bp_messages::storage_keys::outbound_lane_data_key("Messages", &TEST_LANE_ID).0,
		);

		assert_eq!(
			InboundLanes::<TestRuntime>::storage_map_final_key(TEST_LANE_ID),
			bp_messages::storage_keys::inbound_lane_data_key("Messages", &TEST_LANE_ID).0,
		);
	}

	#[test]
	fn inbound_message_details_works() {
		run_test(|| {
			assert_eq!(
				Pallet::<TestRuntime>::inbound_message_data(
					TEST_LANE_ID,
					REGULAR_PAYLOAD.encode(),
					OutboundMessageDetails { nonce: 0, dispatch_weight: Weight::zero(), size: 0 },
				),
				InboundMessageDetails { dispatch_weight: REGULAR_PAYLOAD.declared_weight },
			);
		});
	}

	#[test]
	fn on_idle_callback_respects_remaining_weight() {
		run_test(|| {
			send_regular_message(TEST_LANE_ID);
			send_regular_message(TEST_LANE_ID);
			send_regular_message(TEST_LANE_ID);
			send_regular_message(TEST_LANE_ID);

			assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						last_confirmed_nonce: 4,
						relayers: vec![unrewarded_relayer(1, 4, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
					},
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 4,
					total_messages: 4,
					last_delivered_nonce: 4,
				},
			));

			// all 4 messages may be pruned now
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().latest_received_nonce,
				4
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				1
			);
			System::<TestRuntime>::set_block_number(2);

			// if passed wight is too low to do anything
			let dbw = DbWeight::get();
			assert_eq!(
				Pallet::<TestRuntime, ()>::on_idle(0, dbw.reads_writes(1, 1)),
				Weight::zero(),
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				1
			);

			// if passed wight is enough to prune single message
			assert_eq!(
				Pallet::<TestRuntime, ()>::on_idle(0, dbw.reads_writes(1, 2)),
				dbw.reads_writes(1, 2),
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				2
			);

			// if passed wight is enough to prune two more messages
			assert_eq!(
				Pallet::<TestRuntime, ()>::on_idle(0, dbw.reads_writes(1, 3)),
				dbw.reads_writes(1, 3),
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				4
			);

			// if passed wight is enough to prune many messages
			assert_eq!(
				Pallet::<TestRuntime, ()>::on_idle(0, dbw.reads_writes(100, 100)),
				dbw.reads_writes(1, 2),
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				5
			);
		});
	}

	#[test]
	fn on_idle_callback_is_rotating_lanes_to_prune() {
		run_test(|| {
			// send + receive confirmation for lane 1
			send_regular_message(TEST_LANE_ID);
			receive_messages_delivery_proof();
			// send + receive confirmation for lane 2
			send_regular_message(TEST_LANE_ID_2);
			assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID_2,
					InboundLaneData {
						last_confirmed_nonce: 1,
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
					},
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 1,
					total_messages: 1,
					last_delivered_nonce: 1,
				},
			));

			// nothing is pruned yet
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().latest_received_nonce,
				1
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				1
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID_2).data().latest_received_nonce,
				1
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID_2).data().oldest_unpruned_nonce,
				1
			);

			// in block#2.on_idle lane messages of lane 1 are pruned
			let dbw = DbWeight::get();
			System::<TestRuntime>::set_block_number(2);
			assert_eq!(
				Pallet::<TestRuntime, ()>::on_idle(0, dbw.reads_writes(100, 100)),
				dbw.reads_writes(1, 2),
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				2
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID_2).data().oldest_unpruned_nonce,
				1
			);

			// in block#3.on_idle lane messages of lane 2 are pruned
			System::<TestRuntime>::set_block_number(3);

			assert_eq!(
				Pallet::<TestRuntime, ()>::on_idle(0, dbw.reads_writes(100, 100)),
				dbw.reads_writes(1, 2),
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID).data().oldest_unpruned_nonce,
				2
			);
			assert_eq!(
				outbound_lane::<TestRuntime, ()>(TEST_LANE_ID_2).data().oldest_unpruned_nonce,
				2
			);
		});
	}

	#[test]
	fn outbound_message_from_unconfigured_lane_is_rejected() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime, ()>::validate_message(TEST_LANE_ID_3, &REGULAR_PAYLOAD,),
				Error::<TestRuntime, ()>::InactiveOutboundLane,
			);
		});
	}

	#[test]
	fn test_bridge_messages_call_is_correctly_defined() {
		let account_id = 1;
		let message_proof: TestMessagesProof = Ok(vec![message(1, REGULAR_PAYLOAD)]).into();
		let message_delivery_proof = TestMessagesDeliveryProof(Ok((
			TEST_LANE_ID,
			InboundLaneData {
				last_confirmed_nonce: 1,
				relayers: vec![UnrewardedRelayer {
					relayer: 0,
					messages: DeliveredMessages::new(1),
				}]
				.into_iter()
				.collect(),
			},
		)));
		let unrewarded_relayer_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			total_messages: 1,
			last_delivered_nonce: 1,
			..Default::default()
		};

		let direct_receive_messages_proof_call = Call::<TestRuntime>::receive_messages_proof {
			relayer_id_at_bridged_chain: account_id,
			proof: message_proof.clone(),
			messages_count: 1,
			dispatch_weight: REGULAR_PAYLOAD.declared_weight,
		};
		let indirect_receive_messages_proof_call = BridgeMessagesCall::<
			AccountId,
			TestMessagesProof,
			TestMessagesDeliveryProof,
		>::receive_messages_proof {
			relayer_id_at_bridged_chain: account_id,
			proof: message_proof,
			messages_count: 1,
			dispatch_weight: REGULAR_PAYLOAD.declared_weight,
		};
		assert_eq!(
			direct_receive_messages_proof_call.encode(),
			indirect_receive_messages_proof_call.encode()
		);

		let direct_receive_messages_delivery_proof_call =
			Call::<TestRuntime>::receive_messages_delivery_proof {
				proof: message_delivery_proof.clone(),
				relayers_state: unrewarded_relayer_state.clone(),
			};
		let indirect_receive_messages_delivery_proof_call = BridgeMessagesCall::<
			AccountId,
			TestMessagesProof,
			TestMessagesDeliveryProof,
		>::receive_messages_delivery_proof {
			proof: message_delivery_proof,
			relayers_state: unrewarded_relayer_state,
		};
		assert_eq!(
			direct_receive_messages_delivery_proof_call.encode(),
			indirect_receive_messages_delivery_proof_call.encode()
		);
	}

	generate_owned_bridge_module_tests!(
		MessagesOperatingMode::Basic(BasicOperatingMode::Normal),
		MessagesOperatingMode::Basic(BasicOperatingMode::Halted)
	);

	#[test]
	fn inbound_storage_extra_proof_size_bytes_works() {
		fn relayer_entry() -> UnrewardedRelayer<TestRelayer> {
			UnrewardedRelayer { relayer: 42u64, messages: DeliveredMessages { begin: 0, end: 100 } }
		}

		fn storage(relayer_entries: usize) -> RuntimeInboundLaneStorage<TestRuntime, ()> {
			RuntimeInboundLaneStorage {
				lane_id: Default::default(),
				cached_data: Some(InboundLaneData {
					relayers: vec![relayer_entry(); relayer_entries].into_iter().collect(),
					last_confirmed_nonce: 0,
				}),
				_phantom: Default::default(),
			}
		}

		let max_entries = crate::mock::MaxUnrewardedRelayerEntriesAtInboundLane::get() as usize;

		// when we have exactly `MaxUnrewardedRelayerEntriesAtInboundLane` unrewarded relayers
		assert_eq!(storage(max_entries).extra_proof_size_bytes(), 0);

		// when we have less than `MaxUnrewardedRelayerEntriesAtInboundLane` unrewarded relayers
		assert_eq!(
			storage(max_entries - 1).extra_proof_size_bytes(),
			relayer_entry().encode().len() as u64
		);
		assert_eq!(
			storage(max_entries - 2).extra_proof_size_bytes(),
			2 * relayer_entry().encode().len() as u64
		);

		// when we have more than `MaxUnrewardedRelayerEntriesAtInboundLane` unrewarded relayers
		// (shall not happen in practice)
		assert_eq!(storage(max_entries + 1).extra_proof_size_bytes(), 0);
	}

	#[test]
	fn maybe_outbound_lanes_count_returns_correct_value() {
		assert_eq!(
			MaybeOutboundLanesCount::<TestRuntime, ()>::get(),
			Some(mock::ActiveOutboundLanes::get().len() as u32)
		);
	}
}
