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

#![cfg_attr(not(feature = "std"), no_std)]

pub use crate::weights_ext::{ensure_weights_are_correct, WeightInfoExt};

use crate::inbound_lane::{InboundLane, InboundLaneStorage};
use crate::outbound_lane::{OutboundLane, OutboundLaneStorage};

use bp_message_lane::{
	source_chain::{LaneMessageVerifier, MessageDeliveryAndDispatchPayment, TargetHeaderChain},
	target_chain::{DispatchMessage, MessageDispatch, ProvedLaneMessages, ProvedMessages, SourceHeaderChain},
	total_unrewarded_messages, InboundLaneData, LaneId, MessageData, MessageKey, MessageNonce, MessagePayload,
	OutboundLaneData, UnrewardedRelayersState,
};
use bp_runtime::Size;
use codec::{Decode, Encode};
use frame_support::{
	decl_error, decl_event, decl_module, decl_storage, ensure,
	traits::Get,
	weights::{DispatchClass, Weight},
	Parameter, StorageMap,
};
use frame_system::{ensure_signed, RawOrigin};
use num_traits::{SaturatingAdd, Zero};
use sp_runtime::{traits::BadOrigin, DispatchResult};
use sp_std::{cell::RefCell, marker::PhantomData, prelude::*};

mod inbound_lane;
mod outbound_lane;
mod weights_ext;

pub mod instant_payments;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod mock;

/// The module configuration trait
pub trait Config<I = DefaultInstance>: frame_system::Config {
	// General types

	/// They overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as frame_system::Config>::Event>;
	/// Benchmarks results from runtime we're plugged into.
	type WeightInfo: WeightInfoExt;
	/// Maximal number of messages that may be pruned during maintenance. Maintenance occurs
	/// whenever new message is sent. The reason is that if you want to use lane, you should
	/// be ready to pay for its maintenance.
	type MaxMessagesToPruneAtOnce: Get<MessageNonce>;
	/// Maximal number of unrewarded relayer entries at inbound lane. Unrewarded means that the
	/// relayer has delivered messages, but either confirmations haven't been delivered back to the
	/// source chain, or we haven't received reward confirmations yet.
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
	/// There is no point of making this parameter lesser than MaxUnrewardedRelayerEntriesAtInboundLane,
	/// because then maximal number of relayer entries will be limited by maximal number of messages.
	type MaxUnconfirmedMessagesAtInboundLane: Get<MessageNonce>;

	/// Payload type of outbound messages. This payload is dispatched on the bridged chain.
	type OutboundPayload: Parameter + Size;
	/// Message fee type of outbound messages. This fee is paid on this chain.
	type OutboundMessageFee: From<u32> + Parameter + SaturatingAdd + Zero;

	/// Payload type of inbound messages. This payload is dispatched on this chain.
	type InboundPayload: Decode;
	/// Message fee type of inbound messages. This fee is paid on the bridged chain.
	type InboundMessageFee: Decode;
	/// Identifier of relayer that deliver messages to this chain. Relayer reward is paid on the bridged chain.
	type InboundRelayer: Parameter;

	/// A type which can be turned into an AccountId from a 256-bit hash.
	///
	/// Used when deriving the shared relayer fund account.
	type AccountIdConverter: sp_runtime::traits::Convert<sp_core::hash::H256, Self::AccountId>;

	// Types that are used by outbound_lane (on source chain).

	/// Target header chain.
	type TargetHeaderChain: TargetHeaderChain<Self::OutboundPayload, Self::AccountId>;
	/// Message payload verifier.
	type LaneMessageVerifier: LaneMessageVerifier<Self::AccountId, Self::OutboundPayload, Self::OutboundMessageFee>;
	/// Message delivery payment.
	type MessageDeliveryAndDispatchPayment: MessageDeliveryAndDispatchPayment<Self::AccountId, Self::OutboundMessageFee>;

	// Types that are used by inbound_lane (on target chain).

	/// Source header chain, as it is represented on target chain.
	type SourceHeaderChain: SourceHeaderChain<Self::InboundMessageFee>;
	/// Message dispatch.
	type MessageDispatch: MessageDispatch<Self::InboundMessageFee, DispatchPayload = Self::InboundPayload>;
}

/// Shortcut to messages proof type for Config.
type MessagesProofOf<T, I> =
	<<T as Config<I>>::SourceHeaderChain as SourceHeaderChain<<T as Config<I>>::InboundMessageFee>>::MessagesProof;
/// Shortcut to messages delivery proof type for Config.
type MessagesDeliveryProofOf<T, I> = <<T as Config<I>>::TargetHeaderChain as TargetHeaderChain<
	<T as Config<I>>::OutboundPayload,
	<T as frame_system::Config>::AccountId,
>>::MessagesDeliveryProof;

decl_error! {
	pub enum Error for Module<T: Config<I>, I: Instance> {
		/// All pallet operations are halted.
		Halted,
		/// Message has been treated as invalid by chain verifier.
		MessageRejectedByChainVerifier,
		/// Message has been treated as invalid by lane verifier.
		MessageRejectedByLaneVerifier,
		/// Submitter has failed to pay fee for delivering and dispatching messages.
		FailedToWithdrawMessageFee,
		/// Invalid messages has been submitted.
		InvalidMessagesProof,
		/// Invalid messages dispatch weight has been declared by the relayer.
		InvalidMessagesDispatchWeight,
		/// Invalid messages delivery proof has been submitted.
		InvalidMessagesDeliveryProof,
		/// The relayer has declared invalid unrewarded relayers state in the `receive_messages_delivery_proof` call.
		InvalidUnrewardedRelayersState,
	}
}

