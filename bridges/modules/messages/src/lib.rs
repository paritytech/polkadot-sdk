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
	ensure_maximal_message_dispatch, ensure_weights_are_correct, WeightInfoExt,
	EXPECTED_DEFAULT_MESSAGE_LENGTH, EXTRA_STORAGE_PROOF_SIZE,
};

use crate::{
	inbound_lane::{InboundLane, InboundLaneStorage},
	outbound_lane::{OutboundLane, OutboundLaneStorage, ReceptionConfirmationError},
};

use bp_header_chain::HeaderChain;
use bp_messages::{
	source_chain::{
		DeliveryConfirmationPayments, FromBridgedChainMessagesDeliveryProof, OnMessagesDelivered,
		SendMessageArtifacts,
	},
	target_chain::{
		DeliveryPayments, DispatchMessage, FromBridgedChainMessagesProof, MessageDispatch,
		ProvedLaneMessages, ProvedMessages,
	},
	ChainWithMessages, DeliveredMessages, InboundLaneData, InboundMessageDetails, MessageKey,
	MessageNonce, MessagePayload, MessagesOperatingMode, OutboundLaneData, OutboundMessageDetails,
	UnrewardedRelayersState, VerificationError,
};
use bp_runtime::{
	AccountIdOf, BasicOperatingMode, HashOf, OwnedBridgeModule, PreComputedSize, RangeInclusiveExt,
	Size,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::PostDispatchInfo, ensure, fail, traits::Get, DefaultNoBound};
use sp_runtime::traits::UniqueSaturatedFrom;
use sp_std::{marker::PhantomData, prelude::*};

mod inbound_lane;
mod outbound_lane;
mod proofs;
mod tests;
mod weights_ext;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

