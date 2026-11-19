// Copyright (C) Parity Technologies (UK) Ltd.
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

//! JSON-RPC transport over an embedded smoldot light client.
//!
//! All chains of the process are added to a single smoldot [`Client`]. Each chain gets its own
//! jsonrpsee client, whose requests go to [`Client::json_rpc_request`] and whose responses come
//! from the chain's [`JsonRpcResponses`] stream.

use crate::{
	client::rpc::MAX_SUBSCRIPTION_CAPACITY,
	error::{Error, Result},
};

use async_trait::async_trait;
use jsonrpsee::{
	core::client::{ClientBuilder, ReceivedMessage, TransportReceiverT, TransportSenderT},
	ws_client::WsClient,
};
use smoldot_light::{
	platform::DefaultPlatform, AddChainConfig, AddChainConfigJsonRpc, ChainId, Client,
	JsonRpcResponses,
};
use std::{
	iter,
	num::NonZero,
	path::Path,
	sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
};

/// Platform used by the shared smoldot client.
type Platform = Arc<DefaultPlatform>;

/// Shared smoldot client, guarded by a mutex that is never held across `.await`.
type SharedClient = Arc<Mutex<Client<Platform>>>;

/// Maximum number of JSON-RPC requests smoldot queues for one chain.
///
/// The jsonrpsee client is trusted and already bounds the number of concurrent requests, so
/// smoldot's own limit is lifted, as its documentation recommends for trusted clients. Hitting it
/// would fail the request and force a reconnect.
const MAX_PENDING_REQUESTS: NonZero<u32> = NonZero::<u32>::MAX;

/// Maximum number of JSON-RPC subscriptions per chain.
const MAX_SUBSCRIPTIONS: u32 = 1024;

/// Maximum number of transactions smoldot holds for one chain.
const MAX_PENDING_TRANSACTIONS: NonZero<u32> = NonZero::new(64).unwrap();

/// Process-wide smoldot client, created on first use.
static LIGHT_CLIENT: OnceLock<SharedClient> = OnceLock::new();

/// Returns the process-wide smoldot client.
fn shared_client() -> SharedClient {
	LIGHT_CLIENT
		.get_or_init(|| {
			Arc::new(Mutex::new(Client::new(DefaultPlatform::new(
				"substrate-relay".into(),
				env!("CARGO_PKG_VERSION").into(),
			))))
		})
		.clone()
}

/// Locks the smoldot client.
///
/// A panic while the lock was held can't leave the client half-updated in a way that matters to
/// us: every call made under the lock is a single smoldot method call.
fn lock(client: &Mutex<Client<Platform>>) -> MutexGuard<'_, Client<Platform>> {
	client.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A chain added to the shared smoldot client. Removes the chain (and its relay chain, if any)
/// from the client when dropped.
pub(crate) struct LightChainHandle {
	/// The shared smoldot client.
	client: SharedClient,
	/// The chain with the JSON-RPC endpoint.
	chain_id: ChainId,
	/// The relay chain, if the chain is a parachain.
	relay_chain_id: Option<ChainId>,
}

impl Drop for LightChainHandle {
	fn drop(&mut self) {
		let mut client = lock(&self.client);
		let () = client.remove_chain(self.chain_id);
		if let Some(relay_chain_id) = self.relay_chain_id {
			let () = client.remove_chain(relay_chain_id);
		}
	}
}

/// Reads a chain spec file.
fn read_chain_spec(path: &Path) -> Result<String> {
	std::fs::read_to_string(path).map_err(|e| {
		Error::LightClient(format!("failed to read chain spec {}: {e}", path.display()))
	})
}

/// Adds the chain, and its relay chain if any, to the shared smoldot client.
fn add_chain(
	chain_spec: &Path,
	relay_chain_spec: Option<&Path>,
) -> Result<(LightChainHandle, JsonRpcResponses<Platform>)> {
	let specification = read_chain_spec(chain_spec)?;
	let relay_specification = relay_chain_spec.map(read_chain_spec).transpose()?;

	let shared = shared_client();
	let mut client = lock(&shared);

	let relay_chain_id = match (relay_chain_spec, relay_specification) {
		(Some(path), Some(specification)) => Some(
			client
				.add_chain(AddChainConfig {
					user_data: (),
					specification: &specification,
					database_content: "",
					potential_relay_chains: iter::empty(),
					json_rpc: AddChainConfigJsonRpc::Disabled,
					statement_protocol_config: None,
					max_pending_transactions: MAX_PENDING_TRANSACTIONS,
				})
				.map_err(|e| {
					Error::LightClient(format!("failed to add relay chain {}: {e}", path.display()))
				})?
				.chain_id,
		),
		_ => None,
	};

	let added = client.add_chain(AddChainConfig {
		user_data: (),
		specification: &specification,
		database_content: "",
		potential_relay_chains: relay_chain_id.into_iter(),
		json_rpc: AddChainConfigJsonRpc::Enabled {
			max_pending_requests: MAX_PENDING_REQUESTS,
			max_subscriptions: MAX_SUBSCRIPTIONS,
		},
		statement_protocol_config: None,
		max_pending_transactions: MAX_PENDING_TRANSACTIONS,
	});
	let added = match added {
		Ok(added) => added,
		Err(e) => {
			if let Some(relay_chain_id) = relay_chain_id {
				let () = client.remove_chain(relay_chain_id);
			}
			return Err(Error::LightClient(format!(
				"failed to add chain {}: {e}",
				chain_spec.display()
			)));
		},
	};
	drop(client);

	let handle = LightChainHandle { client: shared, chain_id: added.chain_id, relay_chain_id };
	let responses = added.json_rpc_responses.ok_or_else(|| {
		Error::LightClient(format!("chain {} has no JSON-RPC endpoint", chain_spec.display()))
	})?;
	Ok((handle, responses))
}

/// Error of the in-process transport.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct TransportError(String);

/// Sends JSON-RPC requests to smoldot.
struct Sender {
	handle: Arc<LightChainHandle>,
}

#[async_trait]
impl TransportSenderT for Sender {
	type Error = TransportError;

	async fn send(&mut self, msg: String) -> std::result::Result<(), Self::Error> {
		lock(&self.handle.client)
			.json_rpc_request(msg, self.handle.chain_id)
			.map_err(|e| TransportError(e.to_string()))
	}
}

/// Receives JSON-RPC responses and notifications from smoldot.
struct Receiver {
	responses: JsonRpcResponses<Platform>,
}

#[async_trait]
impl TransportReceiverT for Receiver {
	type Error = TransportError;

	async fn receive(&mut self) -> std::result::Result<ReceivedMessage, Self::Error> {
		self.responses
			.next()
			.await
			.map(ReceivedMessage::Text)
			.ok_or_else(|| TransportError("light client chain removed".into()))
	}
}

/// Adds the chain to the shared smoldot client and builds a jsonrpsee client on top of it.
///
/// Must be called from within a tokio runtime.
pub(crate) fn build(
	chain_spec: &Path,
	relay_chain_spec: Option<&Path>,
) -> Result<(Arc<LightChainHandle>, WsClient)> {
	let (handle, responses) = add_chain(chain_spec, relay_chain_spec)?;
	let handle = Arc::new(handle);
	let client = ClientBuilder::default()
		.max_buffer_capacity_per_subscription(MAX_SUBSCRIPTION_CAPACITY)
		.build_with_tokio(Sender { handle: handle.clone() }, Receiver { responses });
	Ok((handle, client))
}
