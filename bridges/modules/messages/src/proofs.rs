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

//! Tools for messages and delivery proof verification.

use crate::{BridgedChainOf, BridgedHeaderChainOf, Config};

use bp_header_chain::HeaderChain;
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::{FromBridgedChainMessagesProof, ProvedLaneMessages, ProvedMessages},
	ChainWithMessages, InboundLaneData, LaneId, Message, MessageKey, MessageNonce, MessagePayload,
	OutboundLaneData, VerificationError,
};
use bp_runtime::{HashOf, RangeInclusiveExt, VerifiedStorageProof};
use sp_std::vec::Vec;

/// 'Parsed' message delivery proof - inbound lane id and its state.
pub(crate) type ParsedMessagesDeliveryProofFromBridgedChain<T> =
	(LaneId, InboundLaneData<<T as frame_system::Config>::AccountId>);

/// Verify proof of Bridged -> This chain messages.
///
/// This function is used when Bridged chain is directly using GRANDPA finality. For Bridged
/// parachains, please use the `verify_messages_proof_from_parachain`.
///
/// The `messages_count` argument verification (sane limits) is supposed to be made
/// outside of this function. This function only verifies that the proof declares exactly
/// `messages_count` messages.
pub fn verify_messages_proof<T: Config<I>, I: 'static>(
	proof: FromBridgedChainMessagesProof<HashOf<BridgedChainOf<T, I>>>,
	messages_count: u32,
) -> Result<ProvedMessages<Message>, VerificationError> {
	let FromBridgedChainMessagesProof {
		bridged_header_hash,
		storage,
		lane,
		nonces_start,
		nonces_end,
	} = proof;
	let storage = BridgedHeaderChainOf::<T, I>::verify_storage_proof(bridged_header_hash, storage)
		.map_err(VerificationError::HeaderChain)?;
	let mut parser = StorageAdapter::<T, I> { storage, _dummy: Default::default() };
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

	// Check that the storage proof doesn't have any untouched keys.
	parser
		.storage
		.ensure_no_unused_keys()
		.map_err(VerificationError::StorageProof)?;

	Ok((lane, proved_lane_messages))
}

/// Verify proof of This -> Bridged chain messages delivery.
pub fn verify_messages_delivery_proof<T: Config<I>, I: 'static>(
	proof: FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChainOf<T, I>>>,
) -> Result<ParsedMessagesDeliveryProofFromBridgedChain<T>, VerificationError> {
	let FromBridgedChainMessagesDeliveryProof { bridged_header_hash, storage_proof, lane } = proof;
	let mut storage =
		T::BridgedHeaderChain::verify_storage_proof(bridged_header_hash, storage_proof)
			.map_err(VerificationError::HeaderChain)?;
	// Messages delivery proof is just proof of single storage key read => any error
	// is fatal.
	let storage_inbound_lane_data_key = bp_messages::storage_keys::inbound_lane_data_key(
		T::ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
		&lane,
	);
	let inbound_lane_data = storage
		.get_and_decode_mandatory(&storage_inbound_lane_data_key)
		.map_err(VerificationError::InboundLaneStorage)?;

	// check that the storage proof doesn't have any untouched trie nodes
	storage.ensure_no_unused_keys().map_err(VerificationError::StorageProof)?;

	Ok((lane, inbound_lane_data))
}

