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

//! Types that allow runtime to act as a source/target endpoint of message lanes.
//!
//! Messages are assumed to be encoded `Call`s of the target chain. Call-dispatch
//! pallet is used to dispatch incoming messages. Message identified by a tuple
//! of to elements - message lane id and message nonce.

pub use bp_runtime::{RangeInclusiveExt, UnderlyingChainOf, UnderlyingChainProvider};

use bp_header_chain::HeaderChain;
use bp_messages::{
	source_chain::TargetHeaderChain,
	target_chain::{ProvedLaneMessages, ProvedMessages, SourceHeaderChain},
	InboundLaneData, LaneId, Message, MessageKey, MessageNonce, MessagePayload, OutboundLaneData,
	VerificationError,
};
use bp_runtime::{Chain, RawStorageProof, Size, StorageProofChecker};
use codec::{Decode, Encode};
use frame_support::{traits::Get, weights::Weight};
use hash_db::Hasher;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{convert::TryFrom, marker::PhantomData, vec::Vec};

/// Bidirectional message bridge.
pub trait MessageBridge {
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

/// This chain that has `pallet-bridge-messages` module.
pub trait ThisChainWithMessages: UnderlyingChainProvider {
	/// Call origin on the chain.
	type RuntimeOrigin;
}

/// Bridged chain that has `pallet-bridge-messages` module.
pub trait BridgedChainWithMessages: UnderlyingChainProvider {}

/// This chain in context of message bridge.
pub type ThisChain<B> = <B as MessageBridge>::ThisChain;
/// Bridged chain in context of message bridge.
pub type BridgedChain<B> = <B as MessageBridge>::BridgedChain;
/// Hash used on the chain.
pub type HashOf<C> = bp_runtime::HashOf<<C as UnderlyingChainProvider>::Chain>;
/// Hasher used on the chain.
pub type HasherOf<C> = bp_runtime::HasherOf<UnderlyingChainOf<C>>;
/// Account id used on the chain.
pub type AccountIdOf<C> = bp_runtime::AccountIdOf<UnderlyingChainOf<C>>;
/// Type of balances that is used on the chain.
pub type BalanceOf<C> = bp_runtime::BalanceOf<UnderlyingChainOf<C>>;

/// Sub-module that is declaring types required for processing This -> Bridged chain messages.
pub mod source {
	use super::*;

	/// Message payload for This -> Bridged chain messages.
	pub type FromThisChainMessagePayload = crate::messages_xcm_extension::XcmAsPlainPayload;

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

	/// Return maximal message size of This -> Bridged chain message.
	pub fn maximal_message_size<B: MessageBridge>() -> u32 {
		super::target::maximal_incoming_message_size(
			UnderlyingChainOf::<BridgedChain<B>>::max_extrinsic_size(),
		)
	}

	/// `TargetHeaderChain` implementation that is using default types and perform default checks.
	pub struct TargetHeaderChainAdapter<B>(PhantomData<B>);

	impl<B: MessageBridge> TargetHeaderChain<FromThisChainMessagePayload, AccountIdOf<ThisChain<B>>>
		for TargetHeaderChainAdapter<B>
	{
		type MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChain<B>>>;

		fn verify_message(payload: &FromThisChainMessagePayload) -> Result<(), VerificationError> {
			verify_chain_message::<B>(payload)
		}

		fn verify_messages_delivery_proof(
			proof: Self::MessagesDeliveryProof,
		) -> Result<(LaneId, InboundLaneData<AccountIdOf<ThisChain<B>>>), VerificationError> {
			verify_messages_delivery_proof::<B>(proof)
		}
	}

