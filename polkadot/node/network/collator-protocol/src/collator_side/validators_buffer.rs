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

//! Validator groups buffer for connection managements.
//!
//! Solves 2 problems:
//! 	1. A collator may want to stay connected to multiple groups on rotation boundaries.
//! 	2. It's important to disconnect from validator when there're no collations to be fetched.
//!
//! We keep a simple FIFO buffer of N validator groups and a bitvec for each advertisement,
//! 1 indicating we want to be connected to i-th validator in a buffer, 0 otherwise.
//!
//! The bit is set to 1 for the whole **group** whenever it's inserted into the buffer. Given a
//! relay parent, one can reset a bit back to 0 for particular **validator**. For example, if a
//! collation was fetched or some timeout has been hit.
//!
//! The bitwise OR over known advertisements gives us validators indices for connection request.

use std::{
	future::Future,
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};

use futures::FutureExt;

use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::CandidateHash;

/// A timeout for resetting validators' interests in collations.
pub const RESET_INTEREST_TIMEOUT: Duration = Duration::from_secs(6);

/// A future that returns a candidate hash along with validator discovery
/// keys once a timeout hit.
///
/// If a validator doesn't manage to fetch a collation within this timeout
/// we should reset its interest in this advertisement in a buffer. For example,
/// when the PoV was already requested from another peer.
pub struct ResetInterestTimeout {
	fut: futures_timer::Delay,
	candidate_hash: CandidateHash,
	peer_id: PeerId,
}

impl ResetInterestTimeout {
	/// Returns new `ResetInterestTimeout` that resolves after given timeout.
	pub fn new(candidate_hash: CandidateHash, peer_id: PeerId, delay: Duration) -> Self {
		Self { fut: futures_timer::Delay::new(delay), candidate_hash, peer_id }
	}
}

impl Future for ResetInterestTimeout {
	type Output = (CandidateHash, PeerId);

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		self.fut.poll_unpin(cx).map(|_| (self.candidate_hash, self.peer_id))
	}
}
