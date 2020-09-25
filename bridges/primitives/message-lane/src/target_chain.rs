// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Primitives of message lane module, that are used on the target chain.

use crate::Message;

use frame_support::{weights::Weight, Parameter};
use sp_std::{fmt::Debug, prelude::*};

/// Source chain API. Used by target chain, to verify source chain proofs.
///
/// All implementations of this trait should only work with finalized data that
/// can't change. Wrong implementation may lead to invalid lane states (i.e. lane
/// that's stuck) and/or processing messages without paying fees.
pub trait SourceHeaderChain<Payload, Fee> {
	/// Error type.
	type Error: Debug + Into<&'static str>;

	/// Proof that messages are sent from source chain.
	type MessagesProof: Parameter;

	/// Verify messages proof and return proved messages.
	///
	/// Messages vector is required to be sorted by nonce within each lane. Out-of-order
	/// messages will be rejected.
	fn verify_messages_proof(proof: Self::MessagesProof) -> Result<Vec<Message<Payload, Fee>>, Self::Error>;
}

/// Called when inbound message is received.
pub trait MessageDispatch<Payload, Fee> {
	/// Estimate dispatch weight.
	///
	/// This function must: (1) be instant and (2) return correct upper bound
	/// of dispatch weight.
	fn dispatch_weight(message: &Message<Payload, Fee>) -> Weight;

	/// Called when inbound message is received.
	///
	/// It is up to the implementers of this trait to determine whether the message
	/// is invalid (i.e. improperly encoded, has too large weight, ...) or not.
	fn dispatch(message: Message<Payload, Fee>);
}
