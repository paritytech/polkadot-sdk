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

use xcm::{
	latest::{Location, SendError, Xcm, XcmHash},
	prelude::XcmError,
};

/// Defines the event emitter for the XCM executor.
/// This trait allows implementations to emit events related to XCM handling, including successful
/// sends and failure cases.
pub trait EventEmitter {
	/// Emits an event when an XCM is successfully sent.
	///
	/// # Parameters
	/// - `origin`: The origin location of the XCM.
	/// - `destination`: The target location where the message is sent.
	/// - `message`: `Some(Xcm)` for `pallet_xcm::Event::Sent`, `None` for other events to reduce
	///   storage.
	/// - `message_id`: A unique identifier for the XCM.
	fn emit_sent_event(
		origin: Location,
		destination: Location,
		message: Option<Xcm<()>>,
		message_id: XcmHash,
	);

	/// Emits an event when an XCM fails to send.
	///
	/// # Parameters
	/// - `origin`: The origin location of the XCM.
	/// - `destination`: The intended target location.
	/// - `error`: The error encountered while sending.
	/// - `message_id`: The unique identifier for the failed message.
	fn emit_send_failure_event(
		origin: Location,
		destination: Location,
		error: SendError,
		message_id: XcmHash,
	);

	/// Emits an event when an XCM fails to process.
	///
	/// # Parameters
	/// - `origin`: The origin location of the message.
	/// - `error`: The error encountered while processing.
	/// - `message_id`: The unique identifier for the failed message.
	fn emit_process_failure_event(origin: Location, error: XcmError, message_id: XcmHash);
}

/// A no-op implementation of `EventEmitter` for unit type `()`.
/// This can be used when event emission is not required.
impl EventEmitter for () {
	fn emit_sent_event(
		_origin: Location,
		_destination: Location,
		_message: Option<Xcm<()>>,
		_message_id: XcmHash,
	) {
	}

	fn emit_send_failure_event(
		_origin: Location,
		_destination: Location,
		_error: SendError,
		_message_id: XcmHash,
	) {
	}

	fn emit_process_failure_event(_origin: Location, _error: XcmError, _message_id: XcmHash) {}
}
