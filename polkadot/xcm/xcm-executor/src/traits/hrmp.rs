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

use xcm::latest::Result as XcmResult;

/// Executes logic when a `HrmpNewChannelOpenRequest` XCM notification is received.
pub trait HandleHrmpNewChannelOpenRequest {
	fn handle(sender: u32, max_message_size: u32, max_capacity: u32) -> XcmResult;
}

/// Executes optional logic when a `HrmpChannelAccepted` XCM notification is received.
pub trait HandleHrmpChannelAccepted {
	fn handle(recipient: u32) -> XcmResult;
}

/// Executes optional logic when a `HrmpChannelClosing` XCM notification is received.
pub trait HandleHrmpChannelClosing {
	fn handle(initiator: u32, sender: u32, recipient: u32) -> XcmResult;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HandleHrmpNewChannelOpenRequest for Tuple {
	fn handle(sender: u32, max_message_size: u32, max_capacity: u32) -> XcmResult {
		for_tuples!( #( Tuple::handle(sender, max_message_size, max_capacity)?; )* );
		Ok(())
	}
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HandleHrmpChannelAccepted for Tuple {
	fn handle(recipient: u32) -> XcmResult {
		for_tuples!( #( Tuple::handle(recipient)?; )* );
		Ok(())
	}
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HandleHrmpChannelClosing for Tuple {
	fn handle(initiator: u32, sender: u32, recipient: u32) -> XcmResult {
		for_tuples!( #( Tuple::handle(initiator, sender, recipient)?; )* );
		Ok(())
	}
}