decl_storage! {
	trait Store for Module<T: Config<I>, I: Instance = DefaultInstance> as MessageLane {
		/// Optional pallet owner.
		///
		/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
		/// `None`, then there are no direct ways to halt/resume pallet operations, but other
		/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
		/// flag directly or call the `halt_operations`).
		pub ModuleOwner get(fn module_owner): Option<T::AccountId>;
		/// If true, all pallet transactions are failed immediately.
		pub IsHalted get(fn is_halted) config(): bool;
		/// Map of lane id => inbound lane data.
		pub InboundLanes: map hasher(blake2_128_concat) LaneId => InboundLaneData<T::InboundRelayer>;
		/// Map of lane id => outbound lane data.
		pub OutboundLanes: map hasher(blake2_128_concat) LaneId => OutboundLaneData;
		/// All queued outbound messages.
		pub OutboundMessages: map hasher(blake2_128_concat) MessageKey => Option<MessageData<T::OutboundMessageFee>>;
	}
	add_extra_genesis {
		config(phantom): sp_std::marker::PhantomData<I>;
		config(owner): Option<T::AccountId>;
		build(|config| {
			if let Some(ref owner) = config.owner {
				<ModuleOwner<T, I>>::put(owner);
			}
		})
	}
}

decl_event!(
	pub enum Event<T, I = DefaultInstance> where
		<T as frame_system::Config>::AccountId,
	{
		/// Message has been accepted and is waiting to be delivered.
		MessageAccepted(LaneId, MessageNonce),
		/// Messages in the inclusive range have been delivered and processed by the bridged chain.
		MessagesDelivered(LaneId, MessageNonce, MessageNonce),
		/// Phantom member, never used.
		Dummy(PhantomData<(AccountId, I)>),
	}
);

