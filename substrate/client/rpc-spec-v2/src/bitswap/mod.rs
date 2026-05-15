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

//! Substrate bitswap RPC API.
//!
//! Provides methods for retrieving indexed transaction data by CID:
//! - `bitswap_unstable_get` — single CID, returns the chunk or a top-level error. `bitswap_v1_get`
//!   is registered as an alias during the migration to `unstable_`.
//! - `bitswap_unstable_stream` — subscription that emits per-CID outcomes (tagged `streamItem` /
//!   `streamItemError`) as they are looked up, followed by a `streamDone` end-of-stream marker.
//! - `bitswap_unstable_unstream` — cancel an active `bitswap_unstable_stream` subscription.

#[cfg(test)]
mod tests;

pub mod api;
pub mod bitswap;
pub mod error;
pub mod metrics;

pub use api::BitswapApiServer;
pub use bitswap::Bitswap;
pub use metrics::Metrics as BitswapMetrics;
