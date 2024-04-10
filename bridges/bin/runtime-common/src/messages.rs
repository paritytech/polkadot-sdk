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

//! Types that allow runtime to act as a source/target endpoint of message lanes.
//!
//! Messages are assumed to be encoded `Call`s of the target chain. Call-dispatch
//! pallet is used to dispatch incoming messages. Message identified by a tuple
//! of to elements - message lane id and message nonce.

use bp_header_chain::{HeaderChain, HeaderChainError};
use bp_messages::{
	source_chain::LaneMessageVerifier,
	target_chain::{DispatchMessage, MessageDispatch, ProvedLaneMessages, ProvedMessages},
	InboundLaneData, LaneId, Message, MessageKey, MessageNonce, MessagePayload, OutboundLaneData,
};
use bp_runtime::{messages::MessageDispatchResult, Chain, ChainId, Size, StorageProofChecker};
use codec::{Decode, DecodeLimit, Encode};
use frame_support::{traits::Get, weights::Weight, RuntimeDebug};
use hash_db::Hasher;
use scale_info::TypeInfo;
use sp_std::{convert::TryFrom, fmt::Debug, marker::PhantomData, vec::Vec};
use sp_trie::StorageProof;
use xcm::latest::prelude::*;

/// Bidirectional message bridge.
pub trait MessageBridge {
	/// Identifier of this chain.
	const THIS_CHAIN_ID: ChainId;
	/// Identifier of the Bridged chain.
	const BRIDGED_CHAIN_ID: ChainId;
	/// Name of the paired messages pallet instance at the Bridged chain.
	///
	/// Should be the name that is used in the `construct_runtime!()` macro.
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str;

	/// This chain in context of message bridge.
	type ThisChain: ThisChainWithMessages;
	/// Bridged chain in context of message bridge.
	type BridgedChain: BridgedChainWithMessages;
	/// Bridged header chain.
	type BridgedHeaderChain: HeaderChain<UnderlyingChainOf<Self::BridgedChain>>;
}

/// A trait that provides the type of the underlying chain.
pub trait UnderlyingChainProvider {
	/// Underlying chain type.
	type Chain: Chain;
}

/// This chain that has `pallet-bridge-messages` module.
pub trait ThisChainWithMessages: UnderlyingChainProvider {
	/// Call origin on the chain.
	type RuntimeOrigin;
	/// Call type on the chain.
	type RuntimeCall: Encode + Decode;

	/// Do we accept message sent by given origin to given lane?
	fn is_message_accepted(origin: &Self::RuntimeOrigin, lane: &LaneId) -> bool;

	/// Maximal number of pending (not yet delivered) messages at This chain.
	///
	/// Any messages over this limit, will be rejected.
	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce;
}

/// Bridged chain that has `pallet-bridge-messages` module.
pub trait BridgedChainWithMessages: UnderlyingChainProvider {
	/// Returns `true` if message dispatch weight is withing expected limits. `false` means
	/// that the message is too heavy to be sent over the bridge and shall be rejected.
	fn verify_dispatch_weight(message_payload: &[u8]) -> bool;
}

/// This chain in context of message bridge.
pub type ThisChain<B> = <B as MessageBridge>::ThisChain;
/// Bridged chain in context of message bridge.
pub type BridgedChain<B> = <B as MessageBridge>::BridgedChain;
/// Underlying chain type.
pub type UnderlyingChainOf<C> = <C as UnderlyingChainProvider>::Chain;
/// Hash used on the chain.
pub type HashOf<C> = bp_runtime::HashOf<<C as UnderlyingChainProvider>::Chain>;
/// Hasher used on the chain.
pub type HasherOf<C> = bp_runtime::HasherOf<UnderlyingChainOf<C>>;
/// Account id used on the chain.
pub type AccountIdOf<C> = bp_runtime::AccountIdOf<UnderlyingChainOf<C>>;
/// Type of balances that is used on the chain.
pub type BalanceOf<C> = bp_runtime::BalanceOf<UnderlyingChainOf<C>>;
/// Type of origin that is used on the chain.
pub type OriginOf<C> = <C as ThisChainWithMessages>::RuntimeOrigin;
/// Type of call that is used on this chain.
pub type CallOf<C> = <C as ThisChainWithMessages>::RuntimeCall;

/// Raw storage proof type (just raw trie nodes).
pub type RawStorageProof = Vec<Vec<u8>>;

/// Sub-module that is declaring types required for processing This -> Bridged chain messages.
pub mod source {
	use super::*;

	/// Message payload for This -> Bridged chain messages.
	pub type FromThisChainMessagePayload = Vec<u8>;

	/// Maximal size of outbound message payload.
	pub struct FromThisChainMaximalOutboundPayloadSize<B>(PhantomData<B>);

	impl<B: MessageBridge> Get<u32> for FromThisChainMaximalOutboundPayloadSize<B> {
		fn get() -> u32 {
			maximal_message_size::<B>()
		}
	}

	/// Messages delivery proof from bridged chain:
	///
	/// - hash of finalized header;
	/// - storage proof of inbound lane state;
	/// - lane id.
	#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
	pub struct FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash> {
		/// Hash of the bridge header the proof is for.
		pub bridged_header_hash: BridgedHeaderHash,
		/// Storage trie proof generated for [`Self::bridged_header_hash`].
		pub storage_proof: RawStorageProof,
		/// Lane id of which messages were delivered and the proof is for.
		pub lane: LaneId,
	}