decl_module! {
	pub struct Module<T: Config<I>, I: Instance = DefaultInstance> for enum Call where origin: T::Origin {
		/// Deposit one of this module's events by using the default implementation.
		fn deposit_event() = default;

		/// Ensure runtime invariants.
		fn on_runtime_upgrade() -> Weight {
			let reads = T::MessageDeliveryAndDispatchPayment::initialize(
				&Self::relayer_fund_account_id()
			);
			T::DbWeight::get().reads(reads as u64)
		}

		/// Change `ModuleOwner`.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[weight = (T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational)]
		pub fn set_owner(origin, new_owner: Option<T::AccountId>) {
			ensure_owner_or_root::<T, I>(origin)?;
			match new_owner {
				Some(new_owner) => {
					ModuleOwner::<T, I>::put(&new_owner);
					frame_support::debug::info!("Setting pallet Owner to: {:?}", new_owner);
				},
				None => {
					ModuleOwner::<T, I>::kill();
					frame_support::debug::info!("Removed Owner of pallet.");
				},
			}
		}

		/// Halt all pallet operations. Operations may be resumed using `resume_operations` call.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[weight = (T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational)]
		pub fn halt_operations(origin) {
			ensure_owner_or_root::<T, I>(origin)?;
			IsHalted::<I>::put(true);
			frame_support::debug::warn!("Stopping pallet operations.");
		}

		/// Resume all pallet operations. May be called even if pallet is halted.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[weight = (T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational)]
		pub fn resume_operations(origin) {
			ensure_owner_or_root::<T, I>(origin)?;
			IsHalted::<I>::put(false);
			frame_support::debug::info!("Resuming pallet operations.");
		}

		/// Send message over lane.
		#[weight = T::WeightInfo::send_message_overhead()
			.saturating_add(T::WeightInfo::send_message_size_overhead(Size::size_hint(payload)))
		]
		pub fn send_message(
			origin,
			lane_id: LaneId,
			payload: T::OutboundPayload,
			delivery_and_dispatch_fee: T::OutboundMessageFee,
		) -> DispatchResult {
			ensure_operational::<T, I>()?;
			let submitter = origin.into().map_err(|_| BadOrigin)?;

			// let's first check if message can be delivered to target chain
			T::TargetHeaderChain::verify_message(&payload)
				.map_err(|err| {
					frame_support::debug::trace!(
						"Message to lane {:?} is rejected by target chain: {:?}",
						lane_id,
						err,
					);

					Error::<T, I>::MessageRejectedByChainVerifier
				})?;

			// now let's enforce any additional lane rules
			T::LaneMessageVerifier::verify_message(
				&submitter,
				&delivery_and_dispatch_fee,
				&lane_id,
				&payload,
			).map_err(|err| {
				frame_support::debug::trace!(
					"Message to lane {:?} is rejected by lane verifier: {:?}",
					lane_id,
					err,
				);

				Error::<T, I>::MessageRejectedByLaneVerifier
			})?;

			// let's withdraw delivery and dispatch fee from submitter
			T::MessageDeliveryAndDispatchPayment::pay_delivery_and_dispatch_fee(
				&submitter,
				&delivery_and_dispatch_fee,
				&Self::relayer_fund_account_id(),
			).map_err(|err| {
				frame_support::debug::trace!(
					"Message to lane {:?} is rejected because submitter {:?} is unable to pay fee {:?}: {:?}",
					lane_id,
					submitter,
					delivery_and_dispatch_fee,
					err,
				);

				Error::<T, I>::FailedToWithdrawMessageFee
			})?;

			// finally, save message in outbound storage and emit event
			let mut lane = outbound_lane::<T, I>(lane_id);
			let nonce = lane.send_message(MessageData {
				payload: payload.encode(),
				fee: delivery_and_dispatch_fee,
			});
			lane.prune_messages(T::MaxMessagesToPruneAtOnce::get());

			frame_support::debug::trace!(
				"Accepted message {} to lane {:?}",
				nonce,
				lane_id,
			);

			Self::deposit_event(RawEvent::MessageAccepted(lane_id, nonce));

			Ok(())
		}

		/// Receive messages proof from bridged chain.
		///
		/// The weight of the call assumes that the transaction always brings outbound lane
		/// state update. Because of that, the submitter (relayer) has no benefit of not including
		/// this data in the transaction, so reward confirmations lags should be minimal.
		#[weight = T::WeightInfo::receive_messages_proof_overhead()
			.saturating_add(T::WeightInfo::receive_messages_proof_outbound_lane_state_overhead())
			.saturating_add(T::WeightInfo::receive_messages_proof_messages_overhead(*messages_count))
			.saturating_add(*dispatch_weight)
		]
		pub fn receive_messages_proof(
			origin,
			relayer_id: T::InboundRelayer,
			proof: MessagesProofOf<T, I>,
			messages_count: MessageNonce,
			dispatch_weight: Weight,
		) -> DispatchResult {
			ensure_operational::<T, I>()?;
			let _ = ensure_signed(origin)?;

			// verify messages proof && convert proof into messages
			let messages = verify_and_decode_messages_proof::<
				T::SourceHeaderChain,
				T::InboundMessageFee,
				T::InboundPayload,
			>(proof, messages_count)
				.map_err(|err| {
					frame_support::debug::trace!(
						"Rejecting invalid messages proof: {:?}",
						err,
					);

					Error::<T, I>::InvalidMessagesProof
				})?;

			// verify that relayer is paying actual dispatch weight
			let actual_dispatch_weight: Weight = messages
				.values()
				.map(|lane_messages| lane_messages
					.messages
					.iter()
					.map(T::MessageDispatch::dispatch_weight)
					.sum::<Weight>()
				)
				.sum();
			if dispatch_weight < actual_dispatch_weight {
				frame_support::debug::trace!(
					"Rejecting messages proof because of dispatch weight mismatch: declared={}, expected={}",
					dispatch_weight,
					actual_dispatch_weight,
				);

				return Err(Error::<T, I>::InvalidMessagesDispatchWeight.into());
			}

			// dispatch messages and (optionally) update lane(s) state(s)
			let mut total_messages = 0;
			let mut valid_messages = 0;
			for (lane_id, lane_data) in messages {
				let mut lane = inbound_lane::<T, I>(lane_id);

				if let Some(lane_state) = lane_data.lane_state {
					let updated_latest_confirmed_nonce = lane.receive_state_update(lane_state);
					if let Some(updated_latest_confirmed_nonce) = updated_latest_confirmed_nonce {
						frame_support::debug::trace!(
							"Received lane {:?} state update: latest_confirmed_nonce={}",
							lane_id,
							updated_latest_confirmed_nonce,
						);
					}
				}

				for message in lane_data.messages {
					debug_assert_eq!(message.key.lane_id, lane_id);

					total_messages += 1;
					if lane.receive_message::<T::MessageDispatch>(relayer_id.clone(), message.key.nonce, message.data) {
						valid_messages += 1;
					}
				}
			}

			frame_support::debug::trace!(
				"Received messages: total={}, valid={}",
				total_messages,
				valid_messages,
			);

			Ok(())
		}

		/// Receive messages delivery proof from bridged chain.
		#[weight = T::WeightInfo::receive_messages_delivery_proof_overhead()
			.saturating_add(T::WeightInfo::receive_messages_delivery_proof_messages_overhead(
				relayers_state.total_messages
			))
			.saturating_add(T::WeightInfo::receive_messages_delivery_proof_relayers_overhead(
				relayers_state.unrewarded_relayer_entries
			))
		]
		pub fn receive_messages_delivery_proof(
			origin,
			proof: MessagesDeliveryProofOf<T, I>,
			relayers_state: UnrewardedRelayersState,
		) -> DispatchResult {
			ensure_operational::<T, I>()?;

			let confirmation_relayer = ensure_signed(origin)?;
			let (lane_id, lane_data) = T::TargetHeaderChain::verify_messages_delivery_proof(proof).map_err(|err| {
				frame_support::debug::trace!(
					"Rejecting invalid messages delivery proof: {:?}",
					err,
				);

				Error::<T, I>::InvalidMessagesDeliveryProof
			})?;

			// verify that the relayer has declared correct `lane_data::relayers` state
			// (we only care about total number of entries and messages, because this affects call weight)
			ensure!(
				total_unrewarded_messages(&lane_data.relayers) == relayers_state.total_messages
					&& lane_data.relayers.len() as MessageNonce == relayers_state.unrewarded_relayer_entries,
				Error::<T, I>::InvalidUnrewardedRelayersState
			);

			// mark messages as delivered
			let mut lane = outbound_lane::<T, I>(lane_id);
			let last_delivered_nonce = lane_data.last_delivered_nonce();
			let received_range = lane.confirm_delivery(last_delivered_nonce);
			if let Some(received_range) = received_range {
				Self::deposit_event(RawEvent::MessagesDelivered(lane_id, received_range.0, received_range.1));

				// reward relayers that have delivered messages
				// this loop is bounded by `T::MaxUnrewardedRelayerEntriesAtInboundLane` on the bridged chain
				let relayer_fund_account = Self::relayer_fund_account_id();
				for (nonce_low, nonce_high, relayer) in lane_data.relayers {
					let nonce_begin = sp_std::cmp::max(nonce_low, received_range.0);
					let nonce_end = sp_std::cmp::min(nonce_high, received_range.1);

					// loop won't proceed if current entry is ahead of received range (begin > end).
					// this loop is bound by `T::MaxUnconfirmedMessagesAtInboundLane` on the bridged chain
					let mut relayer_fee: T::OutboundMessageFee = Zero::zero();
					for nonce in nonce_begin..nonce_end + 1 {
						let message_data = OutboundMessages::<T, I>::get(MessageKey {
							lane_id,
							nonce,
						}).expect("message was just confirmed; we never prune unconfirmed messages; qed");
						relayer_fee = relayer_fee.saturating_add(&message_data.fee);
					}

					if !relayer_fee.is_zero() {
						<T as Config<I>>::MessageDeliveryAndDispatchPayment::pay_relayer_reward(
							&confirmation_relayer,
							&relayer,
							&relayer_fee,
							&relayer_fund_account,
						);
					}
				}
			}

			frame_support::debug::trace!(
				"Received messages delivery proof up to (and including) {} at lane {:?}",
				last_delivered_nonce,
				lane_id,
			);

			Ok(())
		}
	}
}

