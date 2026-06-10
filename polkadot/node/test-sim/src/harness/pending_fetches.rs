// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Side table of `oneshot::Sender`s the harness extracted from outgoing
//! `NetworkBridgeTxMessage::SendRequests` messages.
//!
//! When the subsystem fires a fetch, the classifier hands the embedded
//! `OutgoingRequest::pending_response` to this table. Each entry is keyed by an opaque
//! [`RequestId`] which surfaces in the corresponding `Effect::SendRequest`. Tests then drive
//! the response via `Sim::respond_fetch(request_id, payload)`.
//!
//! [`RequestId`]: crate::contract::RequestId

use crate::contract::RequestId;
use polkadot_node_network_protocol::request_response::ResponseSender;
use sc_network::ProtocolName;
use std::{collections::HashMap, time::Instant};

/// A pending fetch: the response sender plus the simulated-time deadline at which the network
/// layer would force-detect a timeout if no response has arrived.
struct Entry {
	sender: ResponseSender,
	deadline: Instant,
}

/// Side table of pending request response senders keyed by [`RequestId`].
///
/// Each entry also carries the request's timeout deadline. The harness models the network
/// layer, so it owns the request timeout too: [`Self::drain_timed_out`] drops the senders of
/// fetches whose deadline has passed, which makes the subsystem's awaiting `oneshot` resolve
/// with `Canceled` — exactly what a real network-level request timeout looks like to the
/// subsystem.
#[derive(Default)]
pub struct PendingFetches {
	next_id: u64,
	by_id: HashMap<RequestId, Entry>,
}

impl PendingFetches {
	/// Empty pending-fetches table.
	pub fn new() -> Self {
		Self::default()
	}

	/// Allocate a fresh [`RequestId`] and store the response sender together with the
	/// simulated-time `deadline` after which it is considered timed out.
	pub fn register(&mut self, sender: ResponseSender, deadline: Instant) -> RequestId {
		let id = RequestId(self.next_id);
		self.next_id += 1;
		self.by_id.insert(id, Entry { sender, deadline });
		id
	}

	/// Take ownership of the response sender for `id`. Returns `None` if no such pending
	/// fetch exists (already responded, timed out, or unknown id).
	pub fn take(&mut self, id: RequestId) -> Option<ResponseSender> {
		self.by_id.remove(&id).map(|e| e.sender)
	}

	/// Earliest deadline across all outstanding fetches, if any. The sim's clock-stepping
	/// loops treat this as a scheduled event alongside executor timer wakeups, so they stop
	/// *at* the timeout instant rather than stepping past it.
	pub fn next_deadline(&self) -> Option<Instant> {
		self.by_id.values().map(|e| e.deadline).min()
	}

	/// Drop every fetch whose `deadline <= now`, returning how many were dropped. Dropping the
	/// sender resolves the subsystem's awaiting receiver with `Canceled`, modelling a
	/// network-level request timeout.
	pub fn drain_timed_out(&mut self, now: Instant) -> usize {
		let before = self.by_id.len();
		// Dropping each removed `Entry` (and its `sender`) cancels the subsystem's receiver.
		self.by_id.retain(|_, e| e.deadline > now);
		before - self.by_id.len()
	}

	/// Number of pending fetches currently outstanding.
	pub fn len(&self) -> usize {
		self.by_id.len()
	}

	/// Whether there are any pending fetches.
	pub fn is_empty(&self) -> bool {
		self.by_id.is_empty()
	}
}

/// Convenience: raw response shape the subsystem expects on the oneshot.
pub type RawResponse = std::result::Result<(Vec<u8>, ProtocolName), sc_network::RequestFailure>;