	impl<BridgedHeaderHash> Size for FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash> {
		fn size(&self) -> u32 {
			u32::try_from(
				self.storage_proof
					.iter()
					.fold(0usize, |sum, node| sum.saturating_add(node.len())),
			)
			.unwrap_or(u32::MAX)
		}
	}

	/// 'Parsed' message delivery proof - inbound lane id and its state.
	pub type ParsedMessagesDeliveryProofFromBridgedChain<B> =
		(LaneId, InboundLaneData<AccountIdOf<ThisChain<B>>>);

	/// Message verifier that is doing all basic checks.
	///
	/// This verifier assumes following:
	///
	/// - all message lanes are equivalent, so all checks are the same;
	///
	/// Following checks are made:
	///
	/// - message is rejected if its lane is currently blocked;
	/// - message is rejected if there are too many pending (undelivered) messages at the outbound
	///   lane;
	/// - check that the sender has rights to dispatch the call on target chain using provided
	///   dispatch origin;
	/// - check that the sender has paid enough funds for both message delivery and dispatch.
	#[derive(RuntimeDebug)]
	pub struct FromThisChainMessageVerifier<B>(PhantomData<B>);

	/// The error message returned from LaneMessageVerifier when outbound lane is disabled.
	pub const MESSAGE_REJECTED_BY_OUTBOUND_LANE: &str =
		"The outbound message lane has rejected the message.";
	/// The error message returned from LaneMessageVerifier when too many pending messages at the
	/// lane.
	pub const TOO_MANY_PENDING_MESSAGES: &str = "Too many pending messages at the lane.";
	/// The error message returned from LaneMessageVerifier when call origin is mismatch.
	pub const BAD_ORIGIN: &str = "Unable to match the source origin to expected target origin.";

	impl<B> LaneMessageVerifier<OriginOf<ThisChain<B>>, FromThisChainMessagePayload>
		for FromThisChainMessageVerifier<B>
	where
		B: MessageBridge,
		// matches requirements from the `frame_system::Config::Origin`
		OriginOf<ThisChain<B>>: Clone
			+ Into<Result<frame_system::RawOrigin<AccountIdOf<ThisChain<B>>>, OriginOf<ThisChain<B>>>>,
		AccountIdOf<ThisChain<B>>: PartialEq + Clone,
	{
		type Error = &'static str;

		fn verify_message(
			submitter: &OriginOf<ThisChain<B>>,
			lane: &LaneId,
			lane_outbound_data: &OutboundLaneData,
			_payload: &FromThisChainMessagePayload,
		) -> Result<(), Self::Error> {
			// reject message if lane is blocked
			if !ThisChain::<B>::is_message_accepted(submitter, lane) {
				return Err(MESSAGE_REJECTED_BY_OUTBOUND_LANE)
			}

			// reject message if there are too many pending messages at this lane
			let max_pending_messages = ThisChain::<B>::maximal_pending_messages_at_outbound_lane();
			let pending_messages = lane_outbound_data
				.latest_generated_nonce
				.saturating_sub(lane_outbound_data.latest_received_nonce);
			if pending_messages > max_pending_messages {
				return Err(TOO_MANY_PENDING_MESSAGES)
			}

			Ok(())
		}
	}

	/// Return maximal message size of This -> Bridged chain message.
	pub fn maximal_message_size<B: MessageBridge>() -> u32 {
		super::target::maximal_incoming_message_size(
			UnderlyingChainOf::<BridgedChain<B>>::max_extrinsic_size(),
		)
	}

	/// Do basic Bridged-chain specific verification of This -> Bridged chain message.
	///
	/// Ok result from this function means that the delivery transaction with this message
	/// may be 'mined' by the target chain. But the lane may have its own checks (e.g. fee
	/// check) that would reject message (see `FromThisChainMessageVerifier`).
	pub fn verify_chain_message<B: MessageBridge>(
		payload: &FromThisChainMessagePayload,
	) -> Result<(), &'static str> {
		if !BridgedChain::<B>::verify_dispatch_weight(payload) {
			return Err("Incorrect message weight declared")
		}

		// The maximal size of extrinsic at Substrate-based chain depends on the
		// `frame_system::Config::MaximumBlockLength` and
		// `frame_system::Config::AvailableBlockRatio` constants. This check is here to be sure that
		// the lane won't stuck because message is too large to fit into delivery transaction.
		//
		// **IMPORTANT NOTE**: the delivery transaction contains storage proof of the message, not
		// the message itself. The proof is always larger than the message. But unless chain state
		// is enormously large, it should be several dozens/hundreds of bytes. The delivery
		// transaction also contains signatures and signed extensions. Because of this, we reserve
		// 1/3 of the the maximal extrinsic weight for this data.
		if payload.len() > maximal_message_size::<B>() as usize {
			return Err("The message is too large to be sent over the lane")
		}