impl<T: Config<I>, I: Instance> Module<T, I> {
	/// Get payload of given outbound message.
	pub fn outbound_message_payload(lane: LaneId, nonce: MessageNonce) -> Option<MessagePayload> {
		OutboundMessages::<T, I>::get(MessageKey { lane_id: lane, nonce }).map(|message_data| message_data.payload)
	}

	/// Get nonce of latest generated message at given outbound lane.
	pub fn outbound_latest_generated_nonce(lane: LaneId) -> MessageNonce {
		OutboundLanes::<I>::get(&lane).latest_generated_nonce
	}

	/// Get nonce of latest confirmed message at given outbound lane.
	pub fn outbound_latest_received_nonce(lane: LaneId) -> MessageNonce {
		OutboundLanes::<I>::get(&lane).latest_received_nonce
	}

	/// Get nonce of latest received message at given inbound lane.
	pub fn inbound_latest_received_nonce(lane: LaneId) -> MessageNonce {
		InboundLanes::<T, I>::get(&lane).last_delivered_nonce()
	}

	/// Get nonce of latest confirmed message at given inbound lane.
	pub fn inbound_latest_confirmed_nonce(lane: LaneId) -> MessageNonce {
		InboundLanes::<T, I>::get(&lane).last_confirmed_nonce
	}

	/// Get state of unrewarded relayers set.
	pub fn inbound_unrewarded_relayers_state(
		lane: bp_message_lane::LaneId,
	) -> bp_message_lane::UnrewardedRelayersState {
		let relayers = InboundLanes::<T, I>::get(&lane).relayers;
		bp_message_lane::UnrewardedRelayersState {
			unrewarded_relayer_entries: relayers.len() as _,
			messages_in_oldest_entry: relayers.front().map(|(begin, end, _)| 1 + end - begin).unwrap_or(0),
			total_messages: total_unrewarded_messages(&relayers),
		}
	}

	/// AccountId of the shared relayer fund account.
	///
	/// This account is passed to `MessageDeliveryAndDispatchPayment` trait, and depending
	/// on the implementation it can be used to store relayers rewards.
	/// See [InstantCurrencyPayments] for a concrete implementation.
	pub fn relayer_fund_account_id() -> T::AccountId {
		use sp_runtime::traits::Convert;
		let encoded_id = bp_runtime::derive_relayer_fund_account_id(bp_runtime::NO_INSTANCE_ID);
		T::AccountIdConverter::convert(encoded_id)
	}
}

/// Getting storage keys for messages and lanes states. These keys are normally used when building
/// messages and lanes states proofs.
///
/// Keep in mind that all functions in this module are **NOT** using passed `T` argument, so any
/// runtime can be passed. E.g. if you're verifying proof from Runtime1 in Runtime2, you only have
/// access to Runtime2 and you may pass it to the functions, where required. This is because our
/// maps are not using any Runtime-specific data in the keys.
///
/// On the other side, passing correct instance is required. So if proof has been crafted by the
/// Instance1, you should verify it using Instance1. This is inconvenient if you're using different
/// instances on different sides of the bridge. I.e. in Runtime1 it is Instance2, but on Runtime2
/// it is Instance42. But there's no other way, but to craft this key manually (which is what I'm
/// trying to avoid here) - by using strings like "Instance2", "OutboundMessages", etc.
pub mod storage_keys {
	use super::*;
	use frame_support::storage::generator::StorageMap;
	use sp_core::storage::StorageKey;