pub use pallet::*;
#[cfg(feature = "test-helpers")]
pub use tests::*;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-messages";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bp_messages::{LaneIdType, ReceivedMessages, ReceptionResult};
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

		/// This chain type.
		type ThisChain: ChainWithMessages;
		/// Bridged chain type.
		type BridgedChain: ChainWithMessages;
		/// Bridged chain headers provider.
		type BridgedHeaderChain: HeaderChain<Self::BridgedChain>;

		/// Get all active outbound lanes that the message pallet is serving.
		type ActiveOutboundLanes: Get<&'static [LaneId]>;

		/// Payload type of outbound messages. This payload is dispatched on the bridged chain.
		type OutboundPayload: Parameter + Size;
		/// Payload type of inbound messages. This payload is dispatched on this chain.
		type InboundPayload: Decode;
		/// Lane identifier type.
		type LaneId: LaneIdType;

		/// Handler for relayer payments that happen during message delivery transaction.
		type DeliveryPayments: DeliveryPayments<Self::AccountId>;
		/// Handler for relayer payments that happen during message delivery confirmation
		/// transaction.
		type DeliveryConfirmationPayments: DeliveryConfirmationPayments<
			Self::AccountId,
			Self::LaneId,
		>;
		/// Delivery confirmation callback.
		type OnMessagesDelivered: OnMessagesDelivered<Self::LaneId>;

		/// Message dispatch handler.
		type MessageDispatch: MessageDispatch<
			DispatchPayload = Self::InboundPayload,
			LaneId = Self::LaneId,
		>;
	}

	/// Shortcut to this chain type for Config.
	pub type ThisChainOf<T, I> = <T as Config<I>>::ThisChain;
	/// Shortcut to bridged chain type for Config.
	pub type BridgedChainOf<T, I> = <T as Config<I>>::BridgedChain;
	/// Shortcut to bridged header chain type for Config.
	pub type BridgedHeaderChainOf<T, I> = <T as Config<I>>::BridgedHeaderChain;
	/// Shortcut to lane identifier type for Config.
	pub type LaneIdOf<T, I> = <T as Config<I>>::LaneId;

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
		#[pallet::weight(T::WeightInfo::receive_messages_proof_weight(&**proof, *messages_count, *dispatch_weight))]
		pub fn receive_messages_proof(
			origin: OriginFor<T>,
			relayer_id_at_bridged_chain: AccountIdOf<BridgedChainOf<T, I>>,
			proof: Box<FromBridgedChainMessagesProof<HashOf<BridgedChainOf<T, I>>, T::LaneId>>,
			messages_count: u32,
			dispatch_weight: Weight,
		) -> DispatchResultWithPostInfo {
			Self::ensure_not_halted().map_err(Error::<T, I>::BridgeModule)?;
			let relayer_id_at_this_chain = ensure_signed(origin)?;

			// reject transactions that are declaring too many messages
			ensure!(
				MessageNonce::from(messages_count) <=
					BridgedChainOf::<T, I>::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
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
				&*proof,
				messages_count,
				dispatch_weight,
			);
			let mut actual_weight = declared_weight;

			// verify messages proof && convert proof into messages
			let messages = verify_and_decode_messages_proof::<T, I>(*proof, messages_count)
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
						ReceptionResult::Dispatched(dispatch_result) => {
							valid_messages += 1;
							dispatch_result.unspent_weight
						},
						ReceptionResult::InvalidNonce |
						ReceptionResult::TooManyUnrewardedRelayers |
						ReceptionResult::TooManyUnconfirmedMessages => message_dispatch_weight,
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
			proof: FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChainOf<T, I>>, T::LaneId>,
			mut relayers_state: UnrewardedRelayersState,
		) -> DispatchResultWithPostInfo {
			Self::ensure_not_halted().map_err(Error::<T, I>::BridgeModule)?;

			let proof_size = proof.size();
			let confirmation_relayer = ensure_signed(origin)?;
			let (lane_id, lane_data) = proofs::verify_messages_delivery_proof::<T, I>(proof)
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
				.map_err(Error::<T, I>::ReceptionConfirmation)?;

			if let Some(confirmed_messages) = confirmed_messages {
				// emit 'delivered' event
				let received_range = confirmed_messages.begin..=confirmed_messages.end;
				Self::deposit_event(Event::MessagesDelivered {
					lane_id: lane_id.into(),
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
			lane_id: T::LaneId,
			/// Nonce of accepted message.
			nonce: MessageNonce,
		},
		/// Messages have been received from the bridged chain.
		MessagesReceived(
			/// Result of received messages dispatch.
<<<<<<< HEAD
			Vec<ReceivedMessages<<T::MessageDispatch as MessageDispatch>::DispatchLevelResult>>,
=======
			ReceivedMessages<
				<T::MessageDispatch as MessageDispatch>::DispatchLevelResult,
				T::LaneId,
			>,
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
		),
		/// Messages in the inclusive range have been delivered to the bridged chain.
		MessagesDelivered {
			/// Lane for which the delivery has been confirmed.
			lane_id: T::LaneId,
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
		ReceptionConfirmation(ReceptionConfirmationError),
		/// Error generated by the `OwnedBridgeModule` trait.
		BridgeModule(bp_runtime::OwnedBridgeModuleError),
	}

	/// Optional pallet owner.
	///
	/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
	/// `None`, then there are no direct ways to halt/resume pallet operations, but other
	/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
	/// flag directly or call the `set_operating_mode`).
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
<<<<<<< HEAD
		StorageMap<_, Blake2_128Concat, LaneId, StoredInboundLaneData<T, I>, ValueQuery>;
=======
		StorageMap<_, Blake2_128Concat, T::LaneId, StoredInboundLaneData<T, I>, OptionQuery>;
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))

	/// Map of lane id => outbound lane data.
	#[pallet::storage]
	pub type OutboundLanes<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = Blake2_128Concat,
		Key = T::LaneId,
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
		StorageMap<_, Blake2_128Concat, MessageKey<T::LaneId>, StoredMessagePayload<T, I>>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// Initial pallet operating mode.
		pub operating_mode: MessagesOperatingMode,
		/// Initial pallet owner.
		pub owner: Option<T::AccountId>,
<<<<<<< HEAD
=======
		/// Opened lanes.
		pub opened_lanes: Vec<T::LaneId>,
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
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
		pub fn outbound_message_data(
			lane: T::LaneId,
			nonce: MessageNonce,
		) -> Option<MessagePayload> {
			OutboundMessages::<T, I>::get(MessageKey { lane_id: lane, nonce }).map(Into::into)
		}

		/// Prepare data, related to given inbound message.
		pub fn inbound_message_data(
			lane: T::LaneId,
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
<<<<<<< HEAD
		pub fn outbound_lane_data(lane: LaneId) -> OutboundLaneData {
=======
		pub fn outbound_lane_data(lane: T::LaneId) -> Option<OutboundLaneData> {
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
			OutboundLanes::<T, I>::get(lane)
		}

		/// Return inbound lane data.
		pub fn inbound_lane_data(
<<<<<<< HEAD
			lane: LaneId,
		) -> InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>> {
			InboundLanes::<T, I>::get(lane).0
=======
			lane: T::LaneId,
		) -> Option<InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>> {
			InboundLanes::<T, I>::get(lane).map(|lane| lane.0)
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
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
<<<<<<< HEAD
	lane_id: LaneId,
=======
	lane_id: T::LaneId,
	lane: OutboundLane<RuntimeOutboundLaneStorage<T, I>>,
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
	payload: StoredMessagePayload<T, I>,
}

impl<T, I> bp_messages::source_chain::MessagesBridge<T::OutboundPayload, T::LaneId> for Pallet<T, I>
where
	T: Config<I>,
	I: 'static,
{
	type Error = Error<T, I>;
	type SendMessageArgs = SendMessageArgs<T, I>;

	fn validate_message(
<<<<<<< HEAD
		lane: LaneId,
=======
		lane_id: T::LaneId,
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
		message: &T::OutboundPayload,
	) -> Result<SendMessageArgs<T, I>, Self::Error> {
		ensure_normal_operating_mode::<T, I>()?;

		// let's check if outbound lane is active
		ensure!(T::ActiveOutboundLanes::get().contains(&lane), Error::<T, I>::InactiveOutboundLane);

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

		Pallet::<T, I>::deposit_event(Event::MessageAccepted {
			lane_id: args.lane_id.into(),
			nonce,
		});

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

<<<<<<< HEAD
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
	cached_data: Option<InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>>,
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
	/// `MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX` constant from the pallet configuration. The PoV
	/// of the call includes the maximal size of inbound lane state. If the actual size is smaller,
	/// we may subtract extra bytes from this component.
	pub fn extra_proof_size_bytes(&mut self) -> u64 {
		let max_encoded_len = StoredInboundLaneData::<T, I>::max_encoded_len();
		let relayers_count = self.get_or_init_data().relayers.len();
		let actual_encoded_len =
			InboundLaneData::<AccountIdOf<BridgedChainOf<T, I>>>::encoded_size_hint(relayers_count)
				.unwrap_or(usize::MAX);
		max_encoded_len.saturating_sub(actual_encoded_len) as _
	}
}

impl<T: Config<I>, I: 'static> InboundLaneStorage for RuntimeInboundLaneStorage<T, I> {
	type Relayer = AccountIdOf<BridgedChainOf<T, I>>;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn max_unrewarded_relayer_entries(&self) -> MessageNonce {
		BridgedChainOf::<T, I>::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX
	}

	fn max_unconfirmed_messages(&self) -> MessageNonce {
		BridgedChainOf::<T, I>::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX
	}

	fn get_or_init_data(&mut self) -> InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>> {
		match self.cached_data {
			Some(ref data) => data.clone(),
			None => {
				let data: InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>> =
					InboundLanes::<T, I>::get(self.lane_id).into();
				self.cached_data = Some(data.clone());
				data
			},
		}
	}

	fn set_data(&mut self, data: InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>) {
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
=======
/// Creates new inbound lane object, backed by runtime storage. Lane must be active.
fn active_inbound_lane<T: Config<I>, I: 'static>(
	lane_id: T::LaneId,
) -> Result<InboundLane<RuntimeInboundLaneStorage<T, I>>, Error<T, I>> {
	LanesManager::<T, I>::new()
		.active_inbound_lane(lane_id)
		.map_err(Error::LanesManager)
}

/// Creates new outbound lane object, backed by runtime storage. Lane must be active.
fn active_outbound_lane<T: Config<I>, I: 'static>(
	lane_id: T::LaneId,
) -> Result<OutboundLane<RuntimeOutboundLaneStorage<T, I>>, Error<T, I>> {
	LanesManager::<T, I>::new()
		.active_outbound_lane(lane_id)
		.map_err(Error::LanesManager)
}

/// Creates new outbound lane object, backed by runtime storage.
fn any_state_outbound_lane<T: Config<I>, I: 'static>(
	lane_id: T::LaneId,
) -> Result<OutboundLane<RuntimeOutboundLaneStorage<T, I>>, Error<T, I>> {
	LanesManager::<T, I>::new()
		.any_state_outbound_lane(lane_id)
		.map_err(Error::LanesManager)
>>>>>>> 710e74d (Bridges lane id agnostic for backwards compatibility (#5649))
}

/// Verify messages proof and return proved messages with decoded payload.
fn verify_and_decode_messages_proof<T: Config<I>, I: 'static>(
	proof: FromBridgedChainMessagesProof<HashOf<BridgedChainOf<T, I>>, T::LaneId>,
	messages_count: u32,
) -> Result<
	ProvedMessages<T::LaneId, DispatchMessage<T::InboundPayload, T::LaneId>>,
	VerificationError,
> {
	// `receive_messages_proof` weight formula and `MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX`
	// check guarantees that the `message_count` is sane and Vec<Message> may be allocated.
	// (tx with too many messages will either be rejected from the pool, or will fail earlier)
	proofs::verify_messages_proof::<T, I>(proof, messages_count).map(|messages_by_lane| {
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
