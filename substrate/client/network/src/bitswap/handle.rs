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

//! Public user-facing handle for the Bitswap service.
//!
//! The handle is returned by [`crate::service::traits::BitswapProvider::bitswap_handle`] when
//! the node is configured with `--ipfs-server` and uses the litep2p network backend.
//!
//! Cheap to clone. Submit work via [`BitswapHandle::request_stream`], drain the receiver to
//! get per-CID outcomes as they resolve.

use super::{is_cid_supported, Cid, MAX_WANTED_BLOCKS};

use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

/// Outcome of a single Bitswap fetch for one CID.
///
/// Operational causes for `Missing` (timeout, no peers, all DONT_HAVE, verification failure)
/// are collapsed into one variant on purpose — they are not actionable from the caller's
/// perspective. Diagnostic distinction is available via tracing/metrics on the service side.
#[derive(Debug)]
pub enum FetchOutcome {
	/// Hash-verified bytes for the requested CID.
	Block(Vec<u8>),
	/// The block was not retrieved before the request deadline expired. Could mean any of:
	/// no peers were available, every peer replied DONT_HAVE, every peer timed out, or every
	/// candidate block failed CID verification.
	Missing,
}

/// Service-level Bitswap errors.
///
/// `BitswapError` is returned **synchronously** from [`BitswapHandle::request_stream`] for
/// admission-time failures. It also appears at most once **inside** the returned stream
/// (as the `Err` variant of a stream item) to signal `ServiceClosed` mid-stream. All
/// per-CID failure modes collapse into [`FetchOutcome::Missing`] instead of producing an
/// error.
#[derive(Debug, thiserror::Error)]
pub enum BitswapError {
	/// `ipfs_server` is not enabled, or the network backend does not support Bitswap.
	#[error("Bitswap is not available on this node")]
	Unavailable,
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
	/// The service has too many in-flight wants.
	#[error("Bitswap service is overloaded")]
	Overloaded,
	/// Per-call CID count exceeds [`MAX_CIDS_PER_REQUEST`].
	#[error("too many CIDs in request: {requested} > {max}")]
	TooManyCids {
		/// CIDs requested in the failing call.
		requested: usize,
		/// Service-level maximum.
		max: usize,
	},
}

/// Maximum number of CIDs accepted in a single [`BitswapHandle::request_stream`] call.
///
/// Matches the Bitswap v1.2.0 wantlist-entry cap that the rest of the codebase already
/// enforces.
pub const MAX_CIDS_PER_REQUEST: usize = MAX_WANTED_BLOCKS;

/// Configuration applied at service construction time.
#[derive(Debug, Clone)]
pub struct BitswapServiceConfig {
	/// Per-waiter deadline. Each call to [`BitswapHandle::request_stream`] inherits this
	/// value; the receiver will yield `Missing` for any CID still unresolved at the
	/// deadline and then close.
	pub request_timeout: Duration,
}

impl Default for BitswapServiceConfig {
	fn default() -> Self {
		Self { request_timeout: Duration::from_secs(30) }
	}
}

/// Item carried on the receiver returned by [`BitswapHandle::request_stream`].
pub type FetchItem = Result<(Cid, FetchOutcome), BitswapError>;

/// User-facing handle to the Bitswap service.
///
/// Cheap to clone. Created at network construction time and stored on `NetworkService`;
/// retrieve via `NetworkService::bitswap_handle()`.
#[derive(Debug, Clone)]
pub struct BitswapHandle {
	cmd_tx: mpsc::Sender<BitswapCommand>,
}

impl BitswapHandle {
	/// Construct a new handle around an existing command sender. Used internally by
	/// [`crate::bitswap::start`].
	pub(crate) fn new(cmd_tx: mpsc::Sender<BitswapCommand>) -> Self {
		Self { cmd_tx }
	}

