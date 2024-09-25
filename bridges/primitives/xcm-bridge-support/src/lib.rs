// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives for the xcm-like bridge.

#![cfg_attr(not(feature = "std"), no_std)]

use xcm::latest::prelude::Location;

/// XCM channel status provider that may report whether it is congested or not.
///
/// By channel, we mean the physical channel that is used to deliver messages to/of one
/// of the bridge queues.
pub trait XcmChannelStatusProvider {
	/// Returns true if the channel with given location is currently congested.
	///
	/// The `with` is guaranteed to be in the same consensus. However, it may point to something
	/// below the chain level - like the contract or pallet instance, for example.
	fn is_congested(with: &Location) -> bool;
}

impl XcmChannelStatusProvider for () {
	fn is_congested(_with: &Location) -> bool {
		false
	}
}
