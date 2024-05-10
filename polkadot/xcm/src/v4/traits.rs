// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Cross-Consensus Message format data structures.

pub use crate::v3::{Error, Result, SendError, XcmHash};
use core::result;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use sp_weights::Weight;

use super::*;

/// Outcome of an XCM execution.
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Outcome {
	/// Execution completed successfully; given weight was used.
	Complete { used: Weight },
	/// Execution started, but did not complete successfully due to the given error; given weight
	/// was used.
	Incomplete { used: Weight, error: Error },
	/// Execution did not start due to the given error.
	Error { error: Error },
}

impl Outcome {
	pub fn ensure_complete(self) -> Result {
		match self {
			Outcome::Complete { .. } => Ok(()),
			Outcome::Incomplete { error, .. } => Err(error),
			Outcome::Error { error, .. } => Err(error),
		}
	}
	pub fn ensure_execution(self) -> result::Result<Weight, Error> {
		match self {
			Outcome::Complete { used, .. } => Ok(used),
			Outcome::Incomplete { used, .. } => Ok(used),
			Outcome::Error { error, .. } => Err(error),
		}
	}
	/// How much weight was used by the XCM execution attempt.
	pub fn weight_used(&self) -> Weight {
		match self {
			Outcome::Complete { used, .. } => *used,
			Outcome::Incomplete { used, .. } => *used,
			Outcome::Error { .. } => Weight::zero(),
		}
	}
}

impl From<Error> for Outcome {
	fn from(error: Error) -> Self {
		Self::Error { error }
	}
}

pub trait PreparedMessage {
	fn weight_of(&self) -> Weight;
}

/// Type of XCM message executor.
pub trait ExecuteXcm<Call> {
	type Prepared: PreparedMessage;
	fn prepare(message: Xcm<Call>) -> result::Result<Self::Prepared, Xcm<Call>>;
	fn execute(
		origin: impl Into<Location>,
		pre: Self::Prepared,
		id: &mut XcmHash,
		weight_credit: Weight,
	) -> Outcome;
	fn prepare_and_execute(
		origin: impl Into<Location>,
		message: Xcm<Call>,
		id: &mut XcmHash,
		weight_limit: Weight,
		weight_credit: Weight,
	) -> Outcome {
		let pre = match Self::prepare(message) {
			Ok(x) => x,
			Err(_) => return Outcome::Error { error: Error::WeightNotComputable },
		};
		let xcm_weight = pre.weight_of();
		if xcm_weight.any_gt(weight_limit) {
			return Outcome::Error { error: Error::WeightLimitReached(xcm_weight) }
		}
		Self::execute(origin, pre, id, weight_credit)
	}

	/// Deduct some `fees` to the sovereign account of the given `location` and place them as per
	/// the convention for fees.
	fn charge_fees(location: impl Into<Location>, fees: Assets) -> Result;
}

pub enum Weightless {}
impl PreparedMessage for Weightless {
	fn weight_of(&self) -> Weight {
		unreachable!()
	}
}

impl<C> ExecuteXcm<C> for () {
	type Prepared = Weightless;
	fn prepare(message: Xcm<C>) -> result::Result<Self::Prepared, Xcm<C>> {
		Err(message)
	}
	fn execute(_: impl Into<Location>, _: Self::Prepared, _: &mut XcmHash, _: Weight) -> Outcome {
		unreachable!()
	}
	fn charge_fees(_location: impl Into<Location>, _fees: Assets) -> Result {
		Err(Error::Unimplemented)
	}
}

pub trait Reanchorable: Sized {
	/// Type to return in case of an error.
	type Error: Debug;

	/// Mutate `self` so that it represents the same location from the point of view of `target`.
	/// The context of `self` is provided as `context`.
	///
	/// Does not modify `self` in case of overflow.
	fn reanchor(
		&mut self,
		target: &Location,
		context: &InteriorLocation,
	) -> core::result::Result<(), ()>;

	/// Consume `self` and return a new value representing the same location from the point of view
	/// of `target`. The context of `self` is provided as `context`.
	///
	/// Returns the original `self` in case of overflow.
	fn reanchored(
		self,
		target: &Location,
		context: &InteriorLocation,
	) -> core::result::Result<Self, Self::Error>;
}

/// Result value when attempting to send an XCM message.
pub type SendResult<T> = result::Result<(T, Assets), SendError>;

