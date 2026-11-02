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

//! The requester side of `/spec-msg/exchange/1`: try the source's peers in turn and accept the
//! first response that verifies under the named root. Some refusals are expected, so no peer is
//! penalized.

use crate::{protocol::PROTOCOL_NAME, LOG_TARGET};
use codec::{DecodeLimit, Encode};
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	verify_exchange, ExchangeRequest, ExchangeResponse, ExchangeVerified,
};
use sc_network::{IfDisconnected, NetworkRequest, PeerId, ProtocolName};
use std::{collections::BTreeMap, sync::Arc};

/// Decoding depth limit for responses. They hold flat vectors; this only blocks crafted nesting.
const MAX_DECODE_DEPTH: u32 = 16;

/// How a request reaches a source's collators.
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
	/// The peers that serve `source`, in the order to try them.
	fn peers(&self, source: ParaId) -> Vec<PeerId>;

	/// Send one encoded request to `peer` and return the encoded response.
	async fn request(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String>;
}

/// [`Transport`] over the node's network, to a static set of peers per source.
pub struct NetworkTransport {
	network: Arc<dyn NetworkRequest + Send + Sync>,
	peers: BTreeMap<ParaId, Vec<PeerId>>,
}

impl NetworkTransport {
	/// Requests for `source` go to `peers[source]`, in order. The network must know how to reach
	/// them, for example as reserved peers.
	pub fn new(
		network: Arc<dyn NetworkRequest + Send + Sync>,
		peers: BTreeMap<ParaId, Vec<PeerId>>,
	) -> Self {
		Self { network, peers }
	}
}

#[async_trait::async_trait]
impl Transport for NetworkTransport {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		self.peers.get(&source).cloned().unwrap_or_default()
	}

	async fn request(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String> {
		self.network
			.request(
				peer,
				ProtocolName::from(PROTOCOL_NAME),
				request,
				None,
				IfDisconnected::TryConnect,
			)
			.await
			.map(|(response, _)| response)
			.map_err(|e| e.to_string())
	}
}

/// A response that verified under its request's root, with what it proved.
pub struct Verified {
	/// The response, for its proofs and its payloads; built only after it verified.
	pub response: ExchangeResponse,
	/// What the response proved.
	pub outcome: ExchangeVerified,
}

/// Why no response was accepted.
#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
	/// No peer is known for the source.
	NoPeers,
	/// Every peer refused, failed or sent a response that did not verify.
	Unserved,
}

/// Send `request` to `source`'s peers until one response verifies.
pub async fn fetch<T: Transport + ?Sized>(
	transport: &T,
	source: ParaId,
	request: &ExchangeRequest,
) -> Result<Verified, FetchError> {
	let peers = transport.peers(source);
	if peers.is_empty() {
		return Err(FetchError::NoPeers);
	}
	let encoded = request.encode();
	for peer in peers {
		let bytes = match transport.request(peer, encoded.clone()).await {
			Ok(bytes) => bytes,
			Err(e) => {
				log::debug!(target: LOG_TARGET, "Peer {peer} of {source:?} did not serve: {e}");
				continue;
			},
		};
		let mut input = &bytes[..];
		let response = match ExchangeResponse::decode_with_depth_limit(MAX_DECODE_DEPTH, &mut input)
		{
			Ok(response) if input.is_empty() => response,
			_ => {
				log::debug!(target: LOG_TARGET, "Peer {peer} of {source:?} sent junk");
				continue;
			},
		};
		match verify_exchange(request, &response) {
			Ok(outcome) => return Ok(Verified { response, outcome }),
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Response of peer {peer} of {source:?} does not verify: {e:?}",
				);
			},
		}
	}
	Err(FetchError::Unserved)
}
