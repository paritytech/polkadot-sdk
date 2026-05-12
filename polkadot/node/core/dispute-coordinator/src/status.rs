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

use polkadot_node_clock::Clock;
use polkadot_node_primitives::{dispute_is_inactive, DisputeStatus, Timestamp};
use polkadot_primitives::{CandidateHash, SessionIndex};

/// Get active disputes as iterator, preserving its `DisputeStatus`.
pub fn get_active_with_status(
	recent_disputes: impl Iterator<Item = ((SessionIndex, CandidateHash), DisputeStatus)>,
	now: Timestamp,
) -> impl Iterator<Item = ((SessionIndex, CandidateHash), DisputeStatus)> {
	recent_disputes.filter(move |(_, status)| !dispute_is_inactive(status, &now))
}

/// Read the wall-clock timestamp in seconds since the UNIX epoch via the shared [`Clock`].
///
/// `SystemTime` is notoriously non-monotonic, so our timers might not work exactly as
/// expected. Regardless, disputes are considered active based on an order of minutes, so a few
/// seconds of slippage in either direction shouldn't affect the amount of work the node is
/// doing significantly.
pub fn timestamp_now(clock: &dyn Clock) -> Timestamp {
	(clock.timestamp_millis() / 1000) as Timestamp
}
