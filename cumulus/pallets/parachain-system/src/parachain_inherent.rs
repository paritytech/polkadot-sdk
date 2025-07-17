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

//! Cumulus parachain inherent related structures.

use alloc::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec,
	vec::Vec,
};
use core::fmt::Debug;
use cumulus_primitives_core::{
	relay_chain::{
		vstaging::ApprovedPeerId, BlockNumber as RelayChainBlockNumber, BlockNumber,
		Header as RelayHeader,
	},
	InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
};
use cumulus_primitives_parachain_inherent::{HashedMessage, ParachainInherentData};
use frame_support::{
	defensive,
	pallet_prelude::{Decode, DecodeWithMemTracking, Encode},
};
use scale_info::TypeInfo;
use sp_core::{bounded::BoundedSlice, Get};

/// A structure that helps identify a message inside a collection of messages sorted by `sent_at`.
///
/// This structure contains a `sent_at` field and a reverse index. Using this information, we can
/// identify a message inside a sorted collection by walking back `reverse_idx` positions starting
/// from the last message that has the provided `sent_at`.
///
/// We use a reverse index instead of a normal index because sometimes the messages at the
/// beginning of the collection are being pruned.
///
/// # Example
///
///
/// For the collection
/// `msgs = [{sent_at: 0}, {sent_at: 1}, {sent_at: 1}, {sent_at: 1}, {sent_at: 1}, {sent_at: 3}]`
///
/// `InboundMessageId {sent_at: 1, reverse_idx: 0}` points to `msgs[4]`
/// `InboundMessageId {sent_at: 1, reverse_idx: 3}` points to `msgs[1]`
/// `InboundMessageId {sent_at: 1, reverse_idx: 4}` points to `msgs[0]`
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Default,
	sp_runtime::RuntimeDebug,
	PartialEq,
	TypeInfo,
)]
pub struct InboundMessageId {
	/// The block number at which this message was added to the message passing queue
	/// on the relay chain.
	pub sent_at: BlockNumber,
	/// The reverse index of the message in the collection of messages sent at `sent_at`.
	pub reverse_idx: u32,
}

/// A message that was received by the parachain.
pub trait InboundMessage {
	/// The corresponding compressed message.
	/// This should be an equivalent message that stores the same metadata as the current message,
	/// but stores only a hash of the message data.
	type CompressedMessage: Debug;

	/// Gets the message data.
	fn data(&self) -> &[u8];

	/// Gets the relay chain number where the current message was pushed to the corresponding
	/// relay chain queue.
	fn sent_at(&self) -> RelayChainBlockNumber;

	/// Converts the current message into a `CompressedMessage`
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
pub struct InboundMessagesCollection<Message: InboundMessage> {
	messages: Vec<Message>,
}

impl<Message: InboundMessage> InboundMessagesCollection<Message> {
	/// Creates a new instance of `InboundMessagesCollection` that contains the provided `messages`.
	pub fn new(messages: Vec<Message>) -> Self {
		Self { messages }
	}

	/// Drop all the messages up to `last_processed_msg`.
	pub fn drop_processed_messages(&mut self, last_processed_msg: &InboundMessageId) {
		let mut last_processed_msg_idx = None;
		let messages = &mut self.messages;
		for (idx, message) in messages.iter().enumerate().rev() {
			let sent_at = message.sent_at();
			if sent_at == last_processed_msg.sent_at {
				last_processed_msg_idx = idx.checked_sub(last_processed_msg.reverse_idx as usize);
				break;
			}
			// If we build on the same relay parent twice, we will receive the same messages again
			// while `last_processed_msg` may have been increased. We need this check to make sure
			// that the old messages are dropped.
			if sent_at < last_processed_msg.sent_at {
				last_processed_msg_idx = Some(idx);
				break;
			}
		}
		if let Some(last_processed_msg_idx) = last_processed_msg_idx {
			messages.drain(..=last_processed_msg_idx);
		}
	}

	/// Converts `self` into an [`AbridgedInboundMessagesCollection`].
	///
	/// The first messages in `self` (up to the provided `size_limit`) are kept in their current
	/// form (they will contain the full message data).
	/// The messages that exceed that limit are hashed.
	pub fn into_abridged(
		self,
		size_limit: &mut usize,
	) -> AbridgedInboundMessagesCollection<Message> {
		let mut messages = self.messages;

		let mut split_off_pos = messages.len();
		for (idx, message) in messages.iter().enumerate() {
			if *size_limit < message.data().len() {
				break;
			}
			*size_limit -= message.data().len();

			split_off_pos = idx + 1;
		}

		let extra_messages = messages.split_off(split_off_pos);
		let hashed_messages = extra_messages.iter().map(|msg| msg.to_compressed()).collect();

		AbridgedInboundMessagesCollection { full_messages: messages, hashed_messages }
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
pub struct AbridgedInboundMessagesCollection<Message: InboundMessage> {
	full_messages: Vec<Message>,
	hashed_messages: Vec<Message::CompressedMessage>,
}

impl<Message: InboundMessage> AbridgedInboundMessagesCollection<Message> {
	/// Gets a tuple containing both the full messages and the hashed messages
	/// stored by the current collection.
	pub fn messages(&self) -> (&[Message], &[Message::CompressedMessage]) {
		(&self.full_messages, &self.hashed_messages)
	}

	/// Check that the current collection contains as many full messages as possible.
	///
	/// The `AbridgedInboundMessagesCollection` is provided to the runtime by a collator.
	/// A malicious collator can provide a collection that contains no full messages or fewer
	/// full messages than possible, leading to censorship.
	pub fn check_enough_messages_included(&self, collection_name: &str) {
		if self.hashed_messages.is_empty() {
			return;
		}

		// Ideally, we should check that the collection contains as many full messages as possible
		// without exceeding the max expected size. The worst case scenario is that were the first
		// message that had to be hashed is a max size message. So in this case, the min expected
		// size would be `max_expected_size - max_msg_size`. However, there are multiple issues:
		// 1. The max message size config can change while we still have to process messages with
		//    the old max message size.
		// 2. We can't access the max downward message size from the parachain runtime.
		//
		// So the safest approach is to check that there is at least 1 full message.
		assert!(
			self.full_messages.len() >= 1,
			"[{}] Advancement rule violation: mandatory messages missing",
			collection_name,
		);
	}
}

impl<Message: InboundMessage> Default for AbridgedInboundMessagesCollection<Message> {
	fn default() -> Self {
		Self { full_messages: vec![], hashed_messages: vec![] }
	}
}

impl InboundMessage for InboundDownwardMessage<RelayChainBlockNumber> {
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
	/// Returns an iterator over the messages that maps them to `BoundedSlices`.
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
					defensive!("Inbound Downward message was too long; dropping");
					None
				},
			})
	}
}

