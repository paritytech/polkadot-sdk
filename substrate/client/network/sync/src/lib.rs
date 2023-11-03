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

//! Blockchain syncing implementation in Substrate.

pub use service::chain_sync::SyncingService;
pub use types::{SyncEvent, SyncEventStream, SyncState, SyncStatus, SyncStatusProvider};

mod block_announce_validator;
mod chain_sync;
mod extra_requests;
mod futures_stream;
mod pending_responses;
mod request_metrics;
mod schema;
mod types;

pub mod block_relay_protocol;
pub mod block_request_handler;
pub mod blocks;
pub mod engine;
pub mod mock;
pub mod service;
pub mod state;
pub mod state_request_handler;
pub mod warp;
pub mod warp_request_handler;
