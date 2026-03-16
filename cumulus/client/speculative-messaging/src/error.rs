// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Error types for speculative message networking.

use polkadot_parachain_primitives::primitives::Id as ParaId;

/// Errors that can occur during speculative message exchange.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// No relay chain peer is registered for the given parachain.
	#[error("No relay peer registered for para {0:?}")]
	NoPeerForPara(ParaId),

	/// Failed to send a request to a peer.
	#[error("Failed to send message to peer: {0}")]
	SendFailed(String),

	/// Failed to receive a response from a peer.
	#[error("Failed to receive response: {0}")]
	ReceiveFailed(String),

	/// The remote peer rejected the message batch.
	#[error("Peer rejected message: {0}")]
	Rejected(String),

	/// The incoming message batch failed validation.
	#[error("Invalid message batch: {0}")]
	InvalidBatch(String),

	/// SCALE codec error.
	#[error("Codec error: {0}")]
	Codec(#[from] codec::Error),
}
