// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implementation of the `cumulus-jam-interface` traits over one websocket to a JAM node (JIP-2).
//!
//! One reconnecting worker owns the connection ([`worker`]); a jsonrpsee client wrapper routes
//! every request through it ([`client`]); polkajam's `RpcClient → Node` blanket impl on top of
//! that wrapper provides the typed JIP-2 surface with no hand-written RPC calls. This module
//! maps the `Node` surface onto the three `cumulus-jam-interface` traits.

mod client;
mod worker;

pub use client::JamRpcClient;
pub use worker::JamRpcWorker;

use async_trait::async_trait;
use cumulus_jam_interface::{
	BlockDesc, BoxStream, ChainSubUpdate, CoreIndex, EpochIndex, Error, Hash, HeaderHash,
	JamChainSource, JamStateSource, JamWorkPackageSubmission, MmrPeakHash, RangeProof, Result,
	ServiceId, StateRootHash, StorageKey, WorkPackage, WorkPackageHash, WorkPackageStatus,
	WorkReportHash,
};
use futures::{channel::mpsc, future::Future, StreamExt};
use jam_std_common::{Node, NodeExt};
use jam_types::AnyBytes;
use jsonrpsee_jam::{core::client::ClientT, rpc_params};
use std::time::{Duration, Instant};
use url::Url;
use worker::WorkerMessage;

const LOG_TARGET: &str = "cumulus-jam-rpc-interface";
const NOTIFICATION_CHANNEL_SIZE_LIMIT: usize = 20;
const RESUBSCRIBE_DELAY: Duration = Duration::from_secs(1);

/// The JAM node connection: implements [`JamChainSource`], [`JamStateSource`] and
/// [`JamWorkPackageSubmission`] over one shared reconnecting websocket.
#[derive(Clone, Debug)]
pub struct JamRpcInterface {
	client: JamRpcClient,
}

impl JamRpcInterface {
	/// Connect to the first available of `urls` and build the interface.
	///
	/// Returns the interface and the worker future, which the caller must spawn (essential
	/// task): it drives the connection, the request replay and the block-update fan-out.
	pub async fn new(
		urls: Vec<Url>,
	) -> std::result::Result<(Self, impl Future<Output = ()> + Send), String> {
		let (worker, to_worker, active_client) = JamRpcWorker::new(urls).await?;

		// Param-sized JAM types (auth queues, work-package bounds) decode against process-global
		// protocol parameters; apply the connected chain's parameters before anything decodes.
		// This must go directly to the just-connected client: the worker that serves the normal
		// request path only starts running after this constructor returns.
		let bootstrap_client = active_client.read().expect("shared client lock poisoned").clone();
		let jam_std_common::VersionedParameters::V1(parameters) =
			Node::parameters(&*bootstrap_client)
				.await
				.map_err(|error| format!("Unable to fetch the JAM chain parameters: {error}"))?;
		tracing::info!(target: LOG_TARGET, ?parameters, "Applying the JAM chain parameters.");
		parameters
			.apply()
			.map_err(|error| format!("Invalid JAM chain parameters: {error}"))?;

		let client = JamRpcClient::new(to_worker, active_client);
		Ok((Self { client }, worker.run()))
	}

	/// The underlying client, exposing polkajam's full `Node`/`NodeExt` surface.
	pub fn node(&self) -> &JamRpcClient {
		&self.client
	}

	async fn register_block_listener(
		&self,
		register: impl FnOnce(mpsc::Sender<BlockDesc>) -> WorkerMessage,
	) -> Result<BoxStream<'static, BlockDesc>> {
		let (tx, rx) = mpsc::channel(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.client
			.send_to_worker(register(tx))
			.await
			.map_err(|error| Error::Other(error.to_string()))?;
		Ok(rx.boxed())
	}
}

#[async_trait]
impl JamChainSource for JamRpcInterface {
	async fn best_block(&self) -> Result<BlockDesc> {
		self.client.best_block().await
	}

