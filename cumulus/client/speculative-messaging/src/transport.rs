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

//! Concrete [`NetworkTransport`] implementation backed by
//! [`sc_network::service::traits::NetworkRequest`].

use std::sync::Arc;

use async_trait::async_trait;
use codec::{Decode, Encode};
use sc_network::{service::traits::NetworkRequest, IfDisconnected, ProtocolName};

use crate::{
	error::Error,
	protocol::{ForwardMessageRequest, ForwardMessageResponse},
	registry::OpaquePeerId,
	service::NetworkTransport,
};

/// A [`NetworkTransport`] that delegates to the relay chain's
/// [`NetworkRequest`] implementation.
pub struct ScNetworkTransport {
	network: Arc<dyn NetworkRequest + Send + Sync>,
	protocol: ProtocolName,
}

impl ScNetworkTransport {
	/// Create a new transport using the given relay chain network service
	/// and protocol name.
	pub fn new(
		network: Arc<dyn NetworkRequest + Send + Sync>,
		protocol: ProtocolName,
	) -> Self {
		Self { network, protocol }
	}
}

#[async_trait]
impl NetworkTransport for ScNetworkTransport {
	async fn send_request(
		&self,
		peer: &OpaquePeerId,
		request: ForwardMessageRequest,
	) -> Result<ForwardMessageResponse, Error> {
		let peer_id = sc_network::PeerId::from_bytes(peer)
			.map_err(|e| Error::SendFailed(format!("Invalid peer ID: {e:?}")))?;

		let request_bytes = request.encode();

		let (response_bytes, _protocol) = self
			.network
			.request(
				peer_id.into(),
				self.protocol.clone(),
				request_bytes,
				None,
				IfDisconnected::TryConnect,
			)
			.await
			.map_err(|e| Error::SendFailed(format!("Request failed: {e:?}")))?;

		ForwardMessageResponse::decode(&mut &response_bytes[..])
			.map_err(|e| Error::ReceiveFailed(format!("Failed to decode response: {e}")))
	}
}
