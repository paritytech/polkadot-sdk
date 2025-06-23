// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Cumulus parachain inherent
//!
//! The [`RawParachainInherentData`] is the data that is passed by the collator to the parachain
//! runtime. The runtime will use this data to execute messages from other parachains/the relay
//! chain or to read data from the relay chain state. When the parachain is validated by a parachain
//! validator on the relay chain, this data is checked for correctness. If the data passed by the
//! collator to the runtime isn't correct, the parachain candidate is considered invalid.
//!
//! To create a [`RawParachainInherentData`] for a specific relay chain block, there exists the
//! `ParachainInherentDataExt` trait in `cumulus-client-parachain-inherent` that helps with this.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::btree_map::BTreeMap, vec, vec::Vec};
use core::fmt::Debug;
use cumulus_primitives_core::{
	relay_chain::{
		vstaging::ApprovedPeerId, BlakeTwo256, BlockNumber as RelayChainBlockNumber,
		Hash as RelayHash, HashT as _, Header as RelayHeader, InboundMessageId,
	},
	InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
};
use scale_info::TypeInfo;
use sp_core::{bounded::BoundedSlice, Get};
use sp_inherents::InherentIdentifier;

/// The identifier for the parachain inherent.
pub const PARACHAIN_INHERENT_IDENTIFIER_V0: InherentIdentifier = *b"sysi1337";
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"sysi1338";

/// Legacy ParachainInherentData that is kept around for backward compatibility.
/// Can be removed once we can safely assume that parachain nodes provide the
/// `relay_parent_descendants` and `collator_peer_id` fields.
pub mod v0 {
	use alloc::{collections::BTreeMap, vec::Vec};
	use cumulus_primitives_core::{
		InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
	};
	use scale_info::TypeInfo;

	/// The inherent data that is passed by the collator to the parachain runtime.
	#[derive(
		codec::Encode,
		codec::Decode,
		codec::DecodeWithMemTracking,
		sp_core::RuntimeDebug,
		Clone,
		PartialEq,
		TypeInfo,
	)]
	pub struct RawParachainInherentData {
		pub validation_data: PersistedValidationData,
		/// A storage proof of a predefined set of keys from the relay-chain.
		///
		/// Specifically this witness contains the data for:
		///
		/// - the current slot number at the given relay parent
		/// - active host configuration as per the relay parent,
		/// - the relay dispatch queue sizes
		/// - the list of egress HRMP channels (in the list of recipients form)
		/// - the metadata for the egress HRMP channels
		pub relay_chain_state: sp_trie::StorageProof,
		/// Downward messages in the order they were sent.
		pub downward_messages: Vec<InboundDownwardMessage>,
		/// HRMP messages grouped by channels. The messages in the inner vec must be in order they
		/// were sent. In combination with the rule of no more than one message in a channel per
		/// block, this means `sent_at` is **strictly** greater than the previous one (if any).
		pub horizontal_messages: BTreeMap<ParaId, Vec<InboundHrmpMessage>>,
	}
}

/// An inbound message whose content was hashed.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
pub struct HashedMessage {
	pub sent_at: RelayChainBlockNumber,
	pub msg_hash: sp_core::H256,
}

impl From<&InboundDownwardMessage<RelayChainBlockNumber>> for HashedMessage {
	fn from(msg: &InboundDownwardMessage<RelayChainBlockNumber>) -> Self {
		Self { sent_at: msg.sent_at, msg_hash: MessageQueueChain::hash_msg(&msg.msg) }
	}
}

impl From<&InboundHrmpMessage> for HashedMessage {
	fn from(msg: &InboundHrmpMessage) -> Self {
		Self { sent_at: msg.sent_at, msg_hash: MessageQueueChain::hash_msg(&msg.data) }
	}
}

pub trait InboundMessage<BlockNumber> {
	type CompressedMessage: Debug;

	fn data(&self) -> &[u8];

	fn sent_at(&self) -> BlockNumber;

