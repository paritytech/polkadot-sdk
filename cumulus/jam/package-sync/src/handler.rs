// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Incoming-request handler for the package-info protocol.
//!
//! The handler is shaped like `sc-network-sync`'s `CodeRequestHandler`: an LRU keyed by
//! `(peer, block hash)` caps how often the same request is answered, and the peer is mildly
//! penalised when it exceeds the cap. Two differences from the code handler:
//!
//! - the authority-discovery check is a *soft* gate: a peer is only refused when the cache is
//!   non-empty and says the peer is not an authority, so a freshly started collator whose cache is
//!   still empty is never locked out;
//! - only fulfilled `Known` answers count towards the same-request cap, so repeated `Unknown`
//!   responses (a node that simply does not know the block yet) are not penalised.

use crate::types::{PackageInfo, PackageInfoRequest, PackageInfoResponse};
use async_trait::async_trait;
use codec::{Decode, Encode};
use futures::{channel::oneshot, stream::StreamExt};
use sc_network::{
	request_responses::{IncomingRequest, OutgoingResponse},
	ReputationChange as Rep,
};
use sc_network_types::PeerId;
use schnellru::{ByLength, LruMap};
use std::{hash::Hash as StdHash, sync::Arc};
use tracing::debug;

/// Maximum number of times the same request from the same peer is answered before it is refused.
const MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER: u32 = 2;

/// Capacity of the seen-requests LRU. Only used to bound memory against request spam.
const SEEN_REQUESTS_CAPACITY: u32 = 2048;

/// Log target for this module.
const LOG_TARGET: &str = "jam-package-sync::handler";

mod rep {
	use super::Rep;

	/// Mild reputation change when a peer sent us the same request too many times.
	pub const SAME_REQUEST: Rep = Rep::new(-10, "Same package request multiple times");
}

/// Source of the package metadata this node is willing to serve.
///
/// Implemented in `polkadot-omni-node-lib` over the package store.
pub trait PackageInfoProvider<Hash>: Send + Sync + 'static {
	/// The info this node knows about `block`, if any.
	fn package_info(&self, block: &Hash) -> Option<PackageInfo>;
}

/// Whether a peer is a collator, as far as the authority-discovery cache knows.
#[async_trait]
pub trait PeerAuthorities: Send + 'static {
	/// `Some(true)` if the peer is a known authority, `Some(false)` if the cache is non-empty and
	/// the peer is absent, `None` if the cache is empty or the query failed (unknown, allow).
	async fn is_collator(&mut self, peer: PeerId) -> Option<bool>;
}

#[async_trait]
impl PeerAuthorities for sc_authority_discovery::Service {
	async fn is_collator(&mut self, peer: PeerId) -> Option<bool> {
		self.get_authority_ids_by_peer_id(peer).await.map(|ids| !ids.is_empty())
	}
}

/// Handler for incoming package-info requests.
///
/// `Hash` is the block-hash type the provider is keyed by; it is also the seen-request marker.
pub struct PackageInfoRequestHandler<P, A, Hash> {
	provider: Arc<P>,
	authorities: A,
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Maps a `(peer, requested block hash)` to the number of fulfilled `Known` answers.
	seen_requests: LruMap<(PeerId, Hash), u32>,
}

impl<P, A, Hash> PackageInfoRequestHandler<P, A, Hash>
where
	P: PackageInfoProvider<Hash>,
	Hash: Decode + Eq + StdHash + Clone,
	A: PeerAuthorities,
{
	/// Create a new handler reading requests from `request_receiver`.
	pub fn new(
		provider: Arc<P>,
		authorities: A,
		request_receiver: async_channel::Receiver<IncomingRequest>,
	) -> Self {
		Self {
			provider,
			authorities,
			request_receiver,
			seen_requests: LruMap::new(ByLength::new(SEEN_REQUESTS_CAPACITY)),
		}
	}

	/// Run the handler until the request channel is closed.
	pub async fn run(mut self) {
		while let Some(request) = self.request_receiver.next().await {
			let IncomingRequest { peer, payload, pending_response } = request;

			if let Err(e) = self.handle_request(payload, pending_response, &peer).await {
				debug!(target: LOG_TARGET, "Failed to handle package info request from {peer}: {e}");
			}
		}
	}

	async fn handle_request(
		&mut self,
		payload: Vec<u8>,
		pending_response: oneshot::Sender<OutgoingResponse>,
		peer: &PeerId,
	) -> Result<(), HandleRequestError> {
		let request = PackageInfoRequest::<Hash>::decode(&mut payload.as_slice())?;
		let key = (*peer, request.block_hash.clone());

		let mut reputation_changes = Vec::new();

		if !is_authority(&mut self.authorities, peer).await {
			return respond(pending_response, Err(()), reputation_changes);
		}

		if over_cap(&mut self.seen_requests, &key) {
			reputation_changes.push(rep::SAME_REQUEST);
			return respond(pending_response, Err(()), reputation_changes);
		}

		let response = match self.provider.package_info(&request.block_hash) {
			Some(info) => {
				record_known(&mut self.seen_requests, key);
				PackageInfoResponse::Known(info)
			},
			None => PackageInfoResponse::Unknown,
		};

		respond(pending_response, Ok(response.encode()), reputation_changes)
	}
}

/// The soft authority gate: refuse only when the cache is non-empty and the peer is absent.
async fn is_authority<A: PeerAuthorities>(authorities: &mut A, peer: &PeerId) -> bool {
	!matches!(authorities.is_collator(*peer).await, Some(false))
}

