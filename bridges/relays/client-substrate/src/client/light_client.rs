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
//!
//! smoldot's JSON-RPC server doesn't serve GRANDPA justifications. They are taken from GRANDPA
//! warp sync proofs instead, see [`LightChainHandle::warp_proof`].

use crate::{
	client::rpc::MAX_SUBSCRIPTION_CAPACITY,
	error::{Error, Result},
};

use async_trait::async_trait;
use bp_header_chain::justification::GrandpaJustification;
use codec::{Decode, Encode};
use futures::Stream;
use jsonrpsee::{
	core::client::{ClientBuilder, ReceivedMessage, TransportReceiverT, TransportSenderT},
	ws_client::WsClient,
};
use smoldot_light::{
	platform::DefaultPlatform, AddChainConfig, AddChainConfigJsonRpc, ChainId, Client,
	JsonRpcResponses,
};
use sp_core::Bytes;
use sp_runtime::traits::Header as HeaderT;
use std::{
	collections::VecDeque,
	iter,
	num::NonZero,
	path::Path,
	sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
	time::Duration,
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

/// Number of peers asked for a warp sync proof, one after the other, before giving up.
const WARP_PROOF_ATTEMPTS: u32 = 3;

/// Timeout of a warp sync proof request to a single peer.
const WARP_PROOF_TIMEOUT: Duration = Duration::from_secs(24);

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

impl LightChainHandle {
	/// Asks the chain's peers for a GRANDPA warp sync proof starting at `begin` and returns its
	/// SCALE encoding. `begin` must be finalized. The proof is **not verified**.
	pub(crate) async fn warp_proof(&self, begin: [u8; 32]) -> Result<Vec<u8>> {
		// the returned future doesn't borrow the client, so the lock is released before `.await`
		let request = lock(&self.client).grandpa_warp_sync_proof(
			self.chain_id,
			begin,
			WARP_PROOF_ATTEMPTS,
			WARP_PROOF_TIMEOUT,
		);
		let response = request
			.await
			.map_err(|e| Error::LightClient(format!("warp sync proof request failed: {e}")))?;
		Ok(response.as_encoded().to_vec())
	}
}

/// One fragment of a GRANDPA warp sync proof: a header and the justification that finalizes it.
///
/// Same SCALE layout as `sc_consensus_grandpa::warp_proof::WarpSyncFragment`.
#[derive(Encode, Decode)]
pub(crate) struct WarpSyncFragment<H: HeaderT> {
	/// The last header of an authority set, or the latest finalized header.
	pub header: H,
	/// Justification of `header`.
	pub justification: GrandpaJustification<H>,
}

/// GRANDPA warp sync proof, as sent by Substrate nodes and passed through by smoldot.
///
/// Same SCALE layout as `sc_consensus_grandpa::warp_proof::WarpSyncProof`.
#[derive(Encode, Decode)]
pub(crate) struct WarpSyncProof<H: HeaderT> {
	/// Fragments, in increasing block number order.
	pub proofs: Vec<WarpSyncFragment<H>>,
	/// `false` if the proof was cut short because of its size. Ask again from its last fragment.
	pub is_finished: bool,
}

/// Decodes a warp sync proof.
pub(crate) fn decode_warp_proof<H: HeaderT>(encoded: &[u8]) -> Result<WarpSyncProof<H>> {
	Ok(WarpSyncProof::decode(&mut &encoded[..])?)
}

/// Converts a block hash to the form smoldot takes.
pub(crate) fn hash_to_bytes<Hash: AsRef<[u8]>>(hash: Hash) -> Result<[u8; 32]> {
	<[u8; 32]>::try_from(hash.as_ref()).map_err(|_| {
		Error::LightClient(format!("block hash is {} bytes long, expected 32", hash.as_ref().len()))
	})
}

/// Returns the justification of the given block, if the proof has a fragment for it.
pub(crate) fn justification_for<H: HeaderT>(
	proof: &WarpSyncProof<H>,
	hash: H::Hash,
) -> Option<GrandpaJustification<H>> {
	proof
		.proofs
		.iter()
		.find(|fragment| fragment.header.hash() == hash)
		.map(|fragment| fragment.justification.clone())
}

/// Returns the justifications of the fragments above `after`, in increasing block number order
/// and with at most one justification per block number.
pub(crate) fn new_justifications<H: HeaderT>(
	proof: WarpSyncProof<H>,
	after: H::Number,
) -> Vec<GrandpaJustification<H>> {
	let mut last = after;
	let mut justifications = Vec::new();
	for fragment in proof.proofs {
		let number = *fragment.header.number();
		if number > last {
			last = number;
			justifications.push(fragment.justification);
		}
	}
	justifications
}

/// Fetches and decodes a warp sync proof starting at `begin`.
async fn fetch_warp_proof<H: HeaderT>(
	light: &LightChainHandle,
	begin: H::Hash,
) -> Result<WarpSyncProof<H>> {
	decode_warp_proof(&light.warp_proof(hash_to_bytes(begin)?).await?)
}

/// State of [`grandpa_justifications`].
struct JustificationsState<H: HeaderT> {
	light: Arc<LightChainHandle>,
	/// Where the next warp sync proof starts.
	begin: H::Hash,
	/// Number of the last yielded (or initial) block.
	last_number: H::Number,
	/// Justifications not yielded yet.
	pending: VecDeque<GrandpaJustification<H>>,
	/// Whether to poll again without waiting first.
	poll_now: bool,
}

/// Stream of SCALE-encoded GRANDPA justifications, made by polling warp sync proofs.
///
/// Yields the justifications of authority set changes and of the latest finalized block that
/// peers know, above `last_number`. Unlike `grandpa_subscribeJustifications`, it doesn't yield a
/// justification for every finalized block. Never ends: failed requests are logged and retried
/// after `poll_interval`.
pub(crate) fn grandpa_justifications<H: HeaderT>(
	light: Arc<LightChainHandle>,
	begin: H::Hash,
	last_number: H::Number,
	poll_interval: Duration,
	chain: &'static str,
) -> impl Stream<Item = Bytes> + Send + 'static {
	let state = JustificationsState::<H> {
		light,
		begin,
		last_number,
		pending: VecDeque::new(),
		poll_now: true,
	};
	futures::stream::unfold(state, move |mut state| async move {
		loop {
			if let Some(justification) = state.pending.pop_front() {
				return Some((Bytes(justification.encode()), state));
			}

			if !state.poll_now {
				tokio::time::sleep(poll_interval).await;
			}

			match fetch_warp_proof::<H>(&state.light, state.begin).await {
				Ok(proof) => {
					let is_finished = proof.is_finished;
					let justifications = new_justifications(proof, state.last_number);
					if let Some(last) = justifications.last() {
						state.begin = last.commit.target_hash;
						state.last_number = last.commit.target_number;
					}
					// an unfinished proof continues right away, unless it brought nothing new
					state.poll_now = !is_finished && !justifications.is_empty();
					state.pending.extend(justifications);
				},
				Err(error) => {
					tracing::debug!(
						target: "bridge",
						%error,
						node=%chain,
						begin=?state.begin,
						"Failed to get GRANDPA justifications from a warp sync proof"
					);
					state.poll_now = false;
				},
			}
		}
	})
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

#[cfg(test)]
mod tests {
	use super::*;
	use bp_test_utils::{make_default_justification, test_header};
	use sp_runtime::traits::BlakeTwo256;

	type Header = sp_runtime::generic::Header<u32, BlakeTwo256>;

	fn fragment(number: u32) -> WarpSyncFragment<Header> {
		let header = test_header::<Header>(number);
		let justification = make_default_justification(&header);
		WarpSyncFragment { header, justification }
	}

	fn proof(numbers: &[u32]) -> WarpSyncProof<Header> {
		WarpSyncProof { proofs: numbers.iter().copied().map(fragment).collect(), is_finished: true }
	}

	fn target_numbers(justifications: &[GrandpaJustification<Header>]) -> Vec<u32> {
		justifications.iter().map(|j| j.commit.target_number).collect()
	}

	#[test]
	fn decodes_substrate_warp_proof_layout() {
		// `WarpSyncProof { proofs: Vec<WarpSyncFragment { header, justification }>, is_finished }`
		let fragments: Vec<(Header, GrandpaJustification<Header>)> = [1, 5]
			.into_iter()
			.map(|n| {
				let header = test_header::<Header>(n);
				let justification = make_default_justification(&header);
				(header, justification)
			})
			.collect();
		let encoded = (fragments.clone(), false).encode();

		let decoded = decode_warp_proof::<Header>(&encoded).unwrap();
		assert!(!decoded.is_finished);
		assert_eq!(decoded.proofs.len(), 2);
		for (fragment, (header, justification)) in decoded.proofs.iter().zip(fragments) {
			assert_eq!(fragment.header, header);
			assert_eq!(fragment.justification, justification);
		}
		assert_eq!(decoded.encode(), encoded);
	}

	#[test]
	fn justification_for_finds_and_misses() {
		let proof = proof(&[1, 5, 9]);
		let hash = test_header::<Header>(5).hash();
		assert_eq!(
			justification_for(&proof, hash),
			Some(make_default_justification(&test_header(5)))
		);
		assert_eq!(justification_for(&proof, test_header::<Header>(7).hash()), None);
	}

	#[test]
	fn new_justifications_filters_and_dedups() {
		assert_eq!(target_numbers(&new_justifications(proof(&[1, 5, 5, 9]), 1)), vec![5, 9]);
		assert_eq!(target_numbers(&new_justifications(proof(&[1, 5, 9]), 0)), vec![1, 5, 9]);
		assert!(new_justifications(proof(&[1, 5, 9]), 9).is_empty());
		assert!(new_justifications(proof(&[]), 0).is_empty());
	}

	#[test]
	fn hash_to_bytes_checks_length() {
		assert_eq!(hash_to_bytes(sp_core::H256::repeat_byte(7)).unwrap(), [7; 32]);
		assert!(hash_to_bytes([0u8; 20]).is_err());
	}
}