	fn to_compressed(&self) -> Self::CompressedMessage;
}

/// A collection of inbound messages.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
pub struct InboundMessagesCollection<Message: InboundMessage<RelayChainBlockNumber>> {
	messages: Vec<Message>,
}

impl<Message: InboundMessage<RelayChainBlockNumber>> InboundMessagesCollection<Message> {
	pub fn new(messages: Vec<Message>) -> Self {
		Self { messages }
	}

	/// Drop all the messages up to `last_processed_msg`.
	pub fn drop_processed_messages(
		&mut self,
		last_processed_msg: &InboundMessageId<RelayChainBlockNumber>,
	) {
		let mut last_processed_msg_idx = None;
		let messages = &mut self.messages;
		for (rev_idx, message) in messages.iter().rev().enumerate() {
			let idx = (messages.len() - rev_idx - 1) as u32;
			let sent_at = message.sent_at();
			if sent_at == last_processed_msg.sent_at {
				last_processed_msg_idx = idx.checked_sub(last_processed_msg.reverse_idx);
				break;
			}
			if sent_at < last_processed_msg.sent_at {
				last_processed_msg_idx = Some(idx);
				break;
			}
		}
		if let Some(last_processed_msg_idx) = last_processed_msg_idx {
			messages.drain(..last_processed_msg_idx as usize + 1);
		}
	}

	pub fn into_abridged(
		self,
		size_limit: &mut usize,
	) -> AbridgedInboundMessagesCollection<Message> {
		let mut messages = self.messages;

		let mut maybe_split_off_pos = None;
		for (idx, message) in messages.iter().enumerate() {
			if *size_limit < message.data().len() {
				break;
			}
			*size_limit -= message.data().len();

			maybe_split_off_pos = Some(idx + 1);
		}

		let mut compressed_messages = vec![];
		if let Some(split_off_pos) = maybe_split_off_pos {
			let extra_messages = messages.split_off(split_off_pos);
			compressed_messages = extra_messages.iter().map(|msg| msg.to_compressed()).collect();
		}

		AbridgedInboundMessagesCollection {
			full_messages: messages,
			hashed_messages: compressed_messages,
		}
	}
}

/// A compressed collection of inbound messages.
///
/// The first messages in the collection (up to a limit) contain the full message data.
/// The messages that exceed that limit are hashed.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
pub struct AbridgedInboundMessagesCollection<Message: InboundMessage<RelayChainBlockNumber>> {
	full_messages: Vec<Message>,
	hashed_messages: Vec<Message::CompressedMessage>,
}

impl<Message: InboundMessage<RelayChainBlockNumber>> AbridgedInboundMessagesCollection<Message> {
	pub fn messages(&self) -> (&[Message], &[Message::CompressedMessage]) {
		(&self.full_messages, &self.hashed_messages)
	}

	pub fn check_advancement_rule(&self, collection_name: &str, max_size: usize) {
		if self.hashed_messages.len() > 0 {
			let mut size = 0usize;
			for msg in &self.full_messages {
				size = size.saturating_add(msg.data().len());
			}
			let min_size = ((max_size as f64) * 0.75) as usize;

			assert!(
				size >= min_size,
				"[{}] Advancement rule violation: mandatory messages size smaller than expected: \
				{} < {}",
				collection_name,
				size,
				min_size
			);
		}
	}
}

impl<Message: InboundMessage<RelayChainBlockNumber>> Default
	for AbridgedInboundMessagesCollection<Message>
{
	fn default() -> Self {
		Self { full_messages: vec![], hashed_messages: vec![] }
	}
}

impl InboundMessage<RelayChainBlockNumber> for InboundDownwardMessage<RelayChainBlockNumber> {
	type CompressedMessage = HashedMessage;

	fn data(&self) -> &[u8] {
		&self.msg
	}

	fn sent_at(&self) -> RelayChainBlockNumber {
		self.sent_at
	}

	fn to_compressed(&self) -> Self::CompressedMessage {
		self.into()
	}
}

