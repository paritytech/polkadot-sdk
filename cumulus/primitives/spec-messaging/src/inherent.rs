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

//! The node/runtime consumption interface: what the runtime asks the node to fetch
//! ([`ConsumedStreams`], via the `consumed_streams()` runtime API) and what the node delivers in
//! the messaging inherent ([`MessagingInherentData`] and [`LossyRead`]).
//!
//! Both sides are keyed by the consumer-side typed IDs ([`InChannelId`], [`OutChannelId`],
//! [`BroadcastId`]), one map per stream kind, so kind/discipline mismatches and duplicate entries
//! for the same stream are unrepresentable.
//!
//! The inherent carries payloads and placement hints (`LossyRead::frontier`). No roots, no proofs.
//! The runtime verifies by recomputation: hash payloads into leaves, append to the frontier, and
//! record the interval for the candidate's lift to bind.

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError, Input};
use scale_info::TypeInfo;

use crate::{
	channel::{BroadcastId, InChannelId, OutChannelId},
	mmr::{MessagePosition, MmrFrontier},
};

/// Key of the messaging inherent in `InherentData`.
pub const INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = *b"specmsg0";

pub type Payload = Vec<u8>;

/// Everything the inherent provider should fetch, and from which position on, as returned by
/// `consumed_streams()`. Field-for-field the shape of [`MessagingInherentData`].
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct ConsumedStreams {
	/// Inbound channels to consume (watched and with delivery-queue room), mapped to the stored
	/// frontier. Its `leaf_count()` is the fetch cursor, and fetch responses must match the
	/// frontier.
	pub channels: BTreeMap<InChannelId, MmrFrontier>,
	/// Outbound channels whose recipient's register should be read, mapped to the cursor
	/// ([`crate::OutChannelState::register_cursor`]).
	pub registers: BTreeMap<OutChannelId, MessagePosition>,
	/// Active broadcast subscriptions, mapped to the cursor (`highwater + 1`, or `0` before the
	/// first read).
	pub events: BTreeMap<BroadcastId, MessagePosition>,
}

impl ConsumedStreams {
	pub fn is_empty(&self) -> bool {
		self.channels.is_empty() && self.registers.is_empty() && self.events.is_empty()
	}

	pub fn len(&self) -> usize {
		self.channels.len() + self.registers.len() + self.events.len()
	}
}

/// Lossy consumption item (register or event read): carries the stream's frontier just before its
/// first payload, standing in for the frontier a lossy consumer does not store.
///
/// Trust-free: a false `frontier` yields an endpoint no lift can bind.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct LossyRead {
	pub frontier: MmrFrontier,
	pub payloads: Vec<Payload>,
}

/// The messaging inherent: this block's consumption, keyed by the typed consumer-side IDs.
///
/// Both the node's offer (`InherentData`) and the extrinsic `create_inherent` builds from it use
/// this type. `Decode` enforces strictly increasing keys in every map, rejecting unsorted or
/// duplicate entries that SCALE's default `BTreeMap` decode would silently accept.
#[derive(Clone, Encode, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MessagingInherentData {
	/// Prefix discipline: the sender's next messages on the channel, in order, appended to the
	/// `InChannels` entry's frontier.
	pub channels: BTreeMap<InChannelId, Vec<Payload>>,
	/// Register reads on our outbound channels' `Ack` streams (lossy discipline).
	pub registers: BTreeMap<OutChannelId, LossyRead>,
	/// Event reads on active broadcast subscriptions (lossy discipline).
	pub events: BTreeMap<BroadcastId, LossyRead>,
}

impl MessagingInherentData {
	/// Whether this block consumed nothing. Then no inherent is placed.
	pub fn is_empty(&self) -> bool {
		self.channels.is_empty() && self.registers.is_empty() && self.events.is_empty()
	}

	/// Total number of streams touched across all three maps.
	pub fn len(&self) -> usize {
		self.channels.len() + self.registers.len() + self.events.len()
	}
}

fn decode_strict_map<K: Decode + Ord, V: Decode, I: Input>(
	input: &mut I,
) -> Result<BTreeMap<K, V>, CodecError> {
	let pairs = Vec::<(K, V)>::decode(input)?;
	if pairs.windows(2).any(|w| w[0].0 >= w[1].0) {
		return Err("MessagingInherentData: map keys must be strictly increasing".into());
	}
	Ok(pairs.into_iter().collect())
}

impl Decode for MessagingInherentData {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		Ok(Self {
			channels: decode_strict_map(input)?,
			registers: decode_strict_map(input)?,
			events: decode_strict_map(input)?,
		})
	}
}

impl DecodeWithMemTracking for MessagingInherentData {}

