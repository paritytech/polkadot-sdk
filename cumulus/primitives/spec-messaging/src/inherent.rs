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

//! The node/runtime consumption interface (spec-msg v0.5): what the runtime asks the node to fetch
//! ([`ConsumedStream`], via the `consumed_streams()` runtime API) and what the node delivers in the
//! messaging inherent ([`MessagingInherentData`] / [`ConsumeItem`]).
//!
//! The inherent carries payloads and placement hints. No roots, no proofs. The runtime verifies by
//! recomputation: hash payloads into leaves, append to a frontier, let the candidate's lift bind
//! the endpoint. A wrong payload, `base` or peak set yields an endpoint no lift can bind.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;

use crate::{mmr::MessagePosition, stream::StreamId};

/// Key of the messaging inherent in `InherentData`. Used by the node's provider and the pallet's
/// `ProvideInherent`.
pub const INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = *b"specmsg0";

/// A message payload.
pub type Payload = Vec<u8>;

/// One stream the runtime wants fetched, per source, as returned by `consumed_streams()`. `from` is
/// the fetch cursor: positions `>= from` are wanted. Suspended channels are omitted, which is how
/// collators learn to stop fetching.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum ConsumedStream {
	/// The source's `Channel { us, domain, num }`. Ordered prefix consumption; `from` is the
	/// tracked frontier's leaf count.
	#[codec(index = 0)]
	Channel { domain: u8, num: u16, from: MessagePosition },
	/// The source's `Broadcast { domain, subdomain, num }`. Lossy, latest-wins; `from` is the
	/// highwater plus one.
	#[codec(index = 1)]
	Broadcast { domain: u16, subdomain: u8, num: u32, from: MessagePosition },
}

impl ConsumedStream {
	/// The fetch-cursor view of `stream`. `None` for kinds the runtime never asks to fetch: `Ack`
	/// registers are read out-of-band by the sender, `Private` is chain-defined.
	pub fn project(stream: &StreamId, from: MessagePosition) -> Option<Self> {
		match *stream {
			StreamId::Channel { domain, num, .. } => Some(Self::Channel { domain, num, from }),
			StreamId::Broadcast { domain, subdomain, num } => {
				Some(Self::Broadcast { domain, subdomain, num, from })
			},
			StreamId::Ack { .. } | StreamId::Private { .. } => None,
		}
	}

	/// The `StreamId` this view names on the source's side. A channel's `recipient` is us.
	pub fn stream_id(&self, recipient: ParaId) -> StreamId {
		match *self {
			Self::Channel { domain, num, .. } => StreamId::Channel { recipient, domain, num },
			Self::Broadcast { domain, subdomain, num, .. } => {
				StreamId::Broadcast { domain, subdomain, num }
			},
		}
	}

	/// The fetch cursor: positions `>= from` are wanted.
	pub fn from(&self) -> MessagePosition {
		match *self {
			Self::Channel { from, .. } | Self::Broadcast { from, .. } => from,
		}
	}
}

/// The messaging inherent: this block's consumption, per `(source, stream)`.
///
/// Strict on import. Dispatch is mandatory and first in the block. One invalid item (duplicate or
/// undeclared stream, cap violation, kind/discipline mismatch, `base` at or below the highwater)
/// invalidates the block. Bad items are filtered when building, not tolerated on import: inherent
/// items pay no fees, so tolerance would let a collator pad blocks. It also keeps
/// [`crate::ConsumptionRecord`] simple: every item, no rejected-item bookkeeping. A block that
/// fetched nothing carries no inherent.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MessagingInherentData {
	/// One item per stream consumed this block.
	pub items: Vec<(ParaId, StreamId, ConsumeItem)>,
}

impl MessagingInherentData {
	/// Whether this block consumed nothing. Then no inherent is placed.
	pub fn is_empty(&self) -> bool {
		self.items.is_empty()
	}
}

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