pub type InboundDownwardMessages =
	InboundMessagesCollection<InboundDownwardMessage<RelayChainBlockNumber>>;

pub type AbridgedInboundDownwardMessages =
	AbridgedInboundMessagesCollection<InboundDownwardMessage<RelayChainBlockNumber>>;

impl AbridgedInboundDownwardMessages {
	pub fn bounded_msgs_iter<MaxMessageLen: Get<u32>>(
		&self,
	) -> impl Iterator<Item = BoundedSlice<u8, MaxMessageLen>> {
		self.full_messages
			.iter()
			// Note: we are not using `.defensive()` here since that prints the whole value to
			// console. In case that the message is too long, this clogs up the log quite badly.
			.filter_map(|m| match BoundedSlice::try_from(&m.msg[..]) {
				Ok(bounded) => Some(bounded),
				Err(_) => {
					log::error!("Inbound Downward message was too long; dropping");
					debug_assert!(false);
					None
				},
			})
	}
}

impl InboundMessage<RelayChainBlockNumber> for (ParaId, InboundHrmpMessage) {
	type CompressedMessage = (ParaId, HashedMessage);

	fn data(&self) -> &[u8] {
		&self.1.data
	}

	fn sent_at(&self) -> RelayChainBlockNumber {
		self.1.sent_at
	}

	fn to_compressed(&self) -> Self::CompressedMessage {
		let (sender, message) = self;
		(*sender, message.into())
	}
}

pub type InboundHrmpMessages = InboundMessagesCollection<(ParaId, InboundHrmpMessage)>;

impl InboundHrmpMessages {
	// Prepare horizontal messages for a more convenient processing:
	//
	// Instead of a mapping from a para to a list of inbound HRMP messages, we will have a
	// list of tuples `(sender, message)` first ordered by `sent_at` (the relay chain block
	// number in which the message hit the relay-chain) and second ordered by para id
	// ascending.
	pub fn from_map(messages_map: BTreeMap<ParaId, Vec<InboundHrmpMessage>>) -> Self {
		let mut messages = messages_map
			.into_iter()
			.flat_map(|(sender, channel_contents)| {
				channel_contents.into_iter().map(move |message| (sender, message))
			})
			.collect::<Vec<_>>();
		messages.sort_by(|(sender_a, msg_a), (sender_b, msg_b)| {
			// first sort by sent-at and then by the para id
			(msg_a.sent_at, sender_a).cmp(&(msg_b.sent_at, sender_b))
		});

		Self { messages }
	}
}

pub type AbridgedInboundHrmpMessages =
	AbridgedInboundMessagesCollection<(ParaId, InboundHrmpMessage)>;

impl AbridgedInboundHrmpMessages {
	pub fn get_senders(&self) -> Vec<ParaId> {
		let mut senders = vec![];

		let messages = self.full_messages.iter().map(|(sender, _msg)| sender);
		let hashed_messages = self.hashed_messages.iter().map(|(sender, _msg)| sender);
		for sender in messages.chain(hashed_messages) {
			if let Err(pos) = senders.binary_search(sender) {
				senders.insert(pos, *sender);
			}
		}

		senders
	}

	pub fn flat_msgs_iter(&self) -> impl Iterator<Item = (ParaId, RelayChainBlockNumber, &[u8])> {
		self.full_messages
			.iter()
			.map(|&(sender, ref message)| (sender, message.sent_at, &message.data[..]))
	}
}

