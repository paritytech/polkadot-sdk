// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <https://www.gnu.org/licenses/>.

//! User-facing API for submitting Bitswap requests.

use super::{is_cid_supported, Cid};

use tokio::sync::mpsc;

/// Service-level Bitswap errors.
///
/// Returned synchronously from [`BitswapHandle::request_stream`] for admission-time
/// failures, and appearing at most once inside the returned stream (`Overloaded` or
/// `ServiceClosed`).
#[derive(Debug, thiserror::Error)]
pub enum BitswapError {
	/// The Bitswap service task has shut down.
	#[error("Bitswap service is closed")]
	ServiceClosed,
	/// A CID in the wantlist is unsupported (bad version, bad multihash code, or bad digest
	/// size).
	#[error("invalid CID for Bitswap: {cid}")]
	InvalidCid {
		/// The offending CID.
		cid: Cid,
	},
	/// The service cannot accept the request: the command channel is full, or too many
	/// concurrent requests want the same CID.
	#[error("Bitswap service is overloaded")]
	Overloaded,
}

/// Item carried on the receiver returned by [`BitswapHandle::request_stream`]: the
/// hash-verified bytes for one requested CID, or a terminal service error.
pub type FetchItem = Result<(Cid, Vec<u8>), BitswapError>;

/// User-facing handle to the Bitswap service.
///
/// Cheap to clone.
#[derive(Debug, Clone)]
pub struct BitswapHandle {
	cmd_tx: mpsc::Sender<BitswapCommand>,
}

impl BitswapHandle {
	/// Construct a new handle around an existing command sender.
	pub(crate) fn new(cmd_tx: mpsc::Sender<BitswapCommand>) -> Self {
		Self { cmd_tx }
	}

	/// Submit a wantlist. Returns a receiver that yields `Ok((cid, bytes))` with
	/// hash-verified bytes for each requested CID, in the order they resolve.
	///
	/// The stream closes once every CID has been delivered. Unresolved CIDs are retried
	/// until the receiver is dropped.
	///
	/// There is no per-call CID cap.
	///
	/// `Err(BitswapError::ServiceClosed)` is yielded once, as the final item, if the
	/// service shuts down mid-request. `Err(BitswapError::Overloaded)` is yielded once as
	/// the only item if too many concurrent requests want one of the CIDs.
	///
	/// Returns a synchronous `BitswapError` for admission-time failures (`ServiceClosed`,
	/// `InvalidCid`, or `Overloaded` when the command channel is full). An empty `cids`
	/// slice returns an immediately-closed receiver, not an error.
	pub fn request_stream(
		&self,
		cids: Vec<Cid>,
	) -> Result<mpsc::Receiver<FetchItem>, BitswapError> {
		if cids.is_empty() {
			let (_tx, rx) = mpsc::channel(1);
			return Ok(rx);
		}

		for cid in &cids {
			if !is_cid_supported(cid) {
				return Err(BitswapError::InvalidCid { cid: *cid });
			}
		}

		// `cids.len() + 1` reserves one slot for a possible terminal `Err` item
		// (`Overloaded` or `ServiceClosed`), so the actor's `try_send` for outcomes never
		// fails for well-behaved callers.
		let (sink, rx) = mpsc::channel(cids.len() + 1);

		self.cmd_tx.try_send(BitswapCommand::RequestStream { cids, sink }).map_err(
			|e| match e {
				mpsc::error::TrySendError::Full(_) => BitswapError::Overloaded,
				mpsc::error::TrySendError::Closed(_) => BitswapError::ServiceClosed,
			},
		)?;

		Ok(rx)
	}
}

/// Object-safe surface over [`BitswapHandle::request_stream`], allowing consumers to mock
/// the bitswap client in tests.
pub trait BitswapRequest: Send + Sync {
	/// Submit a wantlist. See [`BitswapHandle::request_stream`] for full semantics.
	fn request_stream(&self, cids: Vec<Cid>) -> Result<mpsc::Receiver<FetchItem>, BitswapError>;
}

impl BitswapRequest for BitswapHandle {
	fn request_stream(&self, cids: Vec<Cid>) -> Result<mpsc::Receiver<FetchItem>, BitswapError> {
		BitswapHandle::request_stream(self, cids)
	}
}

/// Internal command sent from a [`BitswapHandle`] to the service actor.
#[derive(Debug)]
pub(crate) enum BitswapCommand {
	/// Submit a streaming request: fetch `cids` and write per-CID outcomes into `sink`.
	RequestStream { cids: Vec<Cid>, sink: mpsc::Sender<FetchItem> },
}