	async fn finalized_block(&self) -> Result<BlockDesc> {
		self.client.finalized_block().await
	}

	async fn best_block_stream(&self) -> Result<BoxStream<'static, BlockDesc>> {
		self.register_block_listener(WorkerMessage::RegisterBestBlockListener).await
	}

	async fn finalized_block_stream(&self) -> Result<BoxStream<'static, BlockDesc>> {
		self.register_block_listener(WorkerMessage::RegisterFinalizedBlockListener)
			.await
	}

	async fn parent(&self, header_hash: HeaderHash) -> Result<BlockDesc> {
		self.client.parent(header_hash).await
	}

	async fn header(&self, header_hash: HeaderHash) -> Result<Vec<u8>> {
		// polkajam's `header` is not in the vendored `Node` trait, so this is a raw call; the node
		// returns the jam-codec encoding of the header as base64 `AnyBytes`.
		let encoded: AnyBytes =
			ClientT::request(&self.client, "header", rpc_params![header_hash]).await?;
		Ok(encoded.0.into())
	}

	async fn state_root(&self, header_hash: HeaderHash) -> Result<StateRootHash> {
		self.client.state_root(header_hash).await
	}

	async fn beefy_root(&self, header_hash: HeaderHash) -> Result<MmrPeakHash> {
		self.client.beefy_root(header_hash).await
	}

	async fn parameters(&self) -> Result<cumulus_jam_interface::VersionedParameters> {
		self.client.parameters().await
	}
}

#[async_trait]
impl JamStateSource for JamRpcInterface {
	async fn state_value(&self, at: HeaderHash, key: StorageKey) -> Result<Option<Vec<u8>>> {
		Ok(self.client.state_value(at, key).await?.map(Into::into))
	}

	async fn state_value_stream(
		&self,
		key: StorageKey,
		finalized: bool,
	) -> Result<BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>> {
		Ok(resubscribing_value_stream(
			format!("stateValue({key:?})"),
			self.client.clone(),
			move |client| Box::pin(client.subscribe_state_value(key, finalized)),
		))
	}

	async fn state_proof(
		&self,
		at: HeaderHash,
		start_key: StorageKey,
		end_key: StorageKey,
		size_limit: u32,
	) -> Result<RangeProof> {
		self.client.state_proof(at, start_key, end_key, size_limit).await
	}

	async fn service_value(
		&self,
		at: HeaderHash,
		service: ServiceId,
		key: &[u8],
	) -> Result<Option<Vec<u8>>> {
		Ok(self.client.service_value(at, service, key).await?.map(Into::into))
	}

	async fn service_preimage(
		&self,
		at: HeaderHash,
		service: ServiceId,
		hash: Hash,
	) -> Result<Option<Vec<u8>>> {
		Ok(self.client.service_preimage(at, service, hash).await?.map(Into::into))
	}

	async fn service_value_stream(
		&self,
		service: ServiceId,
		key: &[u8],
		finalized: bool,
	) -> Result<BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>> {
		let key = key.to_vec();
		Ok(resubscribing_value_stream(
			format!("serviceValue({service}, 0x{})", hex_of(&key)),
			self.client.clone(),
			move |client| {
				let key = key.clone();
				Box::pin(
					async move { client.subscribe_service_value(service, &key, finalized).await },
				)
			},
		))
	}
}

#[async_trait]
impl JamWorkPackageSubmission for JamRpcInterface {
	async fn submit_work_package(
		&self,
		core: CoreIndex,
		package: &WorkPackage,
		extrinsics: Vec<Vec<u8>>,
	) -> Result<()> {
		let extrinsics: Vec<bytes::Bytes> =
			extrinsics.into_iter().map(bytes::Bytes::from).collect();
		let started = Instant::now();
		let result = self.client.submit_work_package(core, package, &extrinsics).await;
		tracing::debug!(
			target: LOG_TARGET,
			core,
			extrinsics = extrinsics.len(),
			duration = ?started.elapsed(),
			?result,
			"Submitted work package.",
		);
		result
	}