		Ok(())
	}

	/// Verify proof of This -> Bridged chain messages delivery.
	///
	/// This function is used when Bridged chain is directly using GRANDPA finality. For Bridged
	/// parachains, please use the `verify_messages_delivery_proof_from_parachain`.
	pub fn verify_messages_delivery_proof<B: MessageBridge>(
		proof: FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChain<B>>>,
	) -> Result<ParsedMessagesDeliveryProofFromBridgedChain<B>, &'static str> {
		let FromBridgedChainMessagesDeliveryProof { bridged_header_hash, storage_proof, lane } =
			proof;
		B::BridgedHeaderChain::parse_finalized_storage_proof(
			bridged_header_hash,
			StorageProof::new(storage_proof),
			|storage| {
				// Messages delivery proof is just proof of single storage key read => any error
				// is fatal.
				let storage_inbound_lane_data_key =
					bp_messages::storage_keys::inbound_lane_data_key(
						B::BRIDGED_MESSAGES_PALLET_NAME,
						&lane,
					);
				let raw_inbound_lane_data = storage
					.read_value(storage_inbound_lane_data_key.0.as_ref())
					.map_err(|_| "Failed to read inbound lane state from storage proof")?
					.ok_or("Inbound lane state is missing from the messages proof")?;
				let inbound_lane_data = InboundLaneData::decode(&mut &raw_inbound_lane_data[..])
					.map_err(|_| "Failed to decode inbound lane state from the proof")?;

				Ok((lane, inbound_lane_data))
			},
		)
		.map_err(<&'static str>::from)?
	}

	/// XCM bridge.
	pub trait XcmBridge {
		/// Runtime message bridge configuration.
		type MessageBridge: MessageBridge;
		/// Runtime message sender adapter.
		type MessageSender: bp_messages::source_chain::MessagesBridge<
			OriginOf<ThisChain<Self::MessageBridge>>,
			FromThisChainMessagePayload,
		>;

		/// Our location within the Consensus Universe.
		fn universal_location() -> InteriorMultiLocation;
		/// Verify that the adapter is responsible for handling given XCM destination.
		fn verify_destination(dest: &MultiLocation) -> bool;
		/// Build route from this chain to the XCM destination.
		fn build_destination() -> MultiLocation;
		/// Return message lane used to deliver XCM messages.
		fn xcm_lane() -> LaneId;
	}

	/// XCM bridge adapter for `bridge-messages` pallet.
	pub struct XcmBridgeAdapter<T>(PhantomData<T>);

	impl<T: XcmBridge> SendXcm for XcmBridgeAdapter<T>
	where
		BalanceOf<ThisChain<T::MessageBridge>>: Into<Fungibility>,
		OriginOf<ThisChain<T::MessageBridge>>: From<pallet_xcm::Origin>,
	{
		type Ticket = FromThisChainMessagePayload;

		fn validate(
			dest: &mut Option<MultiLocation>,
			msg: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			let d = dest.take().ok_or(SendError::MissingArgument)?;
			if !T::verify_destination(&d) {
				*dest = Some(d);
				return Err(SendError::NotApplicable)
			}

			let route = T::build_destination();
			let msg = (route, msg.take().ok_or(SendError::MissingArgument)?).encode();

			// let's just take fixed (out of thin air) fee per message in our test bridges
			// (this code won't be used in production anyway)
			let fee_assets = MultiAssets::from((Here, 1_000_000_u128));

			Ok((msg, fee_assets))
		}

		fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
			use bp_messages::source_chain::MessagesBridge;

			let lane = T::xcm_lane();
			let msg = ticket;
			let result = T::MessageSender::send_message(
				pallet_xcm::Origin::from(MultiLocation::from(T::universal_location())).into(),
				lane,
				msg,
			);
			result
				.map(|artifacts| {
					let hash = (lane, artifacts.nonce).using_encoded(sp_io::hashing::blake2_256);
					log::debug!(
						target: "runtime::bridge",
						"Sent XCM message {:?}/{} to {:?}: {:?}",
						lane,
						artifacts.nonce,
						T::MessageBridge::BRIDGED_CHAIN_ID,
						hash,
					);
					hash
				})
				.map_err(|e| {
					log::debug!(
						target: "runtime::bridge",
						"Failed to send XCM message over lane {:?} to {:?}: {:?}",
						lane,
						T::MessageBridge::BRIDGED_CHAIN_ID,
						e,
					);
					SendError::Transport("Bridge has rejected the message")
				})
		}
	}
}

/// Sub-module that is declaring types required for processing Bridged -> This chain messages.
pub mod target {
	use super::*;

	/// Decoded Bridged -> This message payload.
	#[derive(RuntimeDebug, PartialEq, Eq)]
	pub struct FromBridgedChainMessagePayload<Call> {
		/// Data that is actually sent over the wire.
		pub xcm: (xcm::v3::MultiLocation, xcm::v3::Xcm<Call>),
		/// Weight of the message, computed by the weigher. Unknown initially.
		pub weight: Option<Weight>,
	}

