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

//! Various implementations for `SendXcm`.

use frame_system::unique;
use parity_scale_codec::Encode;
use sp_std::{marker::PhantomData, result::Result, vec::Vec};
use xcm::prelude::*;
use xcm_executor::{traits::FeeReason, FeesMode};

/// Wrapper router which, if the message does not already end with a `SetTopic` instruction,
/// appends one to the message filled with a universally unique ID. This ID is returned from a
/// successful `deliver`.
///
/// If the message does already end with a `SetTopic` instruction, then it is the responsibility
/// of the code author to ensure that the ID supplied to `SetTopic` is universally unique. Due to
/// this property, consumers of the topic ID must be aware that a user-supplied ID may not be
/// unique.
///
/// This is designed to be at the top-level of any routers, since it will always mutate the
/// passed `message` reference into a `None`. Don't try to combine it within a tuple except as the
/// last element.
pub struct WithUniqueTopic<Inner>(PhantomData<Inner>);
impl<Inner: SendXcm> SendXcm for WithUniqueTopic<Inner> {
	type Ticket = (Inner::Ticket, [u8; 32]);

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let mut message = message.take().ok_or(SendError::MissingArgument)?;
		let unique_id = if let Some(SetTopic(id)) = message.last() {
			*id
		} else {
			let unique_id = unique(&message);
			message.0.push(SetTopic(unique_id));
			unique_id
		};
		let (ticket, assets) = Inner::validate(destination, &mut Some(message))?;
		Ok(((ticket, unique_id), assets))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let (ticket, unique_id) = ticket;
		Inner::deliver(ticket)?;
		Ok(unique_id)
	}
}
impl<Inner: InspectMessageQueues> InspectMessageQueues for WithUniqueTopic<Inner> {
	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		Inner::get_messages()
	}
}

pub trait SourceTopic {
	fn source_topic(entropy: impl Encode) -> XcmHash;
}

impl SourceTopic for () {
	fn source_topic(_: impl Encode) -> XcmHash {
		[0u8; 32]
	}
}

/// Wrapper router which, if the message does not already end with a `SetTopic` instruction,
/// prepends one to the message filled with an ID from `TopicSource`. This ID is returned from a
/// successful `deliver`.
///
/// This is designed to be at the top-level of any routers, since it will always mutate the
/// passed `message` reference into a `None`. Don't try to combine it within a tuple except as the
/// last element.
pub struct WithTopicSource<Inner, TopicSource>(PhantomData<(Inner, TopicSource)>);
impl<Inner: SendXcm, TopicSource: SourceTopic> SendXcm for WithTopicSource<Inner, TopicSource> {
	type Ticket = (Inner::Ticket, [u8; 32]);

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let mut message = message.take().ok_or(SendError::MissingArgument)?;
		let unique_id = if let Some(SetTopic(id)) = message.last() {
			*id
		} else {
			let unique_id = TopicSource::source_topic(&message);
			message.0.push(SetTopic(unique_id));
			unique_id
		};
		let (ticket, assets) = Inner::validate(destination, &mut Some(message))
			.map_err(|_| SendError::NotApplicable)?;
		Ok(((ticket, unique_id), assets))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let (ticket, unique_id) = ticket;
		Inner::deliver(ticket)?;
		Ok(unique_id)
	}
}

/// Trait for a type which ensures all requirements for successful delivery with XCM transport
/// layers.
pub trait EnsureDelivery {
	/// Prepare all requirements for successful `XcmSender: SendXcm` passing (accounts, balances,
	/// channels ...). Returns:
	/// - possible `FeesMode` which is expected to be set to executor
	/// - possible `Assets` which are expected to be subsume to the Holding Register
	fn ensure_successful_delivery(
		origin_ref: &Location,
		dest: &Location,
		fee_reason: FeeReason,
	) -> (Option<FeesMode>, Option<Assets>);
}

/// Tuple implementation for `EnsureDelivery`.
#[impl_trait_for_tuples::impl_for_tuples(30)]
impl EnsureDelivery for Tuple {
	fn ensure_successful_delivery(
		origin_ref: &Location,
		dest: &Location,
		fee_reason: FeeReason,
	) -> (Option<FeesMode>, Option<Assets>) {
		for_tuples!( #(
			// If the implementation returns something, we're done; if not, let others try.
			match Tuple::ensure_successful_delivery(origin_ref, dest, fee_reason.clone()) {
				r @ (Some(_), Some(_)) | r @ (Some(_), None) | r @ (None, Some(_)) => return r,
				(None, None) => (),
			}
		)* );
		// doing nothing
		(None, None)
	}
}

/// Inspects messages in queues.
/// Meant to be used in runtime APIs, not in runtimes.
pub trait InspectMessageQueues {
	/// Get queued messages and their destinations.
	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl InspectMessageQueues for Tuple {
	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		let mut messages = Vec::new();

		for_tuples!( #(
			messages.append(&mut Tuple::get_messages());
		)* );

		messages
	}
}

/// A wrapper router that attempts to *encode* and *decode* passed XCM `message` to ensure that the
/// receiving side will be able to decode, at least with the same XCM version.
///
/// This is designed to be at the top-level of any routers which do the real delivery. While other
/// routers can manipulate the `message`, we cannot access the final XCM due to the generic
/// `Inner::Ticket`. Therefore, this router aims to validate at least the passed `message`.
///
/// NOTE: For use in mock runtimes which don't have the DMP/UMP/HRMP XCM validations.
pub struct EnsureDecodableXcm<Inner>(sp_std::marker::PhantomData<Inner>);
impl<Inner: SendXcm> SendXcm for EnsureDecodableXcm<Inner> {
	type Ticket = Inner::Ticket;

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		if let Some(msg) = message {
			let versioned_xcm = VersionedXcm::<()>::from(msg.clone());
			if versioned_xcm.validate_xcm_nesting().is_err() {
				log::error!(
					target: "xcm::validate_xcm_nesting",
					"EnsureDecodableXcm validate_xcm_nesting error for \nversioned_xcm: {versioned_xcm:?}\nbased on xcm: {msg:?}"
				);
				return Err(SendError::Transport("EnsureDecodableXcm validate_xcm_nesting error"))
			}
		}
		Inner::validate(destination, message)
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		Inner::deliver(ticket)
	}
}
