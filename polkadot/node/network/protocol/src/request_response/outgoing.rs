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

use futures::{channel::oneshot, prelude::Future, FutureExt};

use network::ProtocolName;
use parity_scale_codec::{Decode, Encode, Error as DecodingError};

use sc_network as network;
use sc_network::PeerId;

use polkadot_primitives::AuthorityDiscoveryId;

use super::{v1, v2, IsRequest, Protocol};

/// All requests that can be sent to the network bridge via `NetworkBridgeTxMessage::SendRequest`.
#[derive(Debug)]
pub enum Requests {
	/// Request an availability chunk from a node.
	ChunkFetchingV1(OutgoingRequest<v1::ChunkFetchingRequest>),
	/// Fetch a collation from a collator which previously announced it.
	CollationFetchingV1(OutgoingRequest<v1::CollationFetchingRequest>),
	/// Fetch a PoV from a validator which previously sent out a seconded statement.
	PoVFetchingV1(OutgoingRequest<v1::PoVFetchingRequest>),
	/// Request full available data from a node.
	AvailableDataFetchingV1(OutgoingRequest<v1::AvailableDataFetchingRequest>),
	/// Requests for fetching large statements as part of statement distribution.
	StatementFetchingV1(OutgoingRequest<v1::StatementFetchingRequest>),
	/// Requests for notifying about an ongoing dispute.
	DisputeSendingV1(OutgoingRequest<v1::DisputeRequest>),

	/// Request a candidate and attestations.
	AttestedCandidateV2(OutgoingRequest<v2::AttestedCandidateRequest>),
	/// Fetch a collation from a collator which previously announced it.
	/// Compared to V1 it requires specifying which candidate is requested by its hash.
	CollationFetchingV2(OutgoingRequest<v2::CollationFetchingRequest>),
}

impl Requests {
	/// Encode the request.
	///
	/// The corresponding protocol is returned as well, as we are now leaving typed territory.
	///
	/// Note: `Requests` is just an enum collecting all supported requests supported by network
	/// bridge, it is never sent over the wire. This function just encodes the individual requests
	/// contained in the `enum`.
	pub fn encode_request(self) -> (Protocol, OutgoingRequest<Vec<u8>>) {
		match self {
			Self::ChunkFetchingV1(r) => r.encode_request(),
			Self::CollationFetchingV1(r) => r.encode_request(),
			Self::CollationFetchingV2(r) => r.encode_request(),
			Self::PoVFetchingV1(r) => r.encode_request(),
			Self::AvailableDataFetchingV1(r) => r.encode_request(),
			Self::StatementFetchingV1(r) => r.encode_request(),
			Self::DisputeSendingV1(r) => r.encode_request(),
			Self::AttestedCandidateV2(r) => r.encode_request(),
		}
	}
}

/// Used by the network to send us a response to a request.
pub type ResponseSender = oneshot::Sender<Result<(Vec<u8>, ProtocolName), network::RequestFailure>>;

