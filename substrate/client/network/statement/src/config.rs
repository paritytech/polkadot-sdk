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

use std::{num::NonZeroUsize, time};

/// Interval at which we propagate statements;
pub(crate) const PROPAGATE_TIMEOUT: time::Duration = time::Duration::from_millis(1000);

/// Maximum allowed size for a statement notification.
pub const MAX_STATEMENT_NOTIFICATION_SIZE: u64 = 1024 * 1024;

/// Soft limit on encoded statement chunks held in flight across all peers, shared by initial-sync
/// and propagation sends. Since admission is checked before adding the next whole notification, it
/// may be exceeded by less than one maximum-sized notification.
pub const MAX_SEND_IN_FLIGHT_BYTES: u64 = 16 * MAX_STATEMENT_NOTIFICATION_SIZE;

/// Part of [`MAX_SEND_IN_FLIGHT_BYTES`] withheld from propagation while initial syncs are
/// pending. Freed budget is otherwise reclaimed synchronously by parked propagations, while
/// sync bursts only check on a timer and would always find the budget full.
pub const INITIAL_SYNC_RESERVED_BYTES: u64 = 4 * MAX_STATEMENT_NOTIFICATION_SIZE;

/// Maximum number of statement hashes queued for propagation to one peer.
/// On overflow the oldest hashes are dropped first.
pub const MAX_PROPAGATION_OUTBOX_LEN: usize = 64 * 1024;

/// Maximum number of statement validation request we keep at any moment.
pub const MAX_PENDING_STATEMENTS: usize = 2 * 1024 * 1024;

/// Default maximum statements per second before rate limiting kicks in.
pub const DEFAULT_STATEMENTS_PER_SECOND: u32 = 50_000;

/// Burst capacity coefficient for the rate limiter.
pub const STATEMENTS_BURST_COEFFICIENT: u32 = 5;

/// Default replication factor (K) for v2 DHT-affinity routing: number of statement-protocol peers
/// responsible for storing a given topic.
pub const DEFAULT_REPLICATION_FACTOR: NonZeroUsize = NonZeroUsize::new(20).expect("20 is non-zero");

/// Default gossip target for v2 DHT-affinity routing: maximum number of connected peers we forward
/// a statement to for a given topic.
pub const DEFAULT_GOSSIP_TARGET: NonZeroUsize = NonZeroUsize::new(3).expect("3 is non-zero");