/// One stream's consumption this block. Both arms verify the same way (hash payloads into leaves,
/// append to a frontier, let the lift bind the endpoint) and differ only in where the frontier
/// comes from.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum ConsumeItem {
	/// Prefix discipline: the frontier is the stored inbound frontier; `payloads` are the stream's
	/// next messages, in order. Any deviation in order or count yields an endpoint no lift can
	/// bind.
	#[codec(index = 0)]
	Channel { payloads: Vec<Payload> },
	/// Inclusion discipline (registers, event streams): the frontier arrives in the item.
	/// `start_peaks` is the stream's peak set at `base`, standing in for the frontier a lossy
	/// consumer does not keep. `payloads.len() >= 1`; a register or head read is the
	/// single-payload case at the head. `base` and `start_peaks` are hints: the STF rebuilds the
	/// frontier with `MmrFrontier::from_parts`, and a lie in either yields a root no lift can
	/// bind. Replay is stopped by the highwater rule: `base` must exceed the stream's highwater,
	/// consumption is ascending, and the highwater becomes `base + len - 1`.
	#[codec(index = 1)]
	Events { base: MessagePosition, start_peaks: Vec<Hash>, payloads: Vec<Payload> },
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The inherent is in the block, so its variant indices are consensus-relevant.
	#[test]
	fn variant_indices_are_frozen() {
		let ch = ConsumedStream::Channel { domain: 0, num: 0, from: MessagePosition(0) };
		let bc =
			ConsumedStream::Broadcast { domain: 0, subdomain: 0, num: 0, from: MessagePosition(0) };
		assert_eq!(ch.encode()[0], 0);
		assert_eq!(bc.encode()[0], 1);

		let item_ch = ConsumeItem::Channel { payloads: Vec::new() };
		let item_ev = ConsumeItem::Events {
			base: MessagePosition(0),
			start_peaks: Vec::new(),
			payloads: Vec::new(),
		};
		assert_eq!(item_ch.encode()[0], 0);
		assert_eq!(item_ev.encode()[0], 1);
	}

	#[test]
	fn consumed_stream_projects_and_names_its_stream() {
		let us = ParaId::from(2000);
		let from = MessagePosition(7);
		let channel = StreamId::Channel { recipient: us, domain: 1, num: 2 };
		let broadcast = StreamId::Broadcast { domain: 3, subdomain: 4, num: 5 };
		for stream in [channel, broadcast] {
			let view = ConsumedStream::project(&stream, from).expect("fetchable kinds project");
			assert_eq!(view.stream_id(us), stream, "round-trips through the view");
			assert_eq!(view.from(), from);
		}
		// Never fetched: registers are read out-of-band, private kinds are chain-defined.
		let ack = StreamId::Ack { recipient: us, domain: 0, num: 0 };
		assert_eq!(ConsumedStream::project(&ack, from), None);
		let private =
			StreamId::Private { kind: crate::PrivateKind::new(0x80).unwrap(), body: [0; 7] };
		assert_eq!(ConsumedStream::project(&private, from), None);
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

		let data = MessagingInherentData {
			items: alloc::vec![(
				ParaId::from(1000),
				StreamId::Channel { recipient: ParaId::from(2000), domain: 0, num: 1 },
				ConsumeItem::Channel { payloads: alloc::vec![alloc::vec![1, 2, 3]] },
			)],
		};
		let mut inherent_data = sp_inherents::InherentData::new();
		futures::executor::block_on(data.provide_inherent_data(&mut inherent_data))
			.expect("providing data succeeds");
		assert_eq!(
			inherent_data.get_data::<MessagingInherentData>(&INHERENT_IDENTIFIER).unwrap(),
			Some(data)
		);
	}

	#[test]
	fn inherent_round_trips() {
		let data = MessagingInherentData {
			items: alloc::vec![(
				ParaId::from(1000),
				StreamId::Channel { recipient: ParaId::from(2000), domain: 0, num: 1 },
				ConsumeItem::Channel { payloads: alloc::vec![alloc::vec![1, 2, 3]] },
			)],
		};
		assert_eq!(MessagingInherentData::decode(&mut &data.encode()[..]).unwrap(), data);
	}
}
