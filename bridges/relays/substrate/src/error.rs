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

use futures::channel::mpsc;

#[derive(Debug, derive_more::Display)]
pub enum Error {
	#[display(fmt = "invalid RPC URL: {}", _0)]
	UrlError(String),
	#[display(fmt = "RPC response indicates invalid chain state: {}", _0)]
	InvalidChainState(String),
	#[display(fmt = "could not make RPC call: {}", _0)]
	RPCError(String),
	#[display(fmt = "could not connect to RPC URL: {}", _0)]
	WsConnectionError(String),
	#[display(fmt = "unexpected client event from RPC URL {}: {:?}", _0, _1)]
	UnexpectedClientEvent(String, String),
	#[display(fmt = "serialization error: {}", _0)]
	SerializationError(serde_json::error::Error),
	#[display(fmt = "invalid event received from bridged chain: {}", _0)]
	InvalidBridgeEvent(String),
	#[display(fmt = "error sending over MPSC channel: {}", _0)]
	ChannelError(mpsc::SendError),
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Error::SerializationError(err) => Some(err),
			Error::ChannelError(err) => Some(err),
			_ => None,
		}
	}
}
