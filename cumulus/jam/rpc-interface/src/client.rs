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

//! A jsonrpsee client that routes every request through the reconnecting worker.
//!
//! [`JamRpcClient`] implements jsonrpsee's `ClientT`/`SubscriptionClientT`, so polkajam's
//! generated `RpcClient` — and through its blanket impl the whole `Node`/`NodeExt` surface —
//! works on top of it with zero hand-written RPC calls.
//!
//! Requests go through the worker channel and are replayed on reconnect. Subscriptions are
//! opened on the worker's current active client (the same websocket); when the connection dies
//! they end, and the resubscribe logic lives in the callers (`lib.rs`), since only they know the
//! subscription's semantics.

use crate::worker::{RawParams, SharedActiveClient, WorkerMessage};
use futures::channel::oneshot;
use jsonrpsee_jam::{
	core::{
		client::{BatchResponse, ClientT, Subscription, SubscriptionClientT},
		params::BatchRequestBuilder,
		traits::ToRpcParams,
		ClientError,
	},
	ws_client::WsClient,
};
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;
use std::{fmt, sync::Arc};
use tokio::sync::mpsc::Sender as TokioSender;

/// Client handle to the JAM node connection managed by [`crate::worker::JamRpcWorker`].
#[derive(Clone)]
pub struct JamRpcClient {
	to_worker: TokioSender<WorkerMessage>,
	active_client: SharedActiveClient,
}

impl JamRpcClient {
	pub(crate) fn new(
		to_worker: TokioSender<WorkerMessage>,
		active_client: SharedActiveClient,
	) -> Self {
		Self { to_worker, active_client }
	}

	pub(crate) async fn send_to_worker(&self, message: WorkerMessage) -> Result<(), ClientError> {
		self.to_worker
			.send(message)
			.await
			.map_err(|_| ClientError::Custom("JAM RPC worker channel closed".to_string()))
	}

	fn current_client(&self) -> Arc<WsClient> {
		self.active_client.read().expect("shared client lock poisoned").clone()
	}
}

impl fmt::Debug for JamRpcClient {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("JamRpcClient").finish()
	}
}

impl ClientT for JamRpcClient {
	async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), ClientError>
	where
		Params: ToRpcParams + Send,
	{
		self.current_client().notification(method, params).await
	}

	async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, ClientError>
	where
		R: DeserializeOwned,
		Params: ToRpcParams + Send,
	{
		let params = RawParams(params.to_rpc_params()?);
		let (response_sender, response_receiver) = oneshot::channel();
		self.send_to_worker(WorkerMessage::Request(method.to_owned(), params, response_sender))
			.await?;
		let raw: Box<RawValue> = response_receiver
			.await
			.map_err(|_| ClientError::Custom("JAM RPC worker dropped the request".to_string()))??;
		serde_json::from_str(raw.get()).map_err(ClientError::ParseError)
	}

	async fn batch_request<'a, R>(
		&self,
		batch: BatchRequestBuilder<'a>,
	) -> Result<BatchResponse<'a, R>, ClientError>
	where
		R: DeserializeOwned + fmt::Debug + 'a,
	{
		// Batches are not replayed on reconnect; nothing in the `Node` client uses them.
		self.current_client().batch_request(batch).await
	}
}

impl SubscriptionClientT for JamRpcClient {
	async fn subscribe<'a, Notif, Params>(
		&self,
		subscribe_method: &'a str,
		params: Params,
		unsubscribe_method: &'a str,
	) -> Result<Subscription<Notif>, ClientError>
	where
		Params: ToRpcParams + Send,
		Notif: DeserializeOwned,
	{
		self.current_client()
			.subscribe(subscribe_method, params, unsubscribe_method)
			.await
	}

	async fn subscribe_to_method<Notif>(
		&self,
		method: &str,
	) -> Result<Subscription<Notif>, ClientError>
	where
		Notif: DeserializeOwned,
	{
		self.current_client().subscribe_to_method(method).await
	}
}
