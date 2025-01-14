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

//! Notification related instructions.

/// A message to notify about a new incoming HRMP channel. This message is meant to be sent by
/// the relay-chain to a para.
///
/// - `sender`: The sender in the to-be opened channel. Also, the initiator of the channel
///   opening.
/// - `max_message_size`: The maximum size of a message proposed by the sender.
/// - `max_capacity`: The maximum number of messages that can be queued in the channel.
///
/// Safety: The message should originate directly from the relay-chain.
///
/// Kind: *System Notification*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct HrmpNewChannelOpenRequest {
	#[codec(compact)]
	pub sender: u32,
	#[codec(compact)]
	pub max_message_size: u32,
	#[codec(compact)]
	pub max_capacity: u32,
}

/// A message to notify about that a previously sent open channel request has been accepted by
/// the recipient. That means that the channel will be opened during the next relay-chain
/// session change. This message is meant to be sent by the relay-chain to a para.
///
/// Safety: The message should originate directly from the relay-chain.
///
/// Kind: *System Notification*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct HrmpChannelAccepted {
	// NOTE: We keep this as a structured item to a) keep it consistent with the other Hrmp
	// items; and b) because the field's meaning is not obvious/mentioned from the item name.
	#[codec(compact)]
	pub recipient: u32,
}

/// A message to notify that the other party in an open channel decided to close it. In
/// particular, `initiator` is going to close the channel opened from `sender` to the
/// `recipient`. The close will be enacted at the next relay-chain session change. This message
/// is meant to be sent by the relay-chain to a para.
///
/// Safety: The message should originate directly from the relay-chain.
///
/// Kind: *System Notification*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct HrmpChannelClosing {
	#[codec(compact)]
	pub initiator: u32,
	#[codec(compact)]
	pub sender: u32,
	#[codec(compact)]
	pub recipient: u32,
}
