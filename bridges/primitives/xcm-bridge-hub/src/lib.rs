// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Primitives of the xcm-bridge-hub pallet.

#![cfg_attr(not(feature = "std"), no_std)]

use xcm::latest::prelude::*;

/// Encoded XCM blob. We expect the bridge messages pallet to use this blob type for both inbound
/// and outbound payloads.
pub type XcmAsPlainPayload = sp_std::vec::Vec<u8>;

/// A manager of XCM communication channels between the bridge hub and parent/sibling chains
/// that have opened bridges at this bridge hub.
///
/// We use this interface to suspend and resume channels programmatically to implement backpressure
/// mechanism for bridge queues.
#[allow(clippy::result_unit_err)] // XCM uses `Result<(), ()>` everywhere
pub trait LocalXcmChannelManager {
	// TODO: https://github.com/paritytech/parity-bridges-common/issues/2255
	// check following assumptions. They are important at least for following cases:
	// 1) we now close the associated outbound lane when misbehavior is reported. If we'll keep
	//    handling inbound XCM messages after the `suspend_inbound_channel`, they will be dropped
	// 2) the sender will be able to enqueue message to othe lanes if we won't stop handling inbound
	//    XCM immediately. He even may open additional bridges

	/// Stop handling new incoming XCM messages from given bridge `owner` (parent/sibling chain).
	///
	/// We assume that the channel will be suspended immediately, but we don't mind if inbound
	/// messages will keep piling up here for some time. Once this is communicated to the
	/// `owner` chain (in any form), we expect it to stop sending messages to us and queue
	/// messages at that `owner` chain instead.
	///
	/// This method will be called if we detect a misbehavior in one of bridges, owned by
	/// the `owner`. We expect that:
	///
	/// - no more incoming XCM messages from the `owner` will be processed until further
	///  `resume_inbound_channel` call;
	///
	/// - soon after the call, the channel will switch to the state when incoming messages are
	///   piling up at the sending chain, not at the bridge hub.
	///
	/// This method shall not fail if the channel is already suspended.
	fn suspend_inbound_channel(owner: Location) -> Result<(), ()>;

	/// Start handling incoming messages from from given bridge `owner` (parent/sibling chain)
	/// again.
	///
	/// This method is called when the `owner` tries to resume bridge operations after
	/// resolving "misbehavior" issues. The channel is assumed to be suspended by the previous
	/// `suspend_inbound_channel` call, however we don't check it anywhere.
	///
	/// This method shall not fail if the channel is already resumed.
	fn resume_inbound_channel(owner: Location) -> Result<(), ()>;
}

impl LocalXcmChannelManager for () {
	fn suspend_inbound_channel(_owner: Location) -> Result<(), ()> {
		Ok(())
	}

	fn resume_inbound_channel(_owner: Location) -> Result<(), ()> {
		Err(())
	}
}