/// Utility for sending an XCM message to a given location.
///
/// These can be amalgamated in tuples to form sophisticated routing systems. In tuple format, each
/// router might return `NotApplicable` to pass the execution to the next sender item. Note that
/// each `NotApplicable` might alter the destination and the XCM message for to the next router.
///
/// # Example
/// ```rust
/// # use parity_scale_codec::Encode;
/// # use staging_xcm::v4::{prelude::*, Weight};
/// # use staging_xcm::VersionedXcm;
/// # use std::convert::Infallible;
///
/// /// A sender that only passes the message through and does nothing.
/// struct Sender1;
/// impl SendXcm for Sender1 {
///     type Ticket = Infallible;
///     fn validate(_: &mut Option<Location>, _: &mut Option<Xcm<()>>) -> SendResult<Infallible> {
///         Err(SendError::NotApplicable)
///     }
///     fn deliver(_: Infallible) -> Result<XcmHash, SendError> {
///         unreachable!()
///     }
/// }
///
/// /// A sender that accepts a message that has two junctions, otherwise stops the routing.
/// struct Sender2;
/// impl SendXcm for Sender2 {
///     type Ticket = ();
///     fn validate(destination: &mut Option<Location>, message: &mut Option<Xcm<()>>) -> SendResult<()> {
///         match destination.as_ref().ok_or(SendError::MissingArgument)?.unpack() {
///             (0, [j1, j2]) => Ok(((), Assets::new())),
///             _ => Err(SendError::Unroutable),
///         }
///     }
///     fn deliver(_: ()) -> Result<XcmHash, SendError> {
///         Ok([0; 32])
///     }
/// }
///
/// /// A sender that accepts a message from a parent, passing through otherwise.
/// struct Sender3;
/// impl SendXcm for Sender3 {
///     type Ticket = ();
///     fn validate(destination: &mut Option<Location>, message: &mut Option<Xcm<()>>) -> SendResult<()> {
///         match destination.as_ref().ok_or(SendError::MissingArgument)?.unpack() {
///             (1, []) => Ok(((), Assets::new())),
///             _ => Err(SendError::NotApplicable),
///         }
///     }
///     fn deliver(_: ()) -> Result<XcmHash, SendError> {
///         Ok([0; 32])
///     }
/// }
///
/// // A call to send via XCM. We don't really care about this.
/// # fn main() {
/// let call: Vec<u8> = ().encode();
/// let message = Xcm(vec![Instruction::Transact {
///     origin_kind: OriginKind::Superuser,
///     require_weight_at_most: Weight::zero(),
///     call: call.into(),
/// }]);
/// let message_hash = message.using_encoded(sp_io::hashing::blake2_256);
///
/// // Sender2 will block this.
/// assert!(send_xcm::<(Sender1, Sender2, Sender3)>(Parent.into(), message.clone()).is_err());
///
/// // Sender3 will catch this.
/// assert!(send_xcm::<(Sender1, Sender3)>(Parent.into(), message.clone()).is_ok());
/// # }
/// ```
pub trait SendXcm {
	/// Intermediate value which connects the two phases of the send operation.
	type Ticket;

	/// Check whether the given `_message` is deliverable to the given `_destination` and if so
	/// determine the cost which will be paid by this chain to do so, returning a `Validated` token
	/// which can be used to enact delivery.
	///
	/// The `destination` and `message` must be `Some` (or else an error will be returned) and they
	/// may only be consumed if the `Err` is not `NotApplicable`.
	///
	/// If it is not a destination which can be reached with this type but possibly could by others,
	/// then this *MUST* return `NotApplicable`. Any other error will cause the tuple
	/// implementation to exit early without trying other type fields.
	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket>;

	/// Actually carry out the delivery operation for a previously validated message sending.
	fn deliver(ticket: Self::Ticket) -> result::Result<XcmHash, SendError>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl SendXcm for Tuple {
	for_tuples! { type Ticket = (#( Option<Tuple::Ticket> ),* ); }

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let mut maybe_cost: Option<Assets> = None;
		let one_ticket: Self::Ticket = (for_tuples! { #(
			if maybe_cost.is_some() {
				None
			} else {
				match Tuple::validate(destination, message) {
					Err(SendError::NotApplicable) => None,
					Err(e) => { return Err(e) },
					Ok((v, c)) => {
						maybe_cost = Some(c);
						Some(v)
					},
				}
			}
		),* });
		if let Some(cost) = maybe_cost {
			Ok((one_ticket, cost))
		} else {
			Err(SendError::NotApplicable)
		}
	}

	fn deliver(one_ticket: Self::Ticket) -> result::Result<XcmHash, SendError> {
		for_tuples!( #(
			if let Some(validated) = one_ticket.Tuple {
				return Tuple::deliver(validated);
			}
		)* );
		Err(SendError::Unroutable)
	}
}

/// Convenience function for using a `SendXcm` implementation. Just interprets the `dest` and wraps
/// both in `Some` before passing them as as mutable references into `T::send_xcm`.
pub fn validate_send<T: SendXcm>(dest: Location, msg: Xcm<()>) -> SendResult<T::Ticket> {
	T::validate(&mut Some(dest), &mut Some(msg))
}

/// Convenience function for using a `SendXcm` implementation. Just interprets the `dest` and wraps
/// both in `Some` before passing them as as mutable references into `T::send_xcm`.
///
/// Returns either `Ok` with the price of the delivery, or `Err` with the reason why the message
/// could not be sent.
///
/// Generally you'll want to validate and get the price first to ensure that the sender can pay it
/// before actually doing the delivery.
pub fn send_xcm<T: SendXcm>(
	dest: Location,
	msg: Xcm<()>,
) -> result::Result<(XcmHash, Assets), SendError> {
	let (ticket, price) = T::validate(&mut Some(dest), &mut Some(msg))?;
	let hash = T::deliver(ticket)?;
	Ok((hash, price))
}