/// The inherent data that is passed by the collator to the parachain runtime.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
pub struct RawParachainInherentData {
	pub validation_data: PersistedValidationData,
	/// A storage proof of a predefined set of keys from the relay-chain.
	///
	/// Specifically this witness contains the data for:
	///
	/// - the current slot number at the given relay parent
	/// - active host configuration as per the relay parent,
	/// - the relay dispatch queue sizes
	/// - the list of egress HRMP channels (in the list of recipients form)
	/// - the metadata for the egress HRMP channels
	pub relay_chain_state: sp_trie::StorageProof,
	/// Downward messages in the order they were sent.
	pub downward_messages: Vec<InboundDownwardMessage>,
	/// HRMP messages grouped by channels. The messages in the inner vec must be in order they
	/// were sent. In combination with the rule of no more than one message in a channel per block,
	/// this means `sent_at` is **strictly** greater than the previous one (if any).
	pub horizontal_messages: BTreeMap<ParaId, Vec<InboundHrmpMessage>>,
	/// Contains the relay parent header and its descendants.
	/// This information is used to ensure that a parachain node builds blocks
	/// at a specified offset from the chain tip rather than directly at the tip.
	pub relay_parent_descendants: Vec<RelayHeader>,
	/// Contains the collator peer ID, which is later sent by the parachain to the
	/// relay chain via a UMP signal to promote the reputation of the given peer ID.
	pub collator_peer_id: Option<ApprovedPeerId>,
}

// Upgrades the ParachainInherentData v0 to the newest format.
impl Into<RawParachainInherentData> for v0::RawParachainInherentData {
	fn into(self) -> RawParachainInherentData {
		RawParachainInherentData {
			validation_data: self.validation_data,
			relay_chain_state: self.relay_chain_state,
			downward_messages: self.downward_messages,
			horizontal_messages: self.horizontal_messages,
			relay_parent_descendants: Vec::new(),
			collator_peer_id: None,
		}
	}
}

#[cfg(feature = "std")]
impl RawParachainInherentData {
	/// Transforms [`RawParachainInherentData`] into [`v0::RawParachainInherentData`]. Can be used
	/// to create inherent data compatible with old runtimes.
	fn as_v0(&self) -> v0::RawParachainInherentData {
		v0::RawParachainInherentData {
			validation_data: self.validation_data.clone(),
			relay_chain_state: self.relay_chain_state.clone(),
			downward_messages: self.downward_messages.clone(),
			horizontal_messages: self.horizontal_messages.clone(),
		}
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for RawParachainInherentData {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(PARACHAIN_INHERENT_IDENTIFIER_V0, &self.as_v0())?;
		inherent_data.put_data(INHERENT_IDENTIFIER, &self)
	}

	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

impl RawParachainInherentData {
	pub fn deconstruct(
		self,
	) -> (ParachainInherentData, InboundDownwardMessages, InboundHrmpMessages) {
		(
			ParachainInherentData {
				validation_data: self.validation_data,
				relay_chain_state: self.relay_chain_state,
				relay_parent_descendants: self.relay_parent_descendants,
				collator_peer_id: self.collator_peer_id,
			},
			InboundDownwardMessages::new(self.downward_messages),
			InboundHrmpMessages::from_map(self.horizontal_messages),
		)
	}
}

/// The basic inherent data that is passed by the collator to the parachain runtime.
/// This data doesn't contain any messages.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
pub struct ParachainInherentData {
	pub validation_data: PersistedValidationData,
	pub relay_chain_state: sp_trie::StorageProof,
	pub relay_parent_descendants: Vec<RelayHeader>,
	pub collator_peer_id: Option<ApprovedPeerId>,
}

/// The messages that are passed by the collator to the parachain runtime as part of the
/// inherent data.
#[derive(
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	sp_core::RuntimeDebug,
	Clone,
	PartialEq,
	TypeInfo,
)]
pub struct InboundMessagesData {
	pub downward_messages: AbridgedInboundDownwardMessages,
	pub horizontal_messages: AbridgedInboundHrmpMessages,
}

impl InboundMessagesData {
	pub fn new(
		dmq_msgs: AbridgedInboundDownwardMessages,
		hrmp_msgs: AbridgedInboundHrmpMessages,
	) -> Self {
		Self { downward_messages: dmq_msgs, horizontal_messages: hrmp_msgs }
	}
}

