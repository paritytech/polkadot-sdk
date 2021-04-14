// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Helper datatypes for cumulus. This includes the [`ParentAsUmp`] routing type which will route
//! messages into an [`UpwardMessageSender`] if the destination is `Parent`.

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{marker::PhantomData, convert::TryFrom};
use codec::{Encode, Decode};
use cumulus_primitives_core::{UpwardMessageSender, DownwardMessageHandler, InboundDownwardMessage};
use xcm::{VersionedXcm, v0::{Xcm, MultiLocation, Junction, SendXcm, Error as XcmError, ExecuteXcm}};
use frame_support::{traits::Get, dispatch::Weight};

/// Xcm router which recognises the `Parent` destination and handles it by sending the message into
/// the given UMP `UpwardMessageSender` implementation. Thus this essentially adapts an
/// `UpwardMessageSender` trait impl into a `SendXcm` trait impl.
///
/// NOTE: This is a pretty dumb "just send it" router; we will probably want to introduce queuing
/// to UMP eventually and when we do, the pallet which implements the queuing will be responsible
/// for the `SendXcm` implementation.
pub struct ParentAsUmp<T>(PhantomData<T>);
impl<T: UpwardMessageSender> SendXcm for ParentAsUmp<T> {
	fn send_xcm(dest: MultiLocation, msg: Xcm<()>) -> Result<(), XcmError> {
		match &dest {
			// An upward message for the relay chain.
			MultiLocation::X1(Junction::Parent) => {
				let data = VersionedXcm::<()>::from(msg).encode();

				T::send_upward_message(data)
					.map_err(|e| XcmError::SendFailed(e.into()))?;

				Ok(())
			}
			// Anything else is unhandled. This includes a message this is meant for us.
			_ => Err(XcmError::CannotReachDestination(dest, msg)),
		}
	}
}

/// For an incoming downward message, this just adapts an XCM executor and executes DMP messages
/// immediately up until some `MaxWeight` at which point it errors. Their origin is asserted to be
/// the Parent location.
pub struct UnqueuedDmpAsParent<MaxWeight, XcmExecutor, Call>(
	PhantomData<(MaxWeight, XcmExecutor, Call)>
);
impl<
	MaxWeight: Get<Weight>,
	XcmExecutor: ExecuteXcm<Call>,
	Call,
> DownwardMessageHandler for UnqueuedDmpAsParent<MaxWeight, XcmExecutor, Call> {
	fn handle_downward_message(msg: InboundDownwardMessage) -> Weight {
		let msg = VersionedXcm::<Call>::decode(&mut &msg.msg[..])
			.map(Xcm::<Call>::try_from);
		match msg {
			Ok(Ok(x)) => {
				let weight_limit = MaxWeight::get();
				XcmExecutor::execute_xcm(Junction::Parent.into(), x, weight_limit).weight_used()
			}
			Ok(Err(..)) => 0,
			Err(..) => 0,
		}
	}
}