/// Node side. Places the data under [`INHERENT_IDENTIFIER`], or nothing when empty, so a block that
/// fetched nothing carries no inherent call.
#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for MessagingInherentData {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		if self.is_empty() {
			return Ok(());
		}
		inherent_data.put_data(INHERENT_IDENTIFIER, self)
	}

	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		// An import-time error on this inherent invalidates the block.
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_parachain_primitives::primitives::Id as ParaId;

	fn sample_inherent() -> MessagingInherentData {
		let in_id = InChannelId { sender: ParaId::from(1000), domain: 0, num: 1 };
		let out_id = OutChannelId { recipient: ParaId::from(2000), domain: 0, num: 0 };
		let bcast_id = BroadcastId { sender: ParaId::from(3000), domain: 1, subdomain: 0, num: 7 };
		MessagingInherentData {
			channels: BTreeMap::from([(in_id, alloc::vec![alloc::vec![1, 2, 3]])]),
			registers: BTreeMap::from([(
				out_id,
				LossyRead { frontier: MmrFrontier::new(), payloads: alloc::vec![alloc::vec![4]] },
			)]),
			events: BTreeMap::from([(
				bcast_id,
				LossyRead { frontier: MmrFrontier::new(), payloads: alloc::vec![alloc::vec![5]] },
			)]),
		}
	}

	#[test]
	fn empty_data_places_no_inherent_and_non_empty_round_trips() {
		use sp_inherents::InherentDataProvider;
		let mut inherent_data = sp_inherents::InherentData::new();
		futures::executor::block_on(
			MessagingInherentData::default().provide_inherent_data(&mut inherent_data),
		)
		.expect("providing empty data succeeds");
		assert_eq!(
			inherent_data.get_data::<MessagingInherentData>(&INHERENT_IDENTIFIER).unwrap(),
			None,
			"absent = consumed nothing"
		);

		let data = sample_inherent();
		let mut inherent_data = sp_inherents::InherentData::new();
		futures::executor::block_on(data.provide_inherent_data(&mut inherent_data))
			.expect("providing data succeeds");
		assert_eq!(
			inherent_data.get_data::<MessagingInherentData>(&INHERENT_IDENTIFIER).unwrap(),
			Some(data)
		);
	}

	#[test]
	fn inherent_round_trips_and_rejects_unsorted_or_duplicate_keys() {
		let data = sample_inherent();
		assert_eq!(MessagingInherentData::decode(&mut &data.encode()[..]).unwrap(), data);
		assert_eq!(data.len(), 3);

		let a = InChannelId { sender: ParaId::from(1000), domain: 0, num: 0 };
		let b = InChannelId { sender: ParaId::from(2000), domain: 0, num: 0 };

		// Unsorted channel keys must fail decode.
		let unsorted = (
			alloc::vec![(b, alloc::vec![alloc::vec![1u8]]), (a, alloc::vec![alloc::vec![2u8]])],
			Vec::<(OutChannelId, LossyRead)>::new(),
			Vec::<(BroadcastId, LossyRead)>::new(),
		)
			.encode();
		assert!(MessagingInherentData::decode(&mut &unsorted[..]).is_err());

		// Duplicate channel keys must fail decode.
		let duplicate = (
			alloc::vec![(a, alloc::vec![alloc::vec![1u8]]), (a, alloc::vec![alloc::vec![2u8]])],
			Vec::<(OutChannelId, LossyRead)>::new(),
			Vec::<(BroadcastId, LossyRead)>::new(),
		)
			.encode();
		assert!(MessagingInherentData::decode(&mut &duplicate[..]).is_err());
	}

	#[test]
	fn register_and_event_keys_must_strictly_increase() {
		let o1 = OutChannelId { recipient: ParaId::from(1000), domain: 0, num: 0 };
		let o2 = OutChannelId { recipient: ParaId::from(2000), domain: 0, num: 0 };
		let b1 = BroadcastId { sender: ParaId::from(1000), domain: 0, subdomain: 0, num: 0 };
		let b2 = BroadcastId { sender: ParaId::from(2000), domain: 0, subdomain: 0, num: 0 };
		let read = LossyRead::default();
		let no_channels = Vec::<(InChannelId, Vec<Payload>)>::new;

		for registers in [vec![(o2, read.clone()), (o1, read.clone())], vec![(o1, read.clone()); 2]]
		{
			let bytes = (no_channels(), registers, Vec::<(BroadcastId, LossyRead)>::new()).encode();
			assert!(MessagingInherentData::decode(&mut &bytes[..]).is_err());
		}
		for events in [vec![(b2, read.clone()), (b1, read.clone())], vec![(b1, read.clone()); 2]] {
			let bytes = (no_channels(), Vec::<(OutChannelId, LossyRead)>::new(), events).encode();
			assert!(MessagingInherentData::decode(&mut &bytes[..]).is_err());
		}
	}

	#[test]
	fn inherent_frozen_layout() {
		let empty_frontier = [0u8, 0, 0, 0, 0, 0, 0, 0, 0x00]; // leaf count 0, no peaks
		let read =
			LossyRead { frontier: MmrFrontier::new(), payloads: alloc::vec![alloc::vec![4]] };
		let read_bytes = [&empty_frontier[..], &[0x04, 0x04, 0x04]].concat();
		assert_eq!(read.encode(), read_bytes);

		let data = sample_inherent();
		let expected = [
			// channels: 1 entry, `InChannelId { 1000, 0, 1 }`, one 3-byte payload
			&[0x04, 0xE8, 0x03, 0, 0, 0x00, 0x01, 0x00, 0x04, 0x0C, 1, 2, 3][..],
			// registers: 1 entry, `OutChannelId { 2000, 0, 0 }`, then the read
			&[0x04, 0xD0, 0x07, 0, 0, 0x00, 0x00, 0x00],
			&empty_frontier,
			&[0x04, 0x04, 0x04],
			// events: 1 entry, `BroadcastId { 3000, 1, 0, 7 }`, then the read
			&[0x04, 0xB8, 0x0B, 0, 0, 0x01, 0x00, 0x00, 0x07, 0, 0, 0],
			&empty_frontier,
			&[0x04, 0x04, 0x05],
		]
		.concat();
		assert_eq!(data.encode(), expected);
		assert_eq!(MessagingInherentData::decode(&mut &expected[..]).unwrap(), data);
	}
}