/// This struct provides ability to extend a message queue chain (MQC) and compute a new head.
///
/// MQC is an instance of a [hash chain] applied to a message queue. Using a hash chain it's
/// possible to represent a sequence of messages using only a single hash.
///
/// A head for an empty chain is agreed to be a zero hash.
///
/// An instance is used to track either DMP from the relay chain or HRMP across a channel.
/// But a given instance is never used to track both. Therefore, you should call either
/// `extend_downward` or `extend_hrmp`, but not both methods on a single instance.
///
/// [hash chain]: https://en.wikipedia.org/wiki/Hash_chain
#[derive(Default, Clone, codec::Encode, codec::Decode, scale_info::TypeInfo)]
pub struct MessageQueueChain(RelayHash);

impl MessageQueueChain {
	/// Create a new instance initialized to `hash`.
	pub fn new(hash: RelayHash) -> Self {
		Self(hash)
	}

	fn hash_msg(msg: &Vec<u8>) -> sp_core::H256 {
		BlakeTwo256::hash_of(msg)
	}

	/// Extend the hash chain with a `HashedMessage`.
	pub fn extend_with_hashed_msg(&mut self, hashed_msg: &HashedMessage) -> &mut Self {
		let prev_head = self.0;
		self.0 = BlakeTwo256::hash_of(&(prev_head, hashed_msg.sent_at, &hashed_msg.msg_hash));
		self
	}

	/// Extend the hash chain with an HRMP message. This method should be used only when
	/// this chain is tracking HRMP.
	pub fn extend_hrmp(&mut self, horizontal_message: &InboundHrmpMessage) -> &mut Self {
		self.extend_with_hashed_msg(&horizontal_message.into())
	}

	/// Extend the hash chain with a downward message. This method should be used only when
	/// this chain is tracking DMP.
	pub fn extend_downward(&mut self, downward_message: &InboundDownwardMessage) -> &mut Self {
		self.extend_with_hashed_msg(&downward_message.into())
	}