	/// Storage key of the outbound message in the runtime storage.
	pub fn message_key<T: Config<I>, I: Instance>(lane: &LaneId, nonce: MessageNonce) -> StorageKey {
		let message_key = MessageKey { lane_id: *lane, nonce };
		let raw_storage_key = OutboundMessages::<T, I>::storage_map_final_key(message_key);
		StorageKey(raw_storage_key)
	}

	/// Storage key of the outbound message lane state in the runtime storage.
	pub fn outbound_lane_data_key<I: Instance>(lane: &LaneId) -> StorageKey {
		StorageKey(OutboundLanes::<I>::storage_map_final_key(*lane))
	}

	/// Storage key of the inbound message lane state in the runtime storage.
	pub fn inbound_lane_data_key<T: Config<I>, I: Instance>(lane: &LaneId) -> StorageKey {
		StorageKey(InboundLanes::<T, I>::storage_map_final_key(*lane))
	}
}

/// Ensure that the origin is either root, or `ModuleOwner`.
fn ensure_owner_or_root<T: Config<I>, I: Instance>(origin: T::Origin) -> Result<(), BadOrigin> {
	match origin.into() {
		Ok(RawOrigin::Root) => Ok(()),
		Ok(RawOrigin::Signed(ref signer)) if Some(signer) == Module::<T, I>::module_owner().as_ref() => Ok(()),
		_ => Err(BadOrigin),
	}
}

/// Ensure that the pallet is in operational mode (not halted).
fn ensure_operational<T: Config<I>, I: Instance>() -> Result<(), Error<T, I>> {
	if IsHalted::<I>::get() {
		Err(Error::<T, I>::Halted)
	} else {
		Ok(())
	}
}

/// Creates new inbound lane object, backed by runtime storage.
fn inbound_lane<T: Config<I>, I: Instance>(lane_id: LaneId) -> InboundLane<RuntimeInboundLaneStorage<T, I>> {
	InboundLane::new(inbound_lane_storage::<T, I>(lane_id))
}

/// Creates new runtime inbound lane storage.
fn inbound_lane_storage<T: Config<I>, I: Instance>(lane_id: LaneId) -> RuntimeInboundLaneStorage<T, I> {
	RuntimeInboundLaneStorage {
		lane_id,
		cached_data: RefCell::new(None),
		_phantom: Default::default(),
	}
}

/// Creates new outbound lane object, backed by runtime storage.
fn outbound_lane<T: Config<I>, I: Instance>(lane_id: LaneId) -> OutboundLane<RuntimeOutboundLaneStorage<T, I>> {
	OutboundLane::new(RuntimeOutboundLaneStorage {
		lane_id,
		_phantom: Default::default(),
	})
}

/// Runtime inbound lane storage.
struct RuntimeInboundLaneStorage<T: Config<I>, I = DefaultInstance> {
	lane_id: LaneId,
	cached_data: RefCell<Option<InboundLaneData<T::InboundRelayer>>>,
	_phantom: PhantomData<I>,
}

impl<T: Config<I>, I: Instance> InboundLaneStorage for RuntimeInboundLaneStorage<T, I> {
	type MessageFee = T::InboundMessageFee;
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

	fn data(&self) -> InboundLaneData<T::InboundRelayer> {
		match self.cached_data.clone().into_inner() {
			Some(data) => data,
			None => {
				let data = InboundLanes::<T, I>::get(&self.lane_id);
				*self.cached_data.try_borrow_mut().expect(
					"we're in the single-threaded environment;\
						we have no recursive borrows; qed",
				) = Some(data.clone());
				data
			}
		}
	}

	fn set_data(&mut self, data: InboundLaneData<T::InboundRelayer>) {
		*self.cached_data.try_borrow_mut().expect(
			"we're in the single-threaded environment;\
				we have no recursive borrows; qed",
		) = Some(data.clone());
		InboundLanes::<T, I>::insert(&self.lane_id, data)
	}
}

/// Runtime outbound lane storage.
struct RuntimeOutboundLaneStorage<T, I = DefaultInstance> {
	lane_id: LaneId,
	_phantom: PhantomData<(T, I)>,
}

impl<T: Config<I>, I: Instance> OutboundLaneStorage for RuntimeOutboundLaneStorage<T, I> {
	type MessageFee = T::OutboundMessageFee;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn data(&self) -> OutboundLaneData {
		OutboundLanes::<I>::get(&self.lane_id)
	}

	fn set_data(&mut self, data: OutboundLaneData) {
		OutboundLanes::<I>::insert(&self.lane_id, data)
	}

	#[cfg(test)]
	fn message(&self, nonce: &MessageNonce) -> Option<MessageData<T::OutboundMessageFee>> {
		OutboundMessages::<T, I>::get(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		})
	}

	fn save_message(&mut self, nonce: MessageNonce, mesage_data: MessageData<T::OutboundMessageFee>) {
		OutboundMessages::<T, I>::insert(
			MessageKey {
				lane_id: self.lane_id,
				nonce,
			},
			mesage_data,
		);
	}

	fn remove_message(&mut self, nonce: &MessageNonce) {
		OutboundMessages::<T, I>::remove(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		});
	}
}

