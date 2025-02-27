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

use xcm::latest::prelude::*;

/// Defines the event emitter for the XCM executor.
pub trait EventEmitter {
    fn emit_sent_event(
        origin: Location,
        destination: Location,
        message_id: XcmHash,
    );

    fn emit_send_failure_event(
        origin: Location,
        destination: Location,
        error: SendError,
        message_id: XcmHash,
    );
    fn emit_process_failure_event(
        origin: Location,
        error: XcmError,
        message_id: XcmHash,
    );
}

impl EventEmitter for () {
    fn emit_sent_event(
        _origin: Location,
        _destination: Location,
        _message: Xcm<()>,
        _message_id: XcmHash,
    ) {}

    fn emit_sent_failure_event(
        _origin: Location,
        _destination: Location,
        _error: SendError,
        _message_id: XcmHash,
    ) {}

    fn emit_process_failure_event(
        _origin: Location,
        _error: XcmError,
        _message_id: XcmHash
    ) {}
}