	/// Do basic Bridged-chain specific verification of This -> Bridged chain message.
	///
	/// Ok result from this function means that the delivery transaction with this message
	/// may be 'mined' by the target chain.
	pub fn verify_chain_message<B: MessageBridge>(
		payload: &FromThisChainMessagePayload,
	) -> Result<(), VerificationError> {
		// IMPORTANT: any error that is returned here is fatal for the bridge, because
		// this code is executed at the bridge hub and message sender actually lives
		// at some sibling parachain. So we are failing **after** the message has been
		// sent and we can't report it back to sender (unless error report mechanism is
		// embedded into message and its dispatcher).

		// apart from maximal message size check (see below), we should also check the message
		// dispatch weight here. But we assume that the bridged chain will just push the message
		// to some queue (XCMP, UMP, DMP), so the weight is constant and fits the block.

		// The maximal size of extrinsic at Substrate-based chain depends on the
		// `frame_system::Config::MaximumBlockLength` and
		// `frame_system::Config::AvailableBlockRatio` constants. This check is here to be sure that
		// the lane won't stuck because message is too large to fit into delivery transaction.
		//
		// **IMPORTANT NOTE**: the delivery transaction contains storage proof of the message, not
		// the message itself. The proof is always larger than the message. But unless chain state
		// is enormously large, it should be several dozens/hundreds of bytes. The delivery
		// transaction also contains signatures and signed extensions. Because of this, we reserve
		// 1/3 of the the maximal extrinsic size for this data.
		if payload.len() > maximal_message_size::<B>() as usize {
			return Err(VerificationError::MessageTooLarge)
		}

		Ok(())
	}

	/// Verify proof of This -> Bridged chain messages delivery.
	///
	/// This function is used when Bridged chain is directly using GRANDPA finality. For Bridged
	/// parachains, please use the `verify_messages_delivery_proof_from_parachain`.
	pub fn verify_messages_delivery_proof<B: MessageBridge>(
		proof: FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChain<B>>>,
	) -> Result<ParsedMessagesDeliveryProofFromBridgedChain<B>, VerificationError> {
		let FromBridgedChainMessagesDeliveryProof { bridged_header_hash, storage_proof, lane } =
			proof;
		let mut storage =
			B::BridgedHeaderChain::storage_proof_checker(bridged_header_hash, storage_proof)
				.map_err(VerificationError::HeaderChain)?;
		// Messages delivery proof is just proof of single storage key read => any error
		// is fatal.
		let storage_inbound_lane_data_key = bp_messages::storage_keys::inbound_lane_data_key(
			B::BRIDGED_MESSAGES_PALLET_NAME,
			&lane,
		);
		let inbound_lane_data = storage
			.read_and_decode_mandatory_value(storage_inbound_lane_data_key.0.as_ref())
			.map_err(VerificationError::InboundLaneStorage)?;

		// check that the storage proof doesn't have any untouched trie nodes
		storage.ensure_no_unused_nodes().map_err(VerificationError::StorageProof)?;

		Ok((lane, inbound_lane_data))
	}
}

/// Sub-module that is declaring types required for processing Bridged -> This chain messages.
pub mod target {
	use super::*;

	/// Decoded Bridged -> This message payload.
	pub type FromBridgedChainMessagePayload = crate::messages_xcm_extension::XcmAsPlainPayload;

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

	/// Return maximal dispatch weight of the message we're able to receive.
	pub fn maximal_incoming_message_dispatch_weight(maximal_extrinsic_weight: Weight) -> Weight {
		maximal_extrinsic_weight / 2
	}

	/// Return maximal message size given maximal extrinsic size.
	pub fn maximal_incoming_message_size(maximal_extrinsic_size: u32) -> u32 {
		maximal_extrinsic_size / 3 * 2
	}

	/// `SourceHeaderChain` implementation that is using default types and perform default checks.
	pub struct SourceHeaderChainAdapter<B>(PhantomData<B>);

	impl<B: MessageBridge> SourceHeaderChain for SourceHeaderChainAdapter<B> {
		type MessagesProof = FromBridgedChainMessagesProof<HashOf<BridgedChain<B>>>;