/// Verify messages proof and return proved messages with decoded payload.
fn verify_and_decode_messages_proof<Chain: SourceHeaderChain<Fee>, Fee, DispatchPayload: Decode>(
	proof: Chain::MessagesProof,
	messages_count: MessageNonce,
) -> Result<ProvedMessages<DispatchMessage<DispatchPayload, Fee>>, Chain::Error> {
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
	use crate::mock::{
		message, run_test, Origin, TestEvent, TestMessageDeliveryAndDispatchPayment, TestMessagesProof, TestRuntime,
		PAYLOAD_REJECTED_BY_TARGET_CHAIN, REGULAR_PAYLOAD, TEST_LANE_ID, TEST_RELAYER_A, TEST_RELAYER_B,
	};
	use bp_message_lane::UnrewardedRelayersState;
	use frame_support::{assert_noop, assert_ok};
	use frame_system::{EventRecord, Module as System, Phase};
	use hex_literal::hex;
	use sp_runtime::DispatchError;

	fn send_regular_message() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();

		assert_ok!(Module::<TestRuntime>::send_message(
			Origin::signed(1),
			TEST_LANE_ID,
			REGULAR_PAYLOAD,
			REGULAR_PAYLOAD.1,
		));

		// check event with assigned nonce
		assert_eq!(
			System::<TestRuntime>::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: TestEvent::message_lane(RawEvent::MessageAccepted(TEST_LANE_ID, 1)),
				topics: vec![],
			}],
		);

		// check that fee has been withdrawn from submitter
		assert!(TestMessageDeliveryAndDispatchPayment::is_fee_paid(1, REGULAR_PAYLOAD.1));
	}

	fn receive_messages_delivery_proof() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();

		assert_ok!(Module::<TestRuntime>::receive_messages_delivery_proof(
			Origin::signed(1),
			Ok((
				TEST_LANE_ID,
				InboundLaneData {
					last_confirmed_nonce: 1,
					..Default::default()
				},
			)),
			Default::default(),
		));

		assert_eq!(
			System::<TestRuntime>::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: TestEvent::message_lane(RawEvent::MessagesDelivered(TEST_LANE_ID, 1, 1)),
				topics: vec![],
			}],
		);
	}

	#[test]
	fn pallet_owner_may_change_owner() {
		run_test(|| {
			ModuleOwner::<TestRuntime>::put(2);

			assert_ok!(Module::<TestRuntime>::set_owner(Origin::root(), Some(1)));
			assert_noop!(
				Module::<TestRuntime>::halt_operations(Origin::signed(2)),
				DispatchError::BadOrigin,
			);
			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::root()));

			assert_ok!(Module::<TestRuntime>::set_owner(Origin::signed(1), None));
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(2)),
				DispatchError::BadOrigin,
			);
			assert_ok!(Module::<TestRuntime>::resume_operations(Origin::root()));
		});
	}

	#[test]
	fn pallet_may_be_halted_by_root() {
		run_test(|| {
			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::root()));
			assert_ok!(Module::<TestRuntime>::resume_operations(Origin::root()));
		});
	}

	#[test]
	fn pallet_may_be_halted_by_owner() {
		run_test(|| {
			ModuleOwner::<TestRuntime>::put(2);

			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::signed(2)));
			assert_ok!(Module::<TestRuntime>::resume_operations(Origin::signed(2)));

			assert_noop!(
				Module::<TestRuntime>::halt_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);

			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::signed(2)));
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);
		});
	}

	#[test]
	fn pallet_rejects_transactions_if_halted() {
		run_test(|| {
			// send message first to be able to check that delivery_proof fails later
			send_regular_message();

			IsHalted::<DefaultInstance>::put(true);

			assert_noop!(
				Module::<TestRuntime>::send_message(
					Origin::signed(1),
					TEST_LANE_ID,
					REGULAR_PAYLOAD,
					REGULAR_PAYLOAD.1,
				),
				Error::<TestRuntime, DefaultInstance>::Halted,
			);

			assert_noop!(
				Module::<TestRuntime>::receive_messages_proof(
					Origin::signed(1),
					TEST_RELAYER_A,
					Ok(vec![message(2, REGULAR_PAYLOAD)]).into(),
					1,
					REGULAR_PAYLOAD.1,
				),
				Error::<TestRuntime, DefaultInstance>::Halted,
			);

			assert_noop!(
				Module::<TestRuntime>::receive_messages_delivery_proof(
					Origin::signed(1),
					Ok((
						TEST_LANE_ID,
						InboundLaneData {
							last_confirmed_nonce: 1,
							..Default::default()
						},
					)),
					Default::default(),
				),
				Error::<TestRuntime, DefaultInstance>::Halted,
			);
		});
	}

	#[test]
	fn send_message_works() {
		run_test(|| {
			send_regular_message();
		});
	}

	#[test]
	fn chain_verifier_rejects_invalid_message_in_send_message() {
		run_test(|| {
			// messages with this payload are rejected by target chain verifier
			assert_noop!(
				Module::<TestRuntime>::send_message(
					Origin::signed(1),
					TEST_LANE_ID,
					PAYLOAD_REJECTED_BY_TARGET_CHAIN,
					PAYLOAD_REJECTED_BY_TARGET_CHAIN.1
				),
				Error::<TestRuntime, DefaultInstance>::MessageRejectedByChainVerifier,
			);
		});
	}

	#[test]
	fn lane_verifier_rejects_invalid_message_in_send_message() {
		run_test(|| {
			// messages with zero fee are rejected by lane verifier
			assert_noop!(
				Module::<TestRuntime>::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 0),
				Error::<TestRuntime, DefaultInstance>::MessageRejectedByLaneVerifier,
			);
		});
	}

	#[test]
	fn message_send_fails_if_submitter_cant_pay_message_fee() {
		run_test(|| {
			TestMessageDeliveryAndDispatchPayment::reject_payments();
			assert_noop!(
				Module::<TestRuntime>::send_message(
					Origin::signed(1),
					TEST_LANE_ID,
					REGULAR_PAYLOAD,
					REGULAR_PAYLOAD.1
				),
				Error::<TestRuntime, DefaultInstance>::FailedToWithdrawMessageFee,
			);
		});
	}

	#[test]
	fn receive_messages_proof_works() {
		run_test(|| {
			assert_ok!(Module::<TestRuntime>::receive_messages_proof(
				Origin::signed(1),
				TEST_RELAYER_A,
				Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
				1,
				REGULAR_PAYLOAD.1,
			));

			assert_eq!(InboundLanes::<TestRuntime>::get(TEST_LANE_ID).last_delivered_nonce(), 1);
		});
	}

	#[test]
	fn receive_messages_proof_updates_confirmed_message_nonce() {
		run_test(|| {
			// say we have received 10 messages && last confirmed message is 8
			InboundLanes::<TestRuntime, DefaultInstance>::insert(
				TEST_LANE_ID,
				InboundLaneData {
					last_confirmed_nonce: 8,
					relayers: vec![(9, 9, TEST_RELAYER_A), (10, 10, TEST_RELAYER_B)]
						.into_iter()
						.collect(),
				},
			);
			assert_eq!(
				Module::<TestRuntime>::inbound_unrewarded_relayers_state(TEST_LANE_ID),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					messages_in_oldest_entry: 1,
					total_messages: 2,
				},
			);

			// message proof includes outbound lane state with latest confirmed message updated to 9
			let mut message_proof: TestMessagesProof = Ok(vec![message(11, REGULAR_PAYLOAD)]).into();
			message_proof.result.as_mut().unwrap()[0].1.lane_state = Some(OutboundLaneData {
				latest_received_nonce: 9,
				..Default::default()
			});

			assert_ok!(Module::<TestRuntime>::receive_messages_proof(
				Origin::signed(1),
				TEST_RELAYER_A,
				message_proof,
				1,
				REGULAR_PAYLOAD.1,
			));

			assert_eq!(
				InboundLanes::<TestRuntime>::get(TEST_LANE_ID),
				InboundLaneData {
					last_confirmed_nonce: 9,
					relayers: vec![(10, 10, TEST_RELAYER_B), (11, 11, TEST_RELAYER_A)]
						.into_iter()
						.collect(),
				},
			);
			assert_eq!(
				Module::<TestRuntime>::inbound_unrewarded_relayers_state(TEST_LANE_ID),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					messages_in_oldest_entry: 1,
					total_messages: 2,
				},
			);
		});
	}

	#[test]
	fn receive_messages_proof_rejects_invalid_dispatch_weight() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime>::receive_messages_proof(
					Origin::signed(1),
					TEST_RELAYER_A,
					Ok(vec![message(1, REGULAR_PAYLOAD)]).into(),
					1,
					REGULAR_PAYLOAD.1 - 1,
				),
				Error::<TestRuntime, DefaultInstance>::InvalidMessagesDispatchWeight,
			);
		});
	}

	#[test]
	fn receive_messages_proof_rejects_invalid_proof() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime, DefaultInstance>::receive_messages_proof(
					Origin::signed(1),
					TEST_RELAYER_A,
					Err(()).into(),
					1,
					0,
				),
				Error::<TestRuntime, DefaultInstance>::InvalidMessagesProof,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_works() {
		run_test(|| {
			send_regular_message();
			receive_messages_delivery_proof();

			assert_eq!(
				OutboundLanes::<DefaultInstance>::get(&TEST_LANE_ID).latest_received_nonce,
				1,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rewards_relayers() {
		run_test(|| {
			assert_ok!(Module::<TestRuntime>::send_message(
				Origin::signed(1),
				TEST_LANE_ID,
				REGULAR_PAYLOAD,
				1000,
			));
			assert_ok!(Module::<TestRuntime>::send_message(
				Origin::signed(1),
				TEST_LANE_ID,
				REGULAR_PAYLOAD,
				2000,
			));

			// this reports delivery of message 1 => reward is paid to TEST_RELAYER_A
			assert_ok!(Module::<TestRuntime>::receive_messages_delivery_proof(
				Origin::signed(1),
				Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![(1, 1, TEST_RELAYER_A)].into_iter().collect(),
						..Default::default()
					}
				)),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(
				TEST_RELAYER_A,
				1000
			));
			assert!(!TestMessageDeliveryAndDispatchPayment::is_reward_paid(
				TEST_RELAYER_B,
				2000
			));

			// this reports delivery of both message 1 and message 2 => reward is paid only to TEST_RELAYER_B
			assert_ok!(Module::<TestRuntime>::receive_messages_delivery_proof(
				Origin::signed(1),
				Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![(1, 1, TEST_RELAYER_A), (2, 2, TEST_RELAYER_B)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				)),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					total_messages: 2,
					..Default::default()
				},
			));
			assert!(!TestMessageDeliveryAndDispatchPayment::is_reward_paid(
				TEST_RELAYER_A,
				1000
			));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(
				TEST_RELAYER_B,
				2000
			));
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rejects_invalid_proof() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime>::receive_messages_delivery_proof(Origin::signed(1), Err(()), Default::default(),),
				Error::<TestRuntime, DefaultInstance>::InvalidMessagesDeliveryProof,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rejects_proof_if_declared_relayers_state_is_invalid() {
		run_test(|| {
			// when number of relayers entires is invalid
			assert_noop!(
				Module::<TestRuntime>::receive_messages_delivery_proof(
					Origin::signed(1),
					Ok((
						TEST_LANE_ID,
						InboundLaneData {
							relayers: vec![(1, 1, TEST_RELAYER_A), (2, 2, TEST_RELAYER_B)]
								.into_iter()
								.collect(),
							..Default::default()
						}
					)),
					UnrewardedRelayersState {
						unrewarded_relayer_entries: 1,
						total_messages: 2,
						..Default::default()
					},
				),
				Error::<TestRuntime, DefaultInstance>::InvalidUnrewardedRelayersState,
			);

			// when number of messages is invalid
			assert_noop!(
				Module::<TestRuntime>::receive_messages_delivery_proof(
					Origin::signed(1),
					Ok((
						TEST_LANE_ID,
						InboundLaneData {
							relayers: vec![(1, 1, TEST_RELAYER_A), (2, 2, TEST_RELAYER_B)]
								.into_iter()
								.collect(),
							..Default::default()
						}
					)),
					UnrewardedRelayersState {
						unrewarded_relayer_entries: 2,
						total_messages: 1,
						..Default::default()
					},
				),
				Error::<TestRuntime, DefaultInstance>::InvalidUnrewardedRelayersState,
			);
		});
	}

	#[test]
	fn receive_messages_accepts_single_message_with_invalid_payload() {
		run_test(|| {
			let mut invalid_message = message(1, REGULAR_PAYLOAD);
			invalid_message.data.payload = Vec::new();

			assert_ok!(Module::<TestRuntime, DefaultInstance>::receive_messages_proof(
				Origin::signed(1),
				TEST_RELAYER_A,
				Ok(vec![invalid_message]).into(),
				1,
				0, // weight may be zero in this case (all messages are improperly encoded)
			),);

			assert_eq!(
				InboundLanes::<TestRuntime>::get(&TEST_LANE_ID).last_delivered_nonce(),
				1,
			);
		});
	}

	#[test]
	fn receive_messages_accepts_batch_with_message_with_invalid_payload() {
		run_test(|| {
			let mut invalid_message = message(2, REGULAR_PAYLOAD);
			invalid_message.data.payload = Vec::new();

			assert_ok!(Module::<TestRuntime, DefaultInstance>::receive_messages_proof(
				Origin::signed(1),
				TEST_RELAYER_A,
				Ok(vec![
					message(1, REGULAR_PAYLOAD),
					invalid_message,
					message(3, REGULAR_PAYLOAD),
				])
				.into(),
				3,
				REGULAR_PAYLOAD.1 + REGULAR_PAYLOAD.1,
			),);

			assert_eq!(
				InboundLanes::<TestRuntime>::get(&TEST_LANE_ID).last_delivered_nonce(),
				3,
			);
		});
	}

	#[test]
	fn storage_message_key_computed_properly() {
		// If this test fails, then something has been changed in module storage that is breaking all
		// previously crafted messages proofs.
		assert_eq!(
			storage_keys::message_key::<TestRuntime, DefaultInstance>(&*b"test", 42).0,
			hex!("87f1ffe31b52878f09495ca7482df1a48a395e6242c6813b196ca31ed0547ea79446af0e09063bd4a7874aef8a997cec746573742a00000000000000").to_vec(),
		);
	}

	#[test]
	fn outbound_lane_data_key_computed_properly() {
		// If this test fails, then something has been changed in module storage that is breaking all
		// previously crafted outbound lane state proofs.
		assert_eq!(
			storage_keys::outbound_lane_data_key::<DefaultInstance>(&*b"test").0,
			hex!("87f1ffe31b52878f09495ca7482df1a496c246acb9b55077390e3ca723a0ca1f44a8995dd50b6657a037a7839304535b74657374").to_vec(),
		);
	}

	#[test]
	fn inbound_lane_data_key_computed_properly() {
		// If this test fails, then something has been changed in module storage that is breaking all
		// previously crafted inbound lane state proofs.
		assert_eq!(
			storage_keys::inbound_lane_data_key::<TestRuntime, DefaultInstance>(&*b"test").0,
			hex!("87f1ffe31b52878f09495ca7482df1a4e5f83cf83f2127eb47afdc35d6e43fab44a8995dd50b6657a037a7839304535b74657374").to_vec(),
		);
	}
}
