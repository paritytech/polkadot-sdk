// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The requester's transport of the `/spec-msg/exchange` protocol: who to
//! ask, how to ask, and what a misbehaving answer costs the peer.
//!
//! Requests cross chain boundaries — a receiver's collator dials the
//! *sender* chain's nodes — so peers are supplied per source chain
//! ([`SourcePeers`]) rather than taken from the own chain's peer set.
//! Nothing is trusted by connection: every response is verified against the
//! root the request named ([`crate::verify`]); a response failing
//! verification (or not decoding) discards the response AND the peer
//! ([`SourcePeers::report_bad`]) — transport failures merely rotate to the
//! next peer.
//!
//! MVP peer sourcing is a plain registry ([`PeerRegistry`]) the node wiring
//! fills (static addresses; zombienet). Discovering a source's collators
//! dynamically via the relay chain DHT (authority discovery over the
//! source's para id) plugs in behind the same trait — wiring work, not
//! protocol work.

use std::{collections::BTreeMap, sync::Arc};

use codec::{DecodeAll, Encode};
use parking_lot::RwLock;
use sc_network::{IfDisconnected, NetworkRequest, PeerId};

use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{ExchangeRequest, ExchangeResponse};

use crate::protocol::PROTOCOL_NAME;

/// Why an exchange round trip yielded nothing usable.
#[derive(Clone, Debug, thiserror::Error)]
pub enum ExchangeError {
	/// No peer is currently known to serve the source.
	#[error("no peers known for the source")]
	NoPeers,
	/// The transport failed (dial, timeout, refusal). The peer is NOT
	/// discarded — refusing unservable requests is protocol behavior.
	#[error("transport error: {0}")]
	Network(String),
	/// The response bytes did not decode to the expected envelope. The peer
	/// is discarded.
	#[error("undecodable response")]
	MalformedResponse,
	/// The decoded response did not verify under the requested root. The
	/// peer is discarded.
	#[error("response failed verification: {0}")]
	Verify(#[from] crate::verify::VerifyError),
}

/// Where the fetch pipeline learns which peers currently serve a source
/// chain's streams — and where it reports the ones that served garbage.
pub trait SourcePeers: Send + Sync {
	/// Candidate peers for `source`, best first.
	fn peers(&self, source: ParaId) -> Vec<PeerId>;

	/// `peer` sent a response that failed verification for `source`:
	/// discard it (definitive misbehavior — verified protocol data cannot
	/// fail honestly).
	fn report_bad(&self, source: ParaId, peer: PeerId);
}

/// The MVP [`SourcePeers`] implementation: a plain per-source registry the
/// node wiring fills and the fetch pipeline consumes. Reported-bad peers
/// are removed until re-registered.
#[derive(Default)]
pub struct PeerRegistry {
	peers: RwLock<BTreeMap<ParaId, Vec<PeerId>>>,
}

impl PeerRegistry {
	/// Replaces the peer set serving `source`.
	pub fn set_peers(&self, source: ParaId, peers: Vec<PeerId>) {
		self.peers.write().insert(source, peers);
	}

	/// Adds one peer serving `source` (idempotent).
	pub fn add_peer(&self, source: ParaId, peer: PeerId) {
		let mut peers = self.peers.write();
		let entry = peers.entry(source).or_default();
		if !entry.contains(&peer) {
			entry.push(peer);
		}
	}
}

impl SourcePeers for PeerRegistry {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		self.peers.read().get(&source).cloned().unwrap_or_default()
	}

	fn report_bad(&self, source: ParaId, peer: PeerId) {
		if let Some(peers) = self.peers.write().get_mut(&source) {
			peers.retain(|candidate| *candidate != peer);
		}
	}
}

/// One raw `/spec-msg/exchange` round trip to one peer — the seam between
/// the fetch pipeline and the network backend, mockable in tests.
#[async_trait::async_trait]
pub trait ExchangeNetwork: Send + Sync {
	/// Sends the SCALE-encoded [`ExchangeRequest`] bytes and returns the raw
	/// response bytes.
	async fn exchange(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, ExchangeError>;
}

/// The production transport: any [`NetworkRequest`] handle (typically the
/// network service the wiring registered [`PROTOCOL_NAME`] with, see
/// [`crate::spec_msg_protocol_config`]).
#[async_trait::async_trait]
impl<T: NetworkRequest + Send + Sync + ?Sized> ExchangeNetwork for Arc<T> {
	async fn exchange(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, ExchangeError> {
		// `TryConnect`: source-chain peers are dialed on demand — there is
		// no notification protocol keeping the connection warm.
		self.request(peer, PROTOCOL_NAME.into(), request, None, IfDisconnected::TryConnect)
			.await
			.map(|(bytes, _protocol)| bytes)
			.map_err(|error| ExchangeError::Network(error.to_string()))
	}
}

/// Sends `request` to `peer` and decodes the envelope. A response that does
/// not decode is misbehavior; the caller reports the peer.
pub(crate) async fn exchange_once(
	network: &impl ExchangeNetwork,
	peer: PeerId,
	request: &ExchangeRequest,
) -> Result<ExchangeResponse, ExchangeError> {
	let bytes = network.exchange(peer, request.encode()).await?;
	ExchangeResponse::decode_all(&mut &bytes[..]).map_err(|_| ExchangeError::MalformedResponse)
}