	impl<Call: Decode> Decode for FromBridgedChainMessagePayload<Call> {
		fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
			let _: codec::Compact<u32> = Decode::decode(input)?;
			type XcmPairType<Call> = (xcm::v3::MultiLocation, xcm::v3::Xcm<Call>);
			Ok(FromBridgedChainMessagePayload {
				xcm: XcmPairType::<Call>::decode_with_depth_limit(
					sp_api::MAX_EXTRINSIC_DEPTH,
					input,
				)?,
				weight: None,
			})
		}
	}

	impl<Call> From<(xcm::v3::MultiLocation, xcm::v3::Xcm<Call>)>
		for FromBridgedChainMessagePayload<Call>
	{
		fn from(xcm: (xcm::v3::MultiLocation, xcm::v3::Xcm<Call>)) -> Self {
			FromBridgedChainMessagePayload { xcm, weight: None }
		}
	}

	/// Messages proof from bridged chain:
	///
	/// - hash of finalized header;
	/// - storage proof of messages and (optionally) outbound lane state;
	/// - lane id;
	/// - nonces (inclusive range) of messages which are included in this proof.
	#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
	pub struct FromBridgedChainMessagesProof<BridgedHeaderHash> {
		/// Hash of the finalized bridged header the proof is for.
		pub bridged_header_hash: BridgedHeaderHash,
		/// A storage trie proof of messages being delivered.
		pub storage_proof: RawStorageProof,
		/// Messages in this proof are sent over this lane.
		pub lane: LaneId,
		/// Nonce of the first message being delivered.
		pub nonces_start: MessageNonce,
		/// Nonce of the last message being delivered.
		pub nonces_end: MessageNonce,
	}

	impl<BridgedHeaderHash> Size for FromBridgedChainMessagesProof<BridgedHeaderHash> {
		fn size(&self) -> u32 {
			u32::try_from(
				self.storage_proof
					.iter()
					.fold(0usize, |sum, node| sum.saturating_add(node.len())),
			)
			.unwrap_or(u32::MAX)
		}
	}

	/// Dispatching Bridged -> This chain messages.
	#[derive(RuntimeDebug, Clone, Copy)]
	pub struct FromBridgedChainMessageDispatch<B, XcmExecutor, XcmWeigher, WeightCredit> {
		_marker: PhantomData<(B, XcmExecutor, XcmWeigher, WeightCredit)>,
	}

	impl<B: MessageBridge, XcmExecutor, XcmWeigher, WeightCredit>
		MessageDispatch<AccountIdOf<ThisChain<B>>>
		for FromBridgedChainMessageDispatch<B, XcmExecutor, XcmWeigher, WeightCredit>
	where
		XcmExecutor: xcm::v3::ExecuteXcm<CallOf<ThisChain<B>>>,
		XcmWeigher: xcm_executor::traits::WeightBounds<CallOf<ThisChain<B>>>,
		WeightCredit: Get<Weight>,
	{
		type DispatchPayload = FromBridgedChainMessagePayload<CallOf<ThisChain<B>>>;
		type DispatchLevelResult = ();

		fn dispatch_weight(
			message: &mut DispatchMessage<Self::DispatchPayload>,
		) -> frame_support::weights::Weight {
			match message.data.payload {
				Ok(ref mut payload) => {
					// I have no idea why this method takes `&mut` reference and there's nothing
					// about that in documentation. Hope it'll only mutate iff error is returned.
					let weight = XcmWeigher::weight(&mut payload.xcm.1);
					let weight = Weight::from_ref_time(weight.unwrap_or_else(|e| {
						log::debug!(
							target: "runtime::bridge-dispatch",
							"Failed to compute dispatch weight of incoming XCM message {:?}/{}: {:?}",
							message.key.lane_id,
							message.key.nonce,
							e,
						);

						// we shall return 0 and then the XCM executor will fail to execute XCM
						// if we'll return something else (e.g. maximal value), the lane may stuck
						0
					}));

					payload.weight = Some(weight);
					weight
				},
				_ => Weight::zero(),
			}
		}

		fn dispatch(
			_relayer_account: &AccountIdOf<ThisChain<B>>,
			message: DispatchMessage<Self::DispatchPayload>,
		) -> MessageDispatchResult<Self::DispatchLevelResult> {
			let message_id = (message.key.lane_id, message.key.nonce);
			let do_dispatch = move || -> sp_std::result::Result<Outcome, codec::Error> {
				let FromBridgedChainMessagePayload { xcm: (location, xcm), weight: weight_limit } =
					message.data.payload?;
				log::trace!(
					target: "runtime::bridge-dispatch",
					"Going to execute message {:?} (weight limit: {:?}): {:?} {:?}",
					message_id,
					weight_limit,
					location,
					xcm,
				);
				let hash = message_id.using_encoded(sp_io::hashing::blake2_256);

				// if this cod will end up in production, this most likely needs to be set to zero
				let weight_credit = WeightCredit::get();

				let xcm_outcome = XcmExecutor::execute_xcm_in_credit(
					location,
					xcm,
					hash,
					weight_limit.unwrap_or_else(Weight::zero).ref_time(),
					weight_credit.ref_time(),
				);
				Ok(xcm_outcome)
			};

			let xcm_outcome = do_dispatch();
			match xcm_outcome {
				Ok(outcome) => {
					log::trace!(
						target: "runtime::bridge-dispatch",
						"Incoming message {:?} dispatched with result: {:?}",
						message_id,
						outcome,
					);
					match outcome.ensure_execution() {
						Ok(_weight) => (),
						Err(e) => {
							log::error!(
								target: "runtime::bridge-dispatch",
								"Incoming message {:?} was not dispatched, error: {:?}",
								message_id,
								e,
							);
						},
					}
				},
				Err(e) => {
					log::error!(
						target: "runtime::bridge-dispatch",
						"Incoming message {:?} was not dispatched, codec error: {:?}",
						message_id,
						e,
					);
				},
			}

			MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result: () }
		}
	}

	/// Return maximal dispatch weight of the message we're able to receive.
	pub fn maximal_incoming_message_dispatch_weight(maximal_extrinsic_weight: Weight) -> Weight {
		maximal_extrinsic_weight / 2
	}

	/// Return maximal message size given maximal extrinsic size.
	pub fn maximal_incoming_message_size(maximal_extrinsic_size: u32) -> u32 {
		maximal_extrinsic_size / 3 * 2
	}

	/// Verify proof of Bridged -> This chain messages.
	///
	/// This function is used when Bridged chain is directly using GRANDPA finality. For Bridged
	/// parachains, please use the `verify_messages_proof_from_parachain`.
	///
	/// The `messages_count` argument verification (sane limits) is supposed to be made
	/// outside of this function. This function only verifies that the proof declares exactly
	/// `messages_count` messages.
	pub fn verify_messages_proof<B: MessageBridge>(
		proof: FromBridgedChainMessagesProof<HashOf<BridgedChain<B>>>,
		messages_count: u32,
	) -> Result<ProvedMessages<Message>, MessageProofError> {
		let FromBridgedChainMessagesProof {
			bridged_header_hash,
			storage_proof,
			lane,
			nonces_start,
			nonces_end,
		} = proof;

		B::BridgedHeaderChain::parse_finalized_storage_proof(
			bridged_header_hash,
			StorageProof::new(storage_proof),
			|storage| {
				let parser =
					StorageProofCheckerAdapter::<_, B> { storage, _dummy: Default::default() };

				// receiving proofs where end < begin is ok (if proof includes outbound lane state)
				let messages_in_the_proof =
					if let Some(nonces_difference) = nonces_end.checked_sub(nonces_start) {
						// let's check that the user (relayer) has passed correct `messages_count`
						// (this bounds maximal capacity of messages vec below)
						let messages_in_the_proof = nonces_difference.saturating_add(1);
						if messages_in_the_proof != MessageNonce::from(messages_count) {
							return Err(MessageProofError::MessagesCountMismatch)
						}

						messages_in_the_proof
					} else {
						0
					};

				// Read messages first. All messages that are claimed to be in the proof must
				// be in the proof. So any error in `read_value`, or even missing value is fatal.
				//
				// Mind that we allow proofs with no messages if outbound lane state is proved.
				let mut messages = Vec::with_capacity(messages_in_the_proof as _);
				for nonce in nonces_start..=nonces_end {
					let message_key = MessageKey { lane_id: lane, nonce };
					let raw_message_data = parser
						.read_raw_message(&message_key)
						.ok_or(MessageProofError::MissingRequiredMessage)?;
					let payload = MessagePayload::decode(&mut &raw_message_data[..])
						.map_err(|_| MessageProofError::FailedToDecodeMessage)?;
					messages.push(Message { key: message_key, payload });
				}

				// Now let's check if proof contains outbound lane state proof. It is optional, so
				// we simply ignore `read_value` errors and missing value.
				let mut proved_lane_messages = ProvedLaneMessages { lane_state: None, messages };
				let raw_outbound_lane_data = parser.read_raw_outbound_lane_data(&lane);
				if let Some(raw_outbound_lane_data) = raw_outbound_lane_data {
					proved_lane_messages.lane_state = Some(
						OutboundLaneData::decode(&mut &raw_outbound_lane_data[..])
							.map_err(|_| MessageProofError::FailedToDecodeOutboundLaneState)?,
					);
				}

				// Now we may actually check if the proof is empty or not.
				if proved_lane_messages.lane_state.is_none() &&
					proved_lane_messages.messages.is_empty()
				{
					return Err(MessageProofError::Empty)
				}

				// We only support single lane messages in this generated_schema
				let mut proved_messages = ProvedMessages::new();
				proved_messages.insert(lane, proved_lane_messages);

				Ok(proved_messages)
			},
		)
		.map_err(MessageProofError::HeaderChain)?
	}

	/// Error that happens during message proof verification.
	#[derive(Debug, PartialEq, Eq)]
	pub enum MessageProofError {
		/// Error returned by the bridged header chain.
		HeaderChain(HeaderChainError),
		/// The message proof is empty.
		Empty,
		/// Declared messages count doesn't match actual value.
		MessagesCountMismatch,
		/// Message is missing from the proof.
		MissingRequiredMessage,
		/// Failed to decode message from the proof.
		FailedToDecodeMessage,
		/// Failed to decode outbound lane data from the proof.
		FailedToDecodeOutboundLaneState,
	}

	impl From<MessageProofError> for &'static str {
		fn from(err: MessageProofError) -> &'static str {
			match err {
				MessageProofError::HeaderChain(err) => err.into(),
				MessageProofError::Empty => "Messages proof is empty",
				MessageProofError::MessagesCountMismatch =>
					"Declared messages count doesn't match actual value",
				MessageProofError::MissingRequiredMessage => "Message is missing from the proof",
				MessageProofError::FailedToDecodeMessage =>
					"Failed to decode message from the proof",
				MessageProofError::FailedToDecodeOutboundLaneState =>
					"Failed to decode outbound lane data from the proof",
			}
		}
	}

	struct StorageProofCheckerAdapter<H: Hasher, B> {
		storage: StorageProofChecker<H>,
		_dummy: sp_std::marker::PhantomData<B>,
	}

	impl<H: Hasher, B: MessageBridge> StorageProofCheckerAdapter<H, B> {
		fn read_raw_outbound_lane_data(&self, lane_id: &LaneId) -> Option<Vec<u8>> {
			let storage_outbound_lane_data_key = bp_messages::storage_keys::outbound_lane_data_key(
				B::BRIDGED_MESSAGES_PALLET_NAME,
				lane_id,
			);
			self.storage.read_value(storage_outbound_lane_data_key.0.as_ref()).ok()?
		}

		fn read_raw_message(&self, message_key: &MessageKey) -> Option<Vec<u8>> {
			let storage_message_key = bp_messages::storage_keys::message_key(
				B::BRIDGED_MESSAGES_PALLET_NAME,
				&message_key.lane_id,
				message_key.nonce,
			);
			self.storage.read_value(storage_message_key.0.as_ref()).ok()?
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::messages_generation::{
		encode_all_messages, encode_lane_data, prepare_messages_storage_proof,
	};
	use bp_runtime::HeaderOf;
	use codec::{Decode, Encode};
	use frame_support::weights::Weight;
	use sp_core::H256;
	use sp_runtime::traits::{BlakeTwo256, Header as _};
	use std::cell::RefCell;

	const BRIDGED_CHAIN_MIN_EXTRINSIC_WEIGHT: usize = 5;
	const BRIDGED_CHAIN_MAX_EXTRINSIC_WEIGHT: usize = 2048;
	const BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE: u32 = 1024;

	/// Bridge that is deployed on ThisChain and allows sending/receiving messages to/from
	/// BridgedChain.
	#[derive(Debug, PartialEq, Eq)]
	struct OnThisChainBridge;

	impl MessageBridge for OnThisChainBridge {
		const THIS_CHAIN_ID: ChainId = *b"this";
		const BRIDGED_CHAIN_ID: ChainId = *b"brdg";
		const BRIDGED_MESSAGES_PALLET_NAME: &'static str = "";

		type ThisChain = ThisChain;
		type BridgedChain = BridgedChain;
		type BridgedHeaderChain = BridgedHeaderChain;
	}

	/// Bridge that is deployed on BridgedChain and allows sending/receiving messages to/from
	/// ThisChain;
	#[derive(Debug, PartialEq, Eq)]
	struct OnBridgedChainBridge;

	impl MessageBridge for OnBridgedChainBridge {
		const THIS_CHAIN_ID: ChainId = *b"brdg";
		const BRIDGED_CHAIN_ID: ChainId = *b"this";
		const BRIDGED_MESSAGES_PALLET_NAME: &'static str = "";

		type ThisChain = BridgedChain;
		type BridgedChain = ThisChain;
		type BridgedHeaderChain = ThisHeaderChain;
	}

	#[derive(Clone, Debug)]
	struct ThisChainOrigin(Result<frame_system::RawOrigin<ThisChainAccountId>, ()>);

	impl From<ThisChainOrigin>
		for Result<frame_system::RawOrigin<ThisChainAccountId>, ThisChainOrigin>
	{
		fn from(
			origin: ThisChainOrigin,
		) -> Result<frame_system::RawOrigin<ThisChainAccountId>, ThisChainOrigin> {
			origin.clone().0.map_err(|_| origin)
		}
	}

	#[derive(Clone, Debug)]
	struct BridgedChainOrigin;

	impl From<BridgedChainOrigin>
		for Result<frame_system::RawOrigin<BridgedChainAccountId>, BridgedChainOrigin>
	{
		fn from(
			_origin: BridgedChainOrigin,
		) -> Result<frame_system::RawOrigin<BridgedChainAccountId>, BridgedChainOrigin> {
			unreachable!()
		}
	}

	struct ThisUnderlyingChain;
	type ThisChainHeader = sp_runtime::generic::Header<u64, BlakeTwo256>;
	type ThisChainAccountId = u32;
	type ThisChainBalance = u32;
	#[derive(Decode, Encode)]
	struct ThisChainCall;

	impl Chain for ThisUnderlyingChain {
		type BlockNumber = u64;
		type Hash = H256;
		type Hasher = BlakeTwo256;
		type Header = ThisChainHeader;
		type AccountId = ThisChainAccountId;
		type Balance = ThisChainBalance;
		type Index = u32;
		type Signature = sp_runtime::MultiSignature;

		fn max_extrinsic_size() -> u32 {
			BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE
		}

		fn max_extrinsic_weight() -> Weight {
			Weight::zero()
		}
	}

	struct ThisChain;

	impl UnderlyingChainProvider for ThisChain {
		type Chain = ThisUnderlyingChain;
	}

	impl ThisChainWithMessages for ThisChain {
		type RuntimeOrigin = ThisChainOrigin;
		type RuntimeCall = ThisChainCall;

		fn is_message_accepted(_send_origin: &Self::RuntimeOrigin, lane: &LaneId) -> bool {
			lane == TEST_LANE_ID
		}

		fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
			MAXIMAL_PENDING_MESSAGES_AT_TEST_LANE
		}
	}

	impl BridgedChainWithMessages for ThisChain {
		fn verify_dispatch_weight(_message_payload: &[u8]) -> bool {
			unreachable!()
		}
	}

	struct BridgedUnderlyingChain;
	type BridgedChainHeader = sp_runtime::generic::Header<u64, BlakeTwo256>;
	type BridgedChainAccountId = u128;
	type BridgedChainBalance = u128;
	#[derive(Decode, Encode)]
	struct BridgedChainCall;

	impl Chain for BridgedUnderlyingChain {
		type BlockNumber = u64;
		type Hash = H256;
		type Hasher = BlakeTwo256;
		type Header = BridgedChainHeader;
		type AccountId = BridgedChainAccountId;
		type Balance = BridgedChainBalance;
		type Index = u32;
		type Signature = sp_runtime::MultiSignature;

		fn max_extrinsic_size() -> u32 {
			BRIDGED_CHAIN_MAX_EXTRINSIC_SIZE
		}
		fn max_extrinsic_weight() -> Weight {
			Weight::zero()
		}
	}

	struct BridgedChain;

	impl UnderlyingChainProvider for BridgedChain {
		type Chain = BridgedUnderlyingChain;
	}

	impl ThisChainWithMessages for BridgedChain {
		type RuntimeOrigin = BridgedChainOrigin;
		type RuntimeCall = BridgedChainCall;

		fn is_message_accepted(_send_origin: &Self::RuntimeOrigin, _lane: &LaneId) -> bool {
			unreachable!()
		}

		fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
			unreachable!()
		}
	}

	impl BridgedChainWithMessages for BridgedChain {
		fn verify_dispatch_weight(message_payload: &[u8]) -> bool {
			message_payload.len() >= BRIDGED_CHAIN_MIN_EXTRINSIC_WEIGHT &&
				message_payload.len() <= BRIDGED_CHAIN_MAX_EXTRINSIC_WEIGHT
		}
	}

	thread_local! {
		static TEST_BRIDGED_HEADER: RefCell<Option<BridgedChainHeader>> = RefCell::new(None);
	}

	struct BridgedHeaderChain;

	impl HeaderChain<BridgedUnderlyingChain> for BridgedHeaderChain {
		fn finalized_header(
			_hash: HashOf<BridgedChain>,
		) -> Option<HeaderOf<BridgedUnderlyingChain>> {
			TEST_BRIDGED_HEADER.with(|h| h.borrow().clone())
		}
	}

	struct ThisHeaderChain;

	impl HeaderChain<ThisUnderlyingChain> for ThisHeaderChain {
		fn finalized_header(_hash: HashOf<ThisChain>) -> Option<HeaderOf<ThisUnderlyingChain>> {
			unreachable!()
		}
	}

	fn test_lane_outbound_data() -> OutboundLaneData {
		OutboundLaneData::default()
	}

	const TEST_LANE_ID: &LaneId = b"test";
	const MAXIMAL_PENDING_MESSAGES_AT_TEST_LANE: MessageNonce = 32;

	fn regular_outbound_message_payload() -> source::FromThisChainMessagePayload {
		vec![42]
	}

	#[test]
	fn message_is_rejected_when_sent_using_disabled_lane() {
		assert_eq!(
			source::FromThisChainMessageVerifier::<OnThisChainBridge>::verify_message(
				&ThisChainOrigin(Ok(frame_system::RawOrigin::Root)),
				b"dsbl",
				&test_lane_outbound_data(),
				&regular_outbound_message_payload(),
			),
			Err(source::MESSAGE_REJECTED_BY_OUTBOUND_LANE)
		);
	}

	#[test]
	fn message_is_rejected_when_there_are_too_many_pending_messages_at_outbound_lane() {
		assert_eq!(
			source::FromThisChainMessageVerifier::<OnThisChainBridge>::verify_message(
				&ThisChainOrigin(Ok(frame_system::RawOrigin::Root)),
				TEST_LANE_ID,
				&OutboundLaneData {
					latest_received_nonce: 100,
					latest_generated_nonce: 100 + MAXIMAL_PENDING_MESSAGES_AT_TEST_LANE + 1,
					..Default::default()
				},
				&regular_outbound_message_payload(),
			),
			Err(source::TOO_MANY_PENDING_MESSAGES)
		);
	}

	#[test]
	fn verify_chain_message_rejects_message_with_too_small_declared_weight() {
		assert!(source::verify_chain_message::<OnThisChainBridge>(&vec![
			42;
			BRIDGED_CHAIN_MIN_EXTRINSIC_WEIGHT -
				1
		])
		.is_err());
	}

	#[test]
	fn verify_chain_message_rejects_message_with_too_large_declared_weight() {
		assert!(source::verify_chain_message::<OnThisChainBridge>(&vec![
			42;
			BRIDGED_CHAIN_MAX_EXTRINSIC_WEIGHT -
				1
		])
		.is_err());
	}

	#[test]
	fn verify_chain_message_rejects_message_too_large_message() {
		assert!(source::verify_chain_message::<OnThisChainBridge>(&vec![
			0;
			source::maximal_message_size::<OnThisChainBridge>()
				as usize + 1
		],)
		.is_err());
	}

	#[test]
	fn verify_chain_message_accepts_maximal_message() {
		assert_eq!(
			source::verify_chain_message::<OnThisChainBridge>(&vec![
				0;
				source::maximal_message_size::<OnThisChainBridge>()
					as _
			],),
			Ok(()),
		);
	}

	fn using_messages_proof<R>(
		nonces_end: MessageNonce,
		outbound_lane_data: Option<OutboundLaneData>,
		encode_message: impl Fn(MessageNonce, &MessagePayload) -> Option<Vec<u8>>,
		encode_outbound_lane_data: impl Fn(&OutboundLaneData) -> Vec<u8>,
		test: impl Fn(target::FromBridgedChainMessagesProof<H256>) -> R,
	) -> R {
		let (state_root, storage_proof) = prepare_messages_storage_proof::<OnThisChainBridge>(
			*TEST_LANE_ID,
			1..=nonces_end,
			outbound_lane_data,
			bp_runtime::StorageProofSize::Minimal(0),
			vec![42],
			encode_message,
			encode_outbound_lane_data,
		);

		TEST_BRIDGED_HEADER.with(|h| {
			*h.borrow_mut() = Some(BridgedChainHeader::new(
				0,
				Default::default(),
				state_root,
				Default::default(),
				Default::default(),
			))
		});

		test(target::FromBridgedChainMessagesProof {
			bridged_header_hash: Default::default(),
			storage_proof,
			lane: *TEST_LANE_ID,
			nonces_start: 1,
			nonces_end,
		})
	}

	#[test]
	fn messages_proof_is_rejected_if_declared_less_than_actual_number_of_messages() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				target::verify_messages_proof::<OnThisChainBridge>(proof, 5)
			}),
			Err(target::MessageProofError::MessagesCountMismatch),
		);
	}

	#[test]
	fn messages_proof_is_rejected_if_declared_more_than_actual_number_of_messages() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				target::verify_messages_proof::<OnThisChainBridge>(proof, 15)
			}),
			Err(target::MessageProofError::MessagesCountMismatch),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_header_is_missing_from_the_chain() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				TEST_BRIDGED_HEADER.with(|h| *h.borrow_mut() = None);
				target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			}),
			Err(target::MessageProofError::HeaderChain(HeaderChainError::UnknownHeader)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_header_state_root_mismatches() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				TEST_BRIDGED_HEADER
					.with(|h| h.borrow_mut().as_mut().unwrap().state_root = Default::default());
				target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			}),
			Err(target::MessageProofError::HeaderChain(HeaderChainError::StorageRootMismatch)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_required_message_is_missing() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				|n, m| if n != 5 { Some(m.encode()) } else { None },
				encode_lane_data,
				|proof| target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			),
			Err(target::MessageProofError::MissingRequiredMessage),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_message_decode_fails() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				|n, m| {
					let mut m = m.encode();
					if n == 5 {
						m = vec![42]
					}
					Some(m)
				},
				encode_lane_data,
				|proof| target::verify_messages_proof::<OnThisChainBridge>(proof, 10),
			),
			Err(target::MessageProofError::FailedToDecodeMessage),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_outbound_lane_state_decode_fails() {
		assert_eq!(
			using_messages_proof(
				10,
				Some(OutboundLaneData {
					oldest_unpruned_nonce: 1,
					latest_received_nonce: 1,
					latest_generated_nonce: 1,
				}),
				encode_all_messages,
				|d| {
					let mut d = d.encode();
					d.truncate(1);
					d
				},
				|proof| target::verify_messages_proof::<OnThisChainBridge>(proof, 10),
			),
			Err(target::MessageProofError::FailedToDecodeOutboundLaneState),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_is_empty() {
		assert_eq!(
			using_messages_proof(0, None, encode_all_messages, encode_lane_data, |proof| {
				target::verify_messages_proof::<OnThisChainBridge>(proof, 0)
			},),
			Err(target::MessageProofError::Empty),
		);
	}

	#[test]
	fn non_empty_message_proof_without_messages_is_accepted() {
		assert_eq!(
			using_messages_proof(
				0,
				Some(OutboundLaneData {
					oldest_unpruned_nonce: 1,
					latest_received_nonce: 1,
					latest_generated_nonce: 1,
				}),
				encode_all_messages,
				encode_lane_data,
				|proof| target::verify_messages_proof::<OnThisChainBridge>(proof, 0),
			),
			Ok(vec![(
				*TEST_LANE_ID,
				ProvedLaneMessages {
					lane_state: Some(OutboundLaneData {
						oldest_unpruned_nonce: 1,
						latest_received_nonce: 1,
						latest_generated_nonce: 1,
					}),
					messages: Vec::new(),
				},
			)]
			.into_iter()
			.collect()),
		);
	}

	#[test]
	fn non_empty_message_proof_is_accepted() {
		assert_eq!(
			using_messages_proof(
				1,
				Some(OutboundLaneData {
					oldest_unpruned_nonce: 1,
					latest_received_nonce: 1,
					latest_generated_nonce: 1,
				}),
				encode_all_messages,
				encode_lane_data,
				|proof| target::verify_messages_proof::<OnThisChainBridge>(proof, 1),
			),
			Ok(vec![(
				*TEST_LANE_ID,
				ProvedLaneMessages {
					lane_state: Some(OutboundLaneData {
						oldest_unpruned_nonce: 1,
						latest_received_nonce: 1,
						latest_generated_nonce: 1,
					}),
					messages: vec![Message {
						key: MessageKey { lane_id: *TEST_LANE_ID, nonce: 1 },
						payload: vec![42],
					}],
				},
			)]
			.into_iter()
			.collect()),
		);
	}

	#[test]
	fn verify_messages_proof_does_not_panic_if_messages_count_mismatches() {
		assert_eq!(
			using_messages_proof(1, None, encode_all_messages, encode_lane_data, |mut proof| {
				proof.nonces_end = u64::MAX;
				target::verify_messages_proof::<OnThisChainBridge>(proof, u32::MAX)
			},),
			Err(target::MessageProofError::MessagesCountMismatch),
		);
	}
}