	async fn submit_bundle(&self, core: CoreIndex, bundle: Vec<u8>) -> Result<()> {
		self.client.submit_encoded_work_package_bundle(core, bundle.into()).await
	}

	async fn recover_bundle(
		&self,
		report_hash: WorkReportHash,
		assurance_epoch: EpochIndex,
	) -> Result<Vec<u8>> {
		Ok(self.client.recover_bundle(report_hash, assurance_epoch).await?.into())
	}

	async fn work_package_status_stream(
		&self,
		package_hash: WorkPackageHash,
		anchor: HeaderHash,
		finalized: bool,
	) -> Result<BoxStream<'static, WorkPackageStatus>> {
		let client = self.client.clone();
		let stream = async_stream::stream! {
			let subscription = match client
				.subscribe_work_package_status(package_hash, anchor, finalized)
				.await
			{
				Ok(subscription) => subscription,
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						?package_hash,
						?anchor,
						?error,
						"Unable to open work-package status subscription."
					);
					yield WorkPackageStatus::Failed(
						format!("status subscription failed: {error}").into(),
					);
					return;
				},
			};
			let mut subscription = subscription;
			while let Some(update) = subscription.next().await {
				match update {
					Ok(update) => {
						tracing::debug!(
							target: LOG_TARGET,
							?package_hash,
							at = ?update.header_hash,
							slot = update.slot,
							status = ?update.value,
							"Work-package status update.",
						);
						yield update.value;
					},
					Err(error) => {
						tracing::warn!(
							target: LOG_TARGET,
							?package_hash,
							?anchor,
							?error,
							"Work-package status subscription errored."
						);
						yield WorkPackageStatus::Failed(
							format!("status subscription errored: {error}").into(),
						);
						return;
					},
				}
			}
			tracing::debug!(
				target: LOG_TARGET,
				?package_hash,
				"Work-package status subscription ended."
			);
		};
		Ok(stream.boxed())
	}
}

/// A value-following stream that re-subscribes forever: on subscription errors or a dead
/// connection it waits [`RESUBSCRIBE_DELAY`] and opens the subscription again. Used for the
/// long-lived state/service value followers (e.g. the para head).
fn resubscribing_value_stream<Subscribe>(
	what: String,
	client: JamRpcClient,
	subscribe: Subscribe,
) -> BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>
where
	Subscribe: for<'a> Fn(
			&'a JamRpcClient,
		) -> futures::future::BoxFuture<
			'a,
			Result<jam_std_common::ChainSub<'a, Option<bytes::Bytes>>>,
		> + Send
		+ 'static,
{
	let stream = async_stream::stream! {
		loop {
			let mut subscription = match subscribe(&client).await {
				Ok(subscription) => subscription,
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						what,
						?error,
						"Unable to open value subscription; retrying.",
					);
					tokio::time::sleep(RESUBSCRIBE_DELAY).await;
					continue;
				},
			};
			while let Some(update) = subscription.next().await {
				match update {
					Ok(update) => {
						tracing::debug!(
							target: LOG_TARGET,
							what,
							at = ?update.header_hash,
							slot = update.slot,
							value_len = update.value.as_ref().map(|value| value.len()),
							"Value subscription update.",
						);
						yield update.map(|value| value.map(Into::into));
					},
					Err(error) => {
						tracing::warn!(
							target: LOG_TARGET,
							what,
							?error,
							"Value subscription errored; re-subscribing.",
						);
						break;
					},
				}
			}
			tokio::time::sleep(RESUBSCRIBE_DELAY).await;
		}
	};
	stream.boxed()
}