	/// Submit a wantlist. Returns a receiver that yields one item per requested CID, in
	/// the order they resolve.
	///
	/// Each item is:
	/// - `Ok((cid, FetchOutcome::Block(bytes)))` when a peer delivered hash-verified bytes.
	/// - `Ok((cid, FetchOutcome::Missing))` when the per-waiter deadline expired without a block.
	/// - `Err(BitswapError::ServiceClosed)` once, if the service task shuts down mid-stream.
	///
	/// The stream closes when either every CID has produced an outcome, or
	/// `ServiceClosed` has been emitted. Callers that need to know whether all CIDs were
	/// covered should track the requested set against the items received before the
	/// stream closed.
	///
	/// Returns a synchronous `BitswapError` for admission-time failures (`Unavailable`,
	/// `ServiceClosed`, `InvalidCid`, `Overloaded`, `TooManyCids`).
	///
	/// An empty `cids` slice returns an immediately-closed receiver, not an error.
	pub async fn request_stream(
		&self,
		cids: Vec<Cid>,
	) -> Result<mpsc::Receiver<FetchItem>, BitswapError> {
		if cids.is_empty() {
			let (_tx, rx) = mpsc::channel(1);
			return Ok(rx);
		}

		if cids.len() > MAX_CIDS_PER_REQUEST {
			return Err(BitswapError::TooManyCids {
				requested: cids.len(),
				max: MAX_CIDS_PER_REQUEST,
			});
		}

		for cid in &cids {
			if !is_cid_supported(cid) {
				return Err(BitswapError::InvalidCid { cid: *cid });
			}
		}

		// `cids.len() + 1` reserves one slot for a possible terminal `Err(ServiceClosed)`,
		// so the actor's `try_send` for outcomes never fails for well-behaved callers.
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

/// Object-safe surface over [`BitswapHandle::request_stream`].
///
/// Hold an `Arc<dyn BitswapRequest>` to abstract over the bitswap client for testing or
/// for late-bound wiring. The trait carries no methods beyond `request_stream`; consumers
/// that need other [`BitswapHandle`] functionality should keep the concrete type.
#[async_trait]
pub trait BitswapRequest: Send + Sync {
	/// Submit a wantlist. See [`BitswapHandle::request_stream`] for full semantics.
	async fn request_stream(
		&self,
		cids: Vec<Cid>,
	) -> Result<mpsc::Receiver<FetchItem>, BitswapError>;
}

#[async_trait]
impl BitswapRequest for BitswapHandle {
	async fn request_stream(
		&self,
		cids: Vec<Cid>,
	) -> Result<mpsc::Receiver<FetchItem>, BitswapError> {
		BitswapHandle::request_stream(self, cids).await
	}
}

#[async_trait]
impl<T> BitswapRequest for Arc<T>
where
	T: BitswapRequest + ?Sized,
{
	async fn request_stream(
		&self,
		cids: Vec<Cid>,
	) -> Result<mpsc::Receiver<FetchItem>, BitswapError> {
		T::request_stream(self, cids).await
	}
}

/// Internal command sent from a [`BitswapHandle`] to the service actor.
#[derive(Debug)]
pub(crate) enum BitswapCommand {
	/// Submit a streaming request. The actor inserts a `Waiter` keyed by these `cids`, with
	/// the configured deadline, and writes per-CID outcomes into `sink` as they resolve.
	RequestStream {
		/// Wantlist (already validated for emptiness, cap, and CID support at admission).
		cids: Vec<Cid>,
		/// Sink for `Ok((cid, outcome))` items and the optional final `Err(ServiceClosed)`.
		sink: mpsc::Sender<FetchItem>,
	},
}

/// Peer connect/disconnect events published from the litep2p backend's main loop into the
/// Bitswap service actor.
///
/// `Snapshot` is delivered exactly once, as the first event after the actor subscribes, so
/// the actor learns about already-established connections that pre-date its startup. The
/// actor MUST handle `Snapshot` before treating `connected_peers` as authoritative.
#[derive(Debug, Clone)]
pub enum PeerEvent {
	/// Initial snapshot of currently-connected peers.
	Snapshot {
		/// Already-connected peers at subscription time.
		peers: Vec<litep2p::PeerId>,
	},
	/// A new peer connected.
	Connected {
		/// Peer ID.
		peer: litep2p::PeerId,
	},
	/// A previously-connected peer disconnected.
	Disconnected {
		/// Peer ID.
		peer: litep2p::PeerId,
	},
}

/// Wiring produced by [`crate::bitswap::start`] and consumed by the litep2p network
/// backend at construction time.
///
/// The backend:
/// - feeds [`Self::litep2p_config`] into `Litep2pConfigBuilder::with_libp2p_bitswap`,
/// - stores [`Self::user_handle`] on `Litep2pNetworkService` for the `bitswap_handle()` accessor,
/// - publishes `PeerEvent`s into [`Self::peer_event_tx`] from its main loop.
pub struct BitswapWiring {
	/// Litep2p protocol config; consumed by `with_libp2p_bitswap`.
	pub litep2p_config: litep2p::protocol::libp2p::bitswap::Config,
	/// Public, cloneable user-facing handle.
	pub user_handle: BitswapHandle,
	/// Sender into which the backend's main loop publishes peer events. The actor holds the
	/// receiver internally.
	pub peer_event_tx: mpsc::Sender<PeerEvent>,
}

impl std::fmt::Debug for BitswapWiring {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("BitswapWiring")
			.field("user_handle", &self.user_handle)
			.finish_non_exhaustive()
	}
}