impl InboundMessage for (ParaId, InboundHrmpMessage) {
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
	/// Returns a list of all the unique senders.
	pub fn get_senders(&self) -> BTreeSet<ParaId> {
		self.full_messages
			.iter()
			.map(|(sender, _msg)| *sender)
			.chain(self.hashed_messages.iter().map(|(sender, _msg)| *sender))
			.collect()
	}

	/// Returns an iterator over the deconstructed messages.
	pub fn flat_msgs_iter(&self) -> impl Iterator<Item = (ParaId, RelayChainBlockNumber, &[u8])> {
		self.full_messages
			.iter()
			.map(|&(sender, ref message)| (sender, message.sent_at, &message.data[..]))
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
pub struct BasicParachainInherentData {
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
	/// Creates a new instance of `InboundMessagesData` with the provided messages.
	pub fn new(
		dmq_msgs: AbridgedInboundDownwardMessages,
		hrmp_msgs: AbridgedInboundHrmpMessages,
	) -> Self {
		Self { downward_messages: dmq_msgs, horizontal_messages: hrmp_msgs }
	}
}

/// Deconstructs a `ParachainInherentData` instance.
pub fn deconstruct_parachain_inherent_data(
	data: ParachainInherentData,
) -> (BasicParachainInherentData, InboundDownwardMessages, InboundHrmpMessages) {
	(
		BasicParachainInherentData {
			validation_data: data.validation_data,
			relay_chain_state: data.relay_chain_state,
			relay_parent_descendants: data.relay_parent_descendants,
			collator_peer_id: data.collator_peer_id,
		},
		InboundDownwardMessages::new(data.downward_messages),
		InboundHrmpMessages::from_map(data.horizontal_messages),
	)
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
	fn into_abridged_works() {
		let msgs = InboundDownwardMessages::new(vec![]);
		let mut size_limit = 0;
		let abridged_msgs = msgs.into_abridged(&mut size_limit);
		assert_eq!(size_limit, 0);
		assert_eq!(&abridged_msgs.full_messages, &vec![]);
		assert_eq!(abridged_msgs.hashed_messages, vec![]);

		let msgs_vec = build_inbound_dm_vec(&[(0, 100), (0, 100), (0, 150), (0, 50)]);
		let msgs = InboundDownwardMessages::new(msgs_vec.clone());

		let mut size_limit = 150;
		let abridged_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 50);
		assert_eq!(&abridged_msgs.full_messages, &msgs_vec[..1]);
		assert_eq!(
			abridged_msgs.hashed_messages,
			vec![(&msgs_vec[1]).into(), (&msgs_vec[2]).into(), (&msgs_vec[3]).into()]
		);

		let mut size_limit = 200;
		let abridged_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 0);
		assert_eq!(&abridged_msgs.full_messages, &msgs_vec[..2]);
		assert_eq!(
			abridged_msgs.hashed_messages,
			vec![(&msgs_vec[2]).into(), (&msgs_vec[3]).into()]
		);

		let mut size_limit = 399;
		let abridged_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 49);
		assert_eq!(&abridged_msgs.full_messages, &msgs_vec[..3]);
		assert_eq!(abridged_msgs.hashed_messages, vec![(&msgs_vec[3]).into()]);

		let mut size_limit = 400;
		let abridged_msgs = msgs.clone().into_abridged(&mut size_limit);
		assert_eq!(size_limit, 0);
		assert_eq!(&abridged_msgs.full_messages, &msgs_vec);
		assert_eq!(abridged_msgs.hashed_messages, vec![]);
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

	#[test]
	fn check_enough_messages_included_works() {
		let mut messages = AbridgedInboundHrmpMessages {
			full_messages: vec![(
				1000.into(),
				InboundHrmpMessage { sent_at: 0, data: vec![1; 100] },
			)],
			hashed_messages: vec![(
				2000.into(),
				HashedMessage { sent_at: 1, msg_hash: Default::default() },
			)],
		};

		messages.check_enough_messages_included("Test");

		messages.full_messages = vec![];
		let result = std::panic::catch_unwind(|| messages.check_enough_messages_included("Test"));
		assert!(result.is_err());

		messages.hashed_messages = vec![];
		messages.check_enough_messages_included("Test");
	}
}