fn hex_of(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::worker::{SharedActiveClient, WorkerMessage};
	use jam_types::base64::{engine::general_purpose::STANDARD, Engine};
	use jsonrpsee_jam::{
		core::client::{ClientBuilder, ReceivedMessage, TransportReceiverT, TransportSenderT},
		ws_client::WsClient,
	};
	use serde_json::{json, value::RawValue, Value};
	use std::sync::{Arc, Mutex, RwLock};
	use tokio::sync::mpsc as tokio_mpsc;

	/// Sending half of an in-memory jsonrpsee transport: records every request the client
	/// sends and hands its id to [`MockReceiver`].
	struct MockSender {
		seen: Arc<Mutex<Vec<Value>>>,
		ids: tokio_mpsc::UnboundedSender<Value>,
	}

	impl TransportSenderT for MockSender {
		type Error = std::io::Error;

		async fn send(&mut self, msg: String) -> std::result::Result<(), Self::Error> {
			let request: Value = serde_json::from_str(&msg)?;
			self.ids.send(request["id"].clone()).map_err(std::io::Error::other)?;
			self.seen.lock().expect("test mutex").push(request);
			Ok(())
		}
	}

	/// Receiving half: answers every request with the same `body` (a `result` or an `error`
	/// member), stamped with the request's id.
	struct MockReceiver {
		ids: tokio_mpsc::UnboundedReceiver<Value>,
		body: Value,
	}

	impl TransportReceiverT for MockReceiver {
		type Error = std::io::Error;

		async fn receive(&mut self) -> std::result::Result<ReceivedMessage, Self::Error> {
			let id =
				self.ids.recv().await.ok_or_else(|| std::io::Error::other("transport closed"))?;
			let mut response = self.body.clone();
			response["jsonrpc"] = json!("2.0");
			response["id"] = id;
			Ok(ReceivedMessage::Text(response.to_string()))
		}
	}

	/// A [`JamRpcInterface`] whose node is the in-memory transport above. A minimal stand-in for
	/// the worker performs each raw request the way [`worker::JamRpcWorker`] does.
	fn interface_answering(body: Value) -> (JamRpcInterface, Arc<Mutex<Vec<Value>>>) {
		let seen = Arc::new(Mutex::new(Vec::new()));
		let (ids_tx, ids_rx) = tokio_mpsc::unbounded_channel();
		let node: Arc<WsClient> = Arc::new(ClientBuilder::default().build_with_tokio(
			MockSender { seen: seen.clone(), ids: ids_tx },
			MockReceiver { ids: ids_rx, body },
		));

		let (to_worker, mut from_client) = tokio_mpsc::channel(8);
		let worker_node = node.clone();
		tokio::spawn(async move {
			while let Some(message) = from_client.recv().await {
				let WorkerMessage::Request(method, params, reply) = message else { continue };
				let response: std::result::Result<Box<RawValue>, _> =
					ClientT::request(&*worker_node, &method, params).await;
				let _ = reply.send(response);
			}
		});

		let active: SharedActiveClient = Arc::new(RwLock::new(node));
		(JamRpcInterface { client: JamRpcClient::new(to_worker, active) }, seen)
	}

	#[tokio::test]
	async fn header_is_a_raw_call_returning_the_encoded_header() {
		let encoded_header: Vec<u8> = (0u8..=255).cycle().take(133).collect();
		let (interface, seen) =
			interface_answering(json!({ "result": STANDARD.encode(&encoded_header) }));
		let header_hash = HeaderHash([0x42; 32]);

		let header = interface.header(header_hash).await.expect("header");

		assert_eq!(header, encoded_header);
		let seen = seen.lock().expect("test mutex");
		assert_eq!(seen.len(), 1, "{seen:?}");
		assert_eq!(seen[0]["method"], "header");
		assert_eq!(seen[0]["params"], json!([STANDARD.encode(header_hash.0)]));
	}

	#[tokio::test]
	async fn header_on_a_node_without_the_method_is_an_error() {
		let (interface, _seen) = interface_answering(
			json!({ "error": { "code": -32601, "message": "Method not found" } }),
		);

		let error = interface
			.header(HeaderHash([0x42; 32]))
			.await
			.expect_err("nodes predating `header` reject the call");

		assert!(matches!(error, Error::Other(_)), "{error:?}");
		assert!(error.to_string().contains("Method not found"), "{error}");
	}
}