	/// Return the current head of the message queue chain.
	/// This is agreed to be the zero hash for an empty chain.
	pub fn head(&self) -> RelayHash {
		self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn build_inbound_dm_vec(
		info: &[(RelayChainBlockNumber, usize)],
	) -> Vec<InboundDownwardMessage<RelayChainBlockNumber>> {
		let mut messages = vec![];
		for (sent_at, size) in info.iter() {
			let data = vec![1; *size];
			messages.push(InboundDownwardMessage { sent_at: *sent_at, msg: data })
		}
		messages
	}

	#[test]
	fn drop_processed_messages_works() {
		let msgs_vec =
			build_inbound_dm_vec(&[(0, 0), (0, 0), (2, 0), (2, 0), (2, 0), (2, 0), (3, 0)]);

		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());

		msgs.drop_processed_messages(&InboundMessageId { sent_at: 3, reverse_idx: 0 });
		assert_eq!(msgs.messages, []);

		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());
		msgs.drop_processed_messages(&InboundMessageId { sent_at: 2, reverse_idx: 0 });
		assert_eq!(msgs.messages, msgs_vec[6..]);
		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());
		msgs.drop_processed_messages(&InboundMessageId { sent_at: 2, reverse_idx: 1 });
		assert_eq!(msgs.messages, msgs_vec[5..]);
		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());
		msgs.drop_processed_messages(&InboundMessageId { sent_at: 2, reverse_idx: 4 });
		assert_eq!(msgs.messages, msgs_vec[2..]);

		// Go back starting from the last message sent at block 2, with 1 more message than the
		// total number of messages sent at 2.
		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());
		msgs.drop_processed_messages(&InboundMessageId { sent_at: 2, reverse_idx: 5 });
		assert_eq!(msgs.messages, msgs_vec[1..]);

		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());
		msgs.drop_processed_messages(&InboundMessageId { sent_at: 0, reverse_idx: 1 });
		assert_eq!(msgs.messages, msgs_vec[1..]);
		// Go back starting from the last message sent at block 0, with 1 more message than the
		// total number of messages sent at 0.
		let mut msgs = InboundDownwardMessages::new(msgs_vec.clone());
		msgs.drop_processed_messages(&InboundMessageId { sent_at: 0, reverse_idx: 3 });
		assert_eq!(msgs.messages, msgs_vec);
	}

	#[test]
	fn into_compressed_works() {
		let msgs_vec = build_inbound_dm_vec(&[(0, 100), (0, 100), (0, 150), (0, 50)]);
		let msgs = InboundDownwardMessages::new(msgs_vec.clone());

		let mut size_limit = 150;
		let compressed_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 50);
		assert_eq!(&compressed_msgs.full_messages, &msgs_vec[..1]);
		assert_eq!(
			compressed_msgs.hashed_messages,
			vec![(&msgs_vec[1]).into(), (&msgs_vec[2]).into(), (&msgs_vec[3]).into()]
		);

		let mut size_limit = 200;
		let compressed_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 0);
		assert_eq!(&compressed_msgs.full_messages, &msgs_vec[..2]);
		assert_eq!(
			compressed_msgs.hashed_messages,
			vec![(&msgs_vec[2]).into(), (&msgs_vec[3]).into()]
		);

		let mut size_limit = 399;
		let compressed_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 49);
		assert_eq!(&compressed_msgs.full_messages, &msgs_vec[..3]);
		assert_eq!(compressed_msgs.hashed_messages, vec![(&msgs_vec[3]).into()]);

		let mut size_limit = 400;
		let compressed_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 0);
		assert_eq!(&compressed_msgs.full_messages, &msgs_vec);
		assert_eq!(compressed_msgs.hashed_messages, vec![]);
	}

	#[test]
	fn from_map_works() {
		let mut messages_map: BTreeMap<ParaId, Vec<InboundHrmpMessage>> = BTreeMap::new();
		messages_map.insert(
			1000.into(),
			vec![
				InboundHrmpMessage { sent_at: 0, data: vec![0] },
				InboundHrmpMessage { sent_at: 0, data: vec![1] },
				InboundHrmpMessage { sent_at: 1, data: vec![2] },
			],
		);
		messages_map.insert(
			2000.into(),
			vec![
				InboundHrmpMessage { sent_at: 0, data: vec![3] },
				InboundHrmpMessage { sent_at: 0, data: vec![4] },
				InboundHrmpMessage { sent_at: 1, data: vec![5] },
			],
		);
		messages_map.insert(
			3000.into(),
			vec![
				InboundHrmpMessage { sent_at: 0, data: vec![6] },
				InboundHrmpMessage { sent_at: 1, data: vec![7] },
				InboundHrmpMessage { sent_at: 2, data: vec![8] },
				InboundHrmpMessage { sent_at: 3, data: vec![9] },
				InboundHrmpMessage { sent_at: 4, data: vec![10] },
			],
		);

		let msgs = InboundHrmpMessages::from_map(messages_map);
		assert_eq!(
			msgs.messages,
			[
				(1000.into(), InboundHrmpMessage { sent_at: 0, data: vec![0] }),
				(1000.into(), InboundHrmpMessage { sent_at: 0, data: vec![1] }),
				(2000.into(), InboundHrmpMessage { sent_at: 0, data: vec![3] }),
				(2000.into(), InboundHrmpMessage { sent_at: 0, data: vec![4] }),
				(3000.into(), InboundHrmpMessage { sent_at: 0, data: vec![6] }),
				(1000.into(), InboundHrmpMessage { sent_at: 1, data: vec![2] }),
				(2000.into(), InboundHrmpMessage { sent_at: 1, data: vec![5] }),
				(3000.into(), InboundHrmpMessage { sent_at: 1, data: vec![7] }),
				(3000.into(), InboundHrmpMessage { sent_at: 2, data: vec![8] }),
				(3000.into(), InboundHrmpMessage { sent_at: 3, data: vec![9] }),
				(3000.into(), InboundHrmpMessage { sent_at: 4, data: vec![10] })
			]
		)
	}
}