struct StorageAdapter<T, I> {
	storage: VerifiedStorageProof,
	_dummy: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> StorageAdapter<T, I> {
	fn read_and_decode_outbound_lane_data(
		&mut self,
		lane_id: &LaneId,
	) -> Result<Option<OutboundLaneData>, VerificationError> {
		let storage_outbound_lane_data_key = bp_messages::storage_keys::outbound_lane_data_key(
			T::ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
			lane_id,
		);

		self.storage
			.get_and_decode_optional(&storage_outbound_lane_data_key)
			.map_err(VerificationError::OutboundLaneStorage)
	}

	fn read_and_decode_message_payload(
		&mut self,
		message_key: &MessageKey,
	) -> Result<MessagePayload, VerificationError> {
		let storage_message_key = bp_messages::storage_keys::message_key(
			T::ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
			&message_key.lane_id,
			message_key.nonce,
		);
		self.storage
			.get_and_decode_mandatory(&storage_message_key)
			.map_err(VerificationError::MessageStorage)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{
		messages_generation::{
			encode_all_messages, encode_lane_data, generate_dummy_message,
			prepare_messages_storage_proof,
		},
		mock::*,
	};

	use bp_header_chain::{HeaderChainError, StoredHeaderDataBuilder};
	use bp_messages::LaneState;
	use bp_runtime::{HeaderId, StorageProofError};
	use codec::Encode;
	use sp_runtime::traits::Header;

	fn using_messages_proof<R>(
		nonces_end: MessageNonce,
		outbound_lane_data: Option<OutboundLaneData>,
		encode_message: impl Fn(MessageNonce, &MessagePayload) -> Option<Vec<u8>>,
		encode_outbound_lane_data: impl Fn(&OutboundLaneData) -> Vec<u8>,
		add_duplicate_key: bool,
		add_unused_key: bool,
		test: impl Fn(FromBridgedChainMessagesProof<BridgedHeaderHash>) -> R,
	) -> R {
		let (state_root, storage) = prepare_messages_storage_proof::<BridgedChain, ThisChain>(
			test_lane_id(),
			1..=nonces_end,
			outbound_lane_data,
			bp_runtime::UnverifiedStorageProofParams::default(),
			generate_dummy_message,
			encode_message,
			encode_outbound_lane_data,
			add_duplicate_key,
			add_unused_key,
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
			test(FromBridgedChainMessagesProof {
				bridged_header_hash,
				storage,
				lane: test_lane_id(),
				nonces_start: 1,
				nonces_end,
			})
		})
	}

	#[test]
	fn messages_proof_is_rejected_if_declared_less_than_actual_number_of_messages() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| { verify_messages_proof::<TestRuntime, ()>(proof, 5) }
			),
			Err(VerificationError::MessagesCountMismatch),
		);
	}

	#[test]
	fn messages_proof_is_rejected_if_declared_more_than_actual_number_of_messages() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| { verify_messages_proof::<TestRuntime, ()>(proof, 15) }
			),
			Err(VerificationError::MessagesCountMismatch),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_header_is_missing_from_the_chain() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| {
					let bridged_header_hash =
						pallet_bridge_grandpa::BestFinalized::<TestRuntime>::get().unwrap().1;
					pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::remove(
						bridged_header_hash,
					);
					verify_messages_proof::<TestRuntime, ()>(proof, 10)
				}
			),
			Err(VerificationError::HeaderChain(HeaderChainError::UnknownHeader)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_header_state_root_mismatches() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| {
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
					verify_messages_proof::<TestRuntime, ()>(proof, 10)
				}
			),
			Err(VerificationError::HeaderChain(HeaderChainError::StorageProof(
				StorageProofError::InvalidProof
			))),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_has_duplicate_trie_nodes() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				encode_all_messages,
				encode_lane_data,
				true,
				false,
				|proof| { verify_messages_proof::<TestRuntime, ()>(proof, 10) },
			),
			Err(VerificationError::HeaderChain(HeaderChainError::StorageProof(
				StorageProofError::InvalidProof
			))),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_has_unused_trie_nodes() {
		assert_eq!(
			using_messages_proof(
				10,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				true,
				|proof| { verify_messages_proof::<TestRuntime, ()>(proof, 10) },
			),
			Err(VerificationError::StorageProof(StorageProofError::UnusedKey)),
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
				false,
				false,
				|proof| verify_messages_proof::<TestRuntime, ()>(proof, 10)
			),
			Err(VerificationError::MessageStorage(StorageProofError::EmptyVal)),
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
				false,
				false,
				|proof| verify_messages_proof::<TestRuntime, ()>(proof, 10),
			),
			Err(VerificationError::MessageStorage(StorageProofError::DecodeError)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_outbound_lane_state_decode_fails() {
		matches!(
			using_messages_proof(
				10,
				Some(OutboundLaneData {
					state: LaneState::Opened,
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
				false,
				false,
				|proof| verify_messages_proof::<TestRuntime, ()>(proof, 10),
			),
			Err(VerificationError::OutboundLaneStorage(StorageProofError::DecodeError)),
		);
	}

	#[test]
	fn message_proof_is_rejected_if_it_is_empty() {
		assert_eq!(
			using_messages_proof(
				0,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| { verify_messages_proof::<TestRuntime, ()>(proof, 0) },
			),
			Err(VerificationError::EmptyMessageProof),
		);
	}

	#[test]
	fn non_empty_message_proof_without_messages_is_accepted() {
		assert_eq!(
			using_messages_proof(
				0,
				Some(OutboundLaneData {
					state: LaneState::Opened,
					oldest_unpruned_nonce: 1,
					latest_received_nonce: 1,
					latest_generated_nonce: 1,
				}),
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| verify_messages_proof::<TestRuntime, ()>(proof, 0),
			),
			Ok((
				test_lane_id(),
				ProvedLaneMessages {
					lane_state: Some(OutboundLaneData {
						state: LaneState::Opened,
						oldest_unpruned_nonce: 1,
						latest_received_nonce: 1,
						latest_generated_nonce: 1,
					}),
					messages: Vec::new(),
				},
			)),
		);
	}

	#[test]
	fn non_empty_message_proof_is_accepted() {
		assert_eq!(
			using_messages_proof(
				1,
				Some(OutboundLaneData {
					state: LaneState::Opened,
					oldest_unpruned_nonce: 1,
					latest_received_nonce: 1,
					latest_generated_nonce: 1,
				}),
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|proof| verify_messages_proof::<TestRuntime, ()>(proof, 1),
			),
			Ok((
				test_lane_id(),
				ProvedLaneMessages {
					lane_state: Some(OutboundLaneData {
						state: LaneState::Opened,
						oldest_unpruned_nonce: 1,
						latest_received_nonce: 1,
						latest_generated_nonce: 1,
					}),
					messages: vec![Message {
						key: MessageKey { lane_id: test_lane_id(), nonce: 1 },
						payload: vec![42],
					}],
				},
			))
		);
	}

	#[test]
	fn verify_messages_proof_does_not_panic_if_messages_count_mismatches() {
		assert_eq!(
			using_messages_proof(
				1,
				None,
				encode_all_messages,
				encode_lane_data,
				false,
				false,
				|mut proof| {
					proof.nonces_end = u64::MAX;
					verify_messages_proof::<TestRuntime, ()>(proof, u32::MAX)
				},
			),
			Err(VerificationError::MessagesCountMismatch),
		);
	}
}
