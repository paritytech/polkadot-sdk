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

//! Traits for handling publish/subscribe operations in XCM.

use xcm::latest::{PublishData, Result as XcmResult};

/// Trait for handling publish/subscribe operations on the relay chain.
pub trait BroadcastHandler {
	/// Handle a publish request from a parachain.
	///
	/// This method should:
	/// 1. Validate the publisher ParaId
	/// 2. Store the data in the appropriate child trie
	/// 3. Update subscription registries
	///
	/// # Arguments
	/// * `publisher` - The ParaId of the publishing parachain
	/// * `data` - The key-value pairs to be published
	fn handle_publish(publisher: u32, data: PublishData) -> XcmResult;
}

/// A no-op implementation of `BroadcastHandler` for testing or stub purposes.
pub struct DoNothingBroadcaster;
impl BroadcastHandler for DoNothingBroadcaster {
	fn handle_publish(_publisher: u32, _data: PublishData) -> XcmResult {
		Ok(())
	}
}

/// Implementation of `BroadcastHandler` for the unit type `()`.
/// This allows runtimes to use `BroadcastHandler = ()` in their XCM executor config.
impl BroadcastHandler for () {
	fn handle_publish(_publisher: u32, _data: PublishData) -> XcmResult {
		// No-op implementation for unit type
		Ok(())
	}
}