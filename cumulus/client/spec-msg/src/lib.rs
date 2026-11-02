// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Speculative Messaging node side: the off-chain half that moves payloads between collators.
//!
//! - [`Archive`]: the own chain's sends, served under the root a requester names.
//! - [`run_archiver`]: keeps the archive current through reorgs and retention.
//! - [`fetch_protocol_config`] and [`run_request_handler`]: the `/spec-msg/fetch/1` protocol.
//! - [`fetch()`]: the requester, which verifies each response before accepting it.
//! - [`run_watermark_reader`]: reads recipients' registers under finalized roots, so the archive
//!   prunes a channel only below what the recipient confirmed.
//!
//! Design: paritytech/polkadot-sdk#12659.

#![warn(missing_docs)]

pub mod archive;
pub mod fetch;
pub mod protocol;
pub mod relay;
pub mod store;
pub mod worker;

pub use archive::{Archive, ArchiveError, Tip};
pub use fetch::{fetch, NetworkTransport, RequestError, Transport};
pub use protocol::{fetch_protocol_config, run_request_handler, PROTOCOL_NAME};
pub use relay::{
	newest_included_provides, relay_has_api, FinalizedRoots, RelayFinalizedRoots,
	NEWEST_INCLUDED_PROVIDES, NEWEST_INCLUDED_PROVIDES_VERSION,
};
pub use store::{ArchiveStore, AuxArchiveStore, Batch, MemoryStore};
pub use worker::{run_archiver, run_watermark_reader, SERVING_HORIZON};

/// Log target of this crate.
pub const LOG_TARGET: &str = "spec-msg";

#[cfg(test)]
mod tests;