		fn verify_messages_proof(
			proof: Self::MessagesProof,
			messages_count: u32,
		) -> Result<ProvedMessages<Message>, VerificationError> {
			verify_messages_proof::<B>(proof, messages_count)
		}
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
	) -> Result<ProvedMessages<Message>, VerificationError> {
		let FromBridgedChainMessagesProof {
			bridged_header_hash,
			storage_proof,
			lane,
			nonces_start,
			nonces_end,
		} = proof;
		let storage =
			B::BridgedHeaderChain::storage_proof_checker(bridged_header_hash, storage_proof)
				.map_err(VerificationError::HeaderChain)?;
		let mut parser = StorageProofCheckerAdapter::<_, B> { storage, _dummy: Default::default() };
		let nonces_range = nonces_start..=nonces_end;

		// receiving proofs where end < begin is ok (if proof includes outbound lane state)
		let messages_in_the_proof = nonces_range.checked_len().unwrap_or(0);
		if messages_in_the_proof != MessageNonce::from(messages_count) {
			return Err(VerificationError::MessagesCountMismatch)
		}

		// Read messages first. All messages that are claimed to be in the proof must
		// be in the proof. So any error in `read_value`, or even missing value is fatal.
		//
		// Mind that we allow proofs with no messages if outbound lane state is proved.
		let mut messages = Vec::with_capacity(messages_in_the_proof as _);
		for nonce in nonces_range {
			let message_key = MessageKey { lane_id: lane, nonce };
			let message_payload = parser.read_and_decode_message_payload(&message_key)?;
			messages.push(Message { key: message_key, payload: message_payload });
		}

		// Now let's check if proof contains outbound lane state proof. It is optional, so
		// we simply ignore `read_value` errors and missing value.
		let proved_lane_messages = ProvedLaneMessages {
			lane_state: parser.read_and_decode_outbound_lane_data(&lane)?,
			messages,
		};

		// Now we may actually check if the proof is empty or not.
		if proved_lane_messages.lane_state.is_none() && proved_lane_messages.messages.is_empty() {
			return Err(VerificationError::EmptyMessageProof)
		}

		// check that the storage proof doesn't have any untouched trie nodes
		parser
			.storage
			.ensure_no_unused_nodes()
			.map_err(VerificationError::StorageProof)?;

		// We only support single lane messages in this generated_schema
		let mut proved_messages = ProvedMessages::new();
		proved_messages.insert(lane, proved_lane_messages);

		Ok(proved_messages)
	}

	struct StorageProofCheckerAdapter<H: Hasher, B> {
		storage: StorageProofChecker<H>,
		_dummy: sp_std::marker::PhantomData<B>,
	}

	impl<H: Hasher, B: MessageBridge> StorageProofCheckerAdapter<H, B> {
		fn read_and_decode_outbound_lane_data(
			&mut self,
			lane_id: &LaneId,
		) -> Result<Option<OutboundLaneData>, VerificationError> {
			let storage_outbound_lane_data_key = bp_messages::storage_keys::outbound_lane_data_key(
				B::BRIDGED_MESSAGES_PALLET_NAME,
				lane_id,
			);

			self.storage
				.read_and_decode_opt_value(storage_outbound_lane_data_key.0.as_ref())
				.map_err(VerificationError::OutboundLaneStorage)
		}

