// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Configuration of the statement protocol

use sp_statement_store::Statement;
use std::time;

/// Interval at which we propagate statements;
pub(crate) const PROPAGATE_TIMEOUT: time::Duration = time::Duration::from_millis(1000);

/// Maximum allowed size for a statement notification.
pub const MAX_STATEMENT_NOTIFICATION_SIZE: u64 = 1024 * 1024;

/// Minimum wire size of a canonically-encoded statement (`Compact(1)` prefix plus the
/// always-present `Expiry` field).
pub const MIN_ENCODED_STATEMENT_SIZE: usize = 10;

/// Upper bound on the heap a single inbound notification may allocate while decoding.
pub const MAX_STATEMENT_DECODE_BYTES: usize = (MAX_STATEMENT_NOTIFICATION_SIZE as usize /
	MIN_ENCODED_STATEMENT_SIZE) *
	core::mem::size_of::<Statement>();

/// Soft limit on encoded initial-sync chunks held in flight across all peers. Since admission is
/// checked before adding the next whole notification, it may be exceeded by less than one
/// maximum-sized notification.
pub const MAX_INITIAL_SYNC_IN_FLIGHT_BYTES: u64 = 16 * MAX_STATEMENT_NOTIFICATION_SIZE;

/// Maximum number of statement validation request we keep at any moment.
pub const MAX_PENDING_STATEMENTS: usize = 2 * 1024 * 1024;

/// Default maximum statements per second before rate limiting kicks in.
pub const DEFAULT_STATEMENTS_PER_SECOND: u32 = 50_000;

/// Burst capacity coefficient for the rate limiter.
pub const STATEMENTS_BURST_COEFFICIENT: u32 = 5;

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;

	#[test]
	fn min_encoded_statement_size_matches_encoder() {
		// If the encoding changes, revisit `MAX_STATEMENT_DECODE_BYTES`.
		assert_eq!(Statement::new().encode().len(), MIN_ENCODED_STATEMENT_SIZE);
	}
}