/// Any error that can occur when sending a request.
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
	/// Response could not be decoded.
	#[error("Response could not be decoded: {0}")]
	InvalidResponse(#[from] DecodingError),

	/// Some error in substrate/libp2p happened.
	#[error("{0}")]
	NetworkError(#[from] network::RequestFailure),

	/// Response got canceled by networking.
	#[error("Response channel got canceled")]
	Canceled(#[from] oneshot::Canceled),
}

impl RequestError {
	/// Whether the error represents some kind of timeout condition.
	pub fn is_timed_out(&self) -> bool {
		match self {
			Self::Canceled(_) |
			Self::NetworkError(network::RequestFailure::Obsolete) |
			Self::NetworkError(network::RequestFailure::Network(
				network::OutboundFailure::Timeout,
			)) => true,
			_ => false,
		}
	}
}

/// A request to be sent to the network bridge, including a sender for sending responses/failures.
///
/// The network implementation will make use of that sender for informing the requesting subsystem
/// about responses/errors.
///
/// When using `Recipient::Peer`, keep in mind that no address (as in IP address and port) might
/// be known for that specific peer. You are encouraged to use `Peer` for peers that you are
/// expected to be already connected to.
/// When using `Recipient::Authority`, the addresses can be found thanks to the authority
/// discovery system.
#[derive(Debug)]
pub struct OutgoingRequest<Req, FallbackReq = Req> {
	/// Intended recipient of this request.
	pub peer: Recipient,
	/// The actual request to send over the wire.
	pub payload: Req,
	/// Optional fallback request and protocol.
	pub fallback_request: Option<(FallbackReq, Protocol)>,
	/// Sender which is used by networking to get us back a response.
	pub pending_response: ResponseSender,
}

/// Potential recipients of an outgoing request.
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub enum Recipient {
	/// Recipient is a regular peer and we know its peer id.
	Peer(PeerId),
	/// Recipient is a validator, we address it via this `AuthorityDiscoveryId`.
	Authority(AuthorityDiscoveryId),
}

/// Responses received for an `OutgoingRequest`.
pub type OutgoingResult<Res> = Result<Res, RequestError>;

impl<Req, FallbackReq> OutgoingRequest<Req, FallbackReq>
where
	Req: IsRequest + Encode,
	Req::Response: Decode,
	FallbackReq: IsRequest + Encode,
	FallbackReq::Response: Decode,
{
	/// Create a new `OutgoingRequest`.
	///
	/// It will contain a sender that is used by the networking for sending back responses. The
	/// connected receiver is returned as the second element in the returned tuple.
	pub fn new(
		peer: Recipient,
		payload: Req,
	) -> (Self, impl Future<Output = OutgoingResult<Req::Response>>) {
		let (tx, rx) = oneshot::channel();
		let r = Self { peer, payload, pending_response: tx, fallback_request: None };
		(r, receive_response::<Req>(rx.map(|r| r.map(|r| r.map(|(resp, _)| resp)))))
	}

	/// Create a new `OutgoingRequest` with a fallback in case the remote does not support this
	/// protocol. Useful when adding a new version of a req-response protocol, to achieve
	/// compatibility with the older version.
	///
	/// Returns a raw `Vec<u8>` response over the channel. Use the associated `ProtocolName` to know
	/// which request was the successful one and appropriately decode the response.
	// WARNING: This is commented for now because it's not used yet.
	// If you need it, make sure to test it. You may need to enable the V1 substream upgrade
	// protocol, unless libp2p was in the meantime updated to a version that fixes the problem
	// described in https://github.com/libp2p/rust-libp2p/issues/5074
	// pub fn new_with_fallback(
	// 	peer: Recipient,
	// 	payload: Req,
	// 	fallback_request: FallbackReq,
	// ) -> (Self, impl Future<Output = OutgoingResult<(Vec<u8>, ProtocolName)>>) {
	// 	let (tx, rx) = oneshot::channel();
	// 	let r = Self {
	// 		peer,
	// 		payload,
	// 		pending_response: tx,
	// 		fallback_request: Some((fallback_request, FallbackReq::PROTOCOL)),
	// 	};
	// 	(r, async { Ok(rx.await??) })
	// }

	/// Encode a request into a `Vec<u8>`.
	///
	/// As this throws away type information, we also return the `Protocol` this encoded request
	/// adheres to.
	pub fn encode_request(self) -> (Protocol, OutgoingRequest<Vec<u8>>) {
		let OutgoingRequest { peer, payload, pending_response, fallback_request } = self;
		let encoded = OutgoingRequest {
			peer,
			payload: payload.encode(),
			fallback_request: fallback_request.map(|(r, p)| (r.encode(), p)),
			pending_response,
		};
		(Req::PROTOCOL, encoded)
	}
}

/// Future for actually receiving a typed response for an `OutgoingRequest`.
async fn receive_response<Req>(
	rec: impl Future<Output = Result<Result<Vec<u8>, network::RequestFailure>, oneshot::Canceled>>,
) -> OutgoingResult<Req::Response>
where
	Req: IsRequest,
	Req::Response: Decode,
{
	let raw = rec.await??;
	Ok(Decode::decode(&mut raw.as_ref())?)
}