		fn read_and_decode_message_payload(
			&mut self,
			message_key: &MessageKey,
		) -> Result<MessagePayload, VerificationError> {
			let storage_message_key = bp_messages::storage_keys::message_key(
				B::BRIDGED_MESSAGES_PALLET_NAME,
				&message_key.lane_id,
				message_key.nonce,
			);
			self.storage
				.read_and_decode_mandatory_value(storage_message_key.0.as_ref())
				.map_err(VerificationError::MessageStorage)
		}
	}
}

/// The `BridgeMessagesCall` used by a chain.
pub type BridgeMessagesCallOf<C> = bp_messages::BridgeMessagesCall<
	bp_runtime::AccountIdOf<C>,
	target::FromBridgedChainMessagesProof<bp_runtime::HashOf<C>>,
	source::FromBridgedChainMessagesDeliveryProof<bp_runtime::HashOf<C>>,
>;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		messages_generation::{
			encode_all_messages, encode_lane_data, prepare_messages_storage_proof,
		},
		mock::*,
	};
	use bp_header_chain::{HeaderChainError, StoredHeaderDataBuilder};
	use bp_runtime::{HeaderId, StorageProofError};
	use codec::Encode;
	use sp_core::H256;
	use sp_runtime::traits::Header as _;

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
			TEST_LANE_ID,
			1..=nonces_end,
			outbound_lane_data,
			bp_runtime::StorageProofSize::Minimal(0),
			vec![42],
			encode_message,
			encode_outbound_lane_data,
		);

		sp_io::TestExternalities::new(Default::default()).execute_with(move || {
			let bridged_header = BridgedChainHeader::new(
				0,
				Default::default(),
				state_root,
				Default::default(),
				Default::default(),
			);
			let bridged_header_hash = bridged_header.hash();

			pallet_bridge_grandpa::BestFinalized::<TestRuntime>::put(HeaderId(
				0,
				bridged_header_hash,
			));
			pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::insert(
				bridged_header_hash,
				bridged_header.build(),
			);
			test(target::FromBridgedChainMessagesProof {
				bridged_header_hash,
				storage_proof,
				lane: TEST_LANE_ID,
				nonces_start: 1,
				nonces_end,
			})
		})
	}

	#[test]
	fn messages_proof_is_rejected_if_declared_less_than_actual_number_of_messages() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				target::verify_messages_proof::<OnThisChainBridge>(proof, 5)
			}),
			Err(VerificationError::MessagesCountMismatch),
		);
	}

	#[test]
	fn messages_proof_is_rejected_if_declared_more_than_actual_number_of_messages() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				target::verify_messages_proof::<OnThisChainBridge>(proof, 15)
			}),
			Err(VerificationError::MessagesCountMismatch),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_header_is_missing_from_the_chain() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				let bridged_header_hash =
					pallet_bridge_grandpa::BestFinalized::<TestRuntime>::get().unwrap().1;
				pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::remove(bridged_header_hash);
				target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			}),
			Err(VerificationError::HeaderChain(HeaderChainError::UnknownHeader)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_header_state_root_mismatches() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |proof| {
				let bridged_header_hash =
					pallet_bridge_grandpa::BestFinalized::<TestRuntime>::get().unwrap().1;
				pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::insert(
					bridged_header_hash,
					BridgedChainHeader::new(
						0,
						Default::default(),
						Default::default(),
						Default::default(),
						Default::default(),
					)
					.build(),
				);
				target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			}),
			Err(VerificationError::HeaderChain(HeaderChainError::StorageProof(
				StorageProofError::StorageRootMismatch
			))),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_has_duplicate_trie_nodes() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |mut proof| {
				let node = proof.storage_proof.pop().unwrap();
				proof.storage_proof.push(node.clone());
				proof.storage_proof.push(node);
				target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			},),
			Err(VerificationError::HeaderChain(HeaderChainError::StorageProof(
				StorageProofError::DuplicateNodesInProof
			))),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_has_unused_trie_nodes() {
		assert_eq!(
			using_messages_proof(10, None, encode_all_messages, encode_lane_data, |mut proof| {
				proof.storage_proof.push(vec![42]);
				target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			},),
			Err(VerificationError::StorageProof(StorageProofError::UnusedNodesInTheProof)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_required_message_is_missing() {
		matches!(
			using_messages_proof(
				10,
				None,
				|n, m| if n != 5 { Some(m.encode()) } else { None },
				encode_lane_data,
				|proof| target::verify_messages_proof::<OnThisChainBridge>(proof, 10)
			),
			Err(VerificationError::MessageStorage(StorageProofError::StorageValueEmpty)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_message_decode_fails() {
		matches!(
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
			Err(VerificationError::MessageStorage(StorageProofError::StorageValueDecodeFailed(_))),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_outbound_lane_state_decode_fails() {
		matches!(
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
			Err(VerificationError::OutboundLaneStorage(
				StorageProofError::StorageValueDecodeFailed(_)
			)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_is_empty() {
		assert_eq!(
			using_messages_proof(0, None, encode_all_messages, encode_lane_data, |proof| {
				target::verify_messages_proof::<OnThisChainBridge>(proof, 0)
			},),
			Err(VerificationError::EmptyMessageProof),
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
				TEST_LANE_ID,
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
				TEST_LANE_ID,
				ProvedLaneMessages {
					lane_state: Some(OutboundLaneData {
						oldest_unpruned_nonce: 1,
						latest_received_nonce: 1,
						latest_generated_nonce: 1,
					}),
					messages: vec![Message {
						key: MessageKey { lane_id: TEST_LANE_ID, nonce: 1 },
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
			Err(VerificationError::MessagesCountMismatch),
		);
	}
}