/// Whether the same `(peer, request)` has been fulfilled `Known` too many times.
fn over_cap<Hash>(seen: &mut LruMap<(PeerId, Hash), u32>, key: &(PeerId, Hash)) -> bool
where
	Hash: Eq + StdHash,
{
	seen.get(key).map(|count| *count).unwrap_or(0) >= MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER
}

/// Record one fulfilled `Known` answer for `key`.
fn record_known<Hash>(seen: &mut LruMap<(PeerId, Hash), u32>, key: (PeerId, Hash))
where
	Hash: Eq + StdHash,
{
	let count = seen.get(&key).map(|count| *count).unwrap_or(0);
	seen.insert(key, count.saturating_add(1));
}

/// Send `result` back to the peer.
fn respond(
	pending_response: oneshot::Sender<OutgoingResponse>,
	result: Result<Vec<u8>, ()>,
	reputation_changes: Vec<Rep>,
) -> Result<(), HandleRequestError> {
	pending_response
		.send(OutgoingResponse { result, reputation_changes, sent_feedback: None })
		.map_err(|_| HandleRequestError::SendResponse)
}

/// Errors that can occur while handling one request.
#[derive(Debug)]
enum HandleRequestError {
	/// The request payload could not be SCALE-decoded.
	Decode(codec::Error),
	/// The response could not be sent (the requester went away).
	SendResponse,
}

impl std::fmt::Display for HandleRequestError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Decode(e) => write!(f, "Failed to decode request: {e}."),
			Self::SendResponse => write!(f, "Failed to send response."),
		}
	}
}

impl std::error::Error for HandleRequestError {}

impl From<codec::Error> for HandleRequestError {
	fn from(e: codec::Error) -> Self {
		Self::Decode(e)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::PovSpec;
	use std::collections::HashMap;

	type TestHash = [u8; 32];

	#[derive(Default)]
	struct MockProvider {
		infos: HashMap<TestHash, PackageInfo>,
	}

	impl PackageInfoProvider<TestHash> for MockProvider {
		fn package_info(&self, block: &TestHash) -> Option<PackageInfo> {
			self.infos.get(block).cloned()
		}
	}

	struct MockAuthorities(Option<bool>);

	#[async_trait]
	impl PeerAuthorities for MockAuthorities {
		async fn is_collator(&mut self, _peer: PeerId) -> Option<bool> {
			self.0
		}
	}

	fn info(seed: u8) -> PackageInfo {
		PackageInfo {
			authorization: vec![seed],
			prerequisites: vec![[seed; 32]],
			pov: PovSpec { hash: [seed; 32], len: seed as u32 },
		}
	}

	fn provider_with_info(hash: TestHash) -> MockProvider {
		let mut provider = MockProvider::default();
		provider.infos.insert(hash, info(1));
		provider
	}

	fn info_handler(
		provider: MockProvider,
		authorities: MockAuthorities,
	) -> PackageInfoRequestHandler<MockProvider, MockAuthorities, TestHash> {
		let (_tx, rx) = async_channel::bounded(1);
		PackageInfoRequestHandler::new(Arc::new(provider), authorities, rx)
	}

	async fn send_info(
		handler: &mut PackageInfoRequestHandler<MockProvider, MockAuthorities, TestHash>,
		hash: TestHash,
		peer: PeerId,
	) -> OutgoingResponse {
		let (tx, rx) = oneshot::channel();
		handler
			.handle_request(PackageInfoRequest { block_hash: hash }.encode(), tx, &peer)
			.await
			.expect("request is handled");
		rx.await.expect("a response is sent")
	}

	#[test]
	fn info_handler_serves_known_and_unknown() {
		let hash = [1u8; 32];
		let mut handler = info_handler(provider_with_info(hash), MockAuthorities(None));

		let known = futures::executor::block_on(send_info(&mut handler, hash, PeerId::random()));
		assert_eq!(
			PackageInfoResponse::decode(&mut known.result.unwrap().as_slice()).unwrap(),
			PackageInfoResponse::Known(info(1)),
		);

		let unknown =
			futures::executor::block_on(send_info(&mut handler, [9u8; 32], PeerId::random()));
		assert_eq!(
			PackageInfoResponse::decode(&mut unknown.result.unwrap().as_slice()).unwrap(),
			PackageInfoResponse::Unknown,
		);
	}

	#[test]
	fn soft_gate_allows_when_cache_is_empty() {
		let hash = [1u8; 32];
		let mut handler = info_handler(provider_with_info(hash), MockAuthorities(None));

		let response = futures::executor::block_on(send_info(&mut handler, hash, PeerId::random()));
		assert!(response.result.is_ok());
	}

	#[test]
	fn soft_gate_refuses_when_cache_is_non_empty_and_peer_absent() {
		let hash = [1u8; 32];
		let mut handler = info_handler(provider_with_info(hash), MockAuthorities(Some(false)));

		let response = futures::executor::block_on(send_info(&mut handler, hash, PeerId::random()));
		assert!(response.result.is_err());
	}

	#[test]
	fn same_request_cap_counts_only_known() {
		let hash = [1u8; 32];
		let peer = PeerId::random();
		let mut handler = info_handler(provider_with_info(hash), MockAuthorities(None));

		for _ in 0..MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER {
			let response = futures::executor::block_on(send_info(&mut handler, hash, peer));
			assert!(response.result.is_ok());
		}

		let refused = futures::executor::block_on(send_info(&mut handler, hash, peer));
		assert!(refused.result.is_err());
		assert_eq!(refused.reputation_changes.len(), 1);

		// `Unknown` answers never count towards the cap.
		let mut handler = info_handler(MockProvider::default(), MockAuthorities(None));
		for _ in 0..MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER + 3 {
			let response = futures::executor::block_on(send_info(&mut handler, [9u8; 32], peer));
			assert!(response.result.is_ok());
		}
	}
}
