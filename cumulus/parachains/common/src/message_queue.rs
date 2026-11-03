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

//! Helpers to deal with configuring the message queue in the runtime.

use core::marker::PhantomData;
use cumulus_primitives_core::{AggregateMessageOrigin, InChannelId, ParaId};
use frame_support::traits::{QueueFootprint, QueuePausedQuery};
use pallet_message_queue::OnQueueChanged;

/// Narrow the scope of the `Inner` query from `AggregateMessageOrigin` to `ParaId`.
///
/// All non-`Sibling` variants will be ignored.
pub struct NarrowOriginToSibling<Inner>(PhantomData<Inner>);
impl<Inner: QueuePausedQuery<ParaId>> QueuePausedQuery<AggregateMessageOrigin>
	for NarrowOriginToSibling<Inner>
{
	fn is_paused(origin: &AggregateMessageOrigin) -> bool {
		match origin {
			AggregateMessageOrigin::Sibling(id) => Inner::is_paused(id),
			_ => false,
		}
	}
}

impl<Inner: OnQueueChanged<ParaId>> OnQueueChanged<AggregateMessageOrigin>
	for NarrowOriginToSibling<Inner>
{
	fn on_queue_changed(origin: AggregateMessageOrigin, fp: QueueFootprint) {
		if let AggregateMessageOrigin::Sibling(id) = origin {
			Inner::on_queue_changed(id, fp)
		}
	}
}

/// Narrow the scope of the `Inner` query from `AggregateMessageOrigin` to `InChannelId`.
///
/// All non-`SpecMsg` variants will be ignored. It implements no `OnQueueChanged`: HRMP
/// backpressure never pauses spec-msg.
pub struct NarrowOriginToSpecMsg<Inner>(PhantomData<Inner>);
impl<Inner: QueuePausedQuery<InChannelId>> QueuePausedQuery<AggregateMessageOrigin>
	for NarrowOriginToSpecMsg<Inner>
{
	fn is_paused(origin: &AggregateMessageOrigin) -> bool {
		match origin {
			AggregateMessageOrigin::SpecMsg(channel) => Inner::is_paused(channel),
			_ => false,
		}
	}
}

/// Ask the `Inner` query about the sender of a `SpecMsg` queue, so that a global XCM suspension
/// (such as `XcmpQueue`'s) also covers the spec-msg queues.
///
/// All non-`SpecMsg` variants will be ignored.
pub struct SpecMsgToSender<Inner>(PhantomData<Inner>);
impl<Inner: QueuePausedQuery<ParaId>> QueuePausedQuery<AggregateMessageOrigin>
	for SpecMsgToSender<Inner>
{
	fn is_paused(origin: &AggregateMessageOrigin) -> bool {
		match origin {
			AggregateMessageOrigin::SpecMsg(channel) => Inner::is_paused(&channel.sender),
			_ => false,
		}
	}
}

/// Convert a sibling `ParaId` to an `AggregateMessageOrigin`.
pub struct ParaIdToSibling;
impl sp_runtime::traits::Convert<ParaId, AggregateMessageOrigin> for ParaIdToSibling {
	fn convert(para_id: ParaId) -> AggregateMessageOrigin {
		AggregateMessageOrigin::Sibling(para_id)
	}
}

/// Convert an inbound spec-msg channel to the `AggregateMessageOrigin` of its queue.
///
/// Spec-msg gets its own queue per channel, separate from the HRMP one: HRMP backpressure
/// ([`NarrowOriginToSibling`]) never pauses it.
pub struct ChannelToSpecMsg;
impl sp_runtime::traits::Convert<InChannelId, AggregateMessageOrigin> for ChannelToSpecMsg {
	fn convert(channel: InChannelId) -> AggregateMessageOrigin {
		AggregateMessageOrigin::SpecMsg(channel)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	struct PausedNum7;
	impl QueuePausedQuery<InChannelId> for PausedNum7 {
		fn is_paused(channel: &InChannelId) -> bool {
			channel.num == 7
		}
	}

	struct PausedPara7;
	impl QueuePausedQuery<ParaId> for PausedPara7 {
		fn is_paused(id: &ParaId) -> bool {
			*id == ParaId::from(7)
		}
	}

	fn channel(sender: u32, num: u16) -> AggregateMessageOrigin {
		AggregateMessageOrigin::SpecMsg(InChannelId { sender: sender.into(), domain: 0, num })
	}

	#[test]
	fn narrow_origin_to_spec_msg_maps_only_spec_msg() {
		type Q = NarrowOriginToSpecMsg<PausedNum7>;
		assert!(Q::is_paused(&channel(1, 7)));
		assert!(!Q::is_paused(&channel(7, 1)));
		assert!(!Q::is_paused(&AggregateMessageOrigin::Sibling(7.into())));
		assert!(!Q::is_paused(&AggregateMessageOrigin::Parent));
	}

	#[test]
	fn spec_msg_to_sender_asks_about_the_sender() {
		type Q = SpecMsgToSender<PausedPara7>;
		assert!(Q::is_paused(&channel(7, 1)));
		assert!(!Q::is_paused(&channel(1, 7)));
		assert!(!Q::is_paused(&AggregateMessageOrigin::Sibling(7.into())));
		assert!(!Q::is_paused(&AggregateMessageOrigin::Parent));
	}
}
