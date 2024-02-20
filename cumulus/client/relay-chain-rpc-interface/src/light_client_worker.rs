// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! This module contains a backend that sends RPC requests to an
//! embedded light client. Even though no networking is involved,
//! we treat the light-client as a normal JsonRPC target.

use futures::{channel::mpsc::Sender, prelude::*, stream::FuturesUnordered};
use jsonrpsee::core::client::{
	Client as JsonRpseeClient, ClientBuilder, ClientT, Error, ReceivedMessage, TransportReceiverT,
	TransportSenderT,
};
use smoldot_light::{ChainId, Client as SmoldotClient, JsonRpcResponses};
use std::{num::NonZeroU32, sync::Arc};
use tokio::sync::mpsc::{channel as tokio_channel, Receiver, Sender as TokioSender};

use cumulus_primitives_core::relay_chain::{
	Block as RelayBlock, BlockNumber as RelayNumber, Hash as RelayHash, Header as RelayHeader,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainResult};

use sp_runtime::generic::SignedBlock;

use sc_rpc_api::chain::ChainApiClient;
use sc_service::SpawnTaskHandle;

use crate::{rpc_client::RpcDispatcherMessage, tokio_platform::TokioPlatform};

const LOG_TARGET: &str = "rpc-light-client-worker";
const MAX_PENDING_REQUESTS: u32 = 128;
const MAX_SUBSCRIPTIONS: u32 = 64;

#[derive(thiserror::Error, Debug)]
enum LightClientError {
	#[error("Error occurred while executing smoldot request: {0}")]
	SmoldotError(String),
	#[error("Nothing returned from json_rpc_responses")]
	EmptyResult,
}

/// Sending adapter allowing JsonRpsee to send messages to smoldot
struct SimpleStringSender {
	inner: SmoldotClient<TokioPlatform, ()>,
	chain_id: ChainId,
}

#[async_trait::async_trait]
impl TransportSenderT for SimpleStringSender {
	type Error = LightClientError;

	async fn send(&mut self, msg: String) -> Result<(), Self::Error> {
		self.inner
			.json_rpc_request(msg, self.chain_id)
			.map_err(|e| LightClientError::SmoldotError(e.to_string()))
	}
}

/// Receiving adapter allowing JsonRpsee to receive messages from smoldot
struct SimpleStringReceiver {
	inner: JsonRpcResponses,
}

#[async_trait::async_trait]
impl TransportReceiverT for SimpleStringReceiver {
	type Error = LightClientError;

	async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
		self.inner
			.next()
			.await
			.map(|message| jsonrpsee::core::client::ReceivedMessage::Text(message))
			.ok_or(LightClientError::EmptyResult)
	}
}

/// Build a smoldot client and add the specified chain spec to it.
pub async fn build_smoldot_client(
	spawner: SpawnTaskHandle,
	chain_spec: &str,
) -> RelayChainResult<(SmoldotClient<TokioPlatform, ()>, ChainId, JsonRpcResponses)> {
	let platform = TokioPlatform::new(spawner);
	let mut client = SmoldotClient::new(platform);

	// Ask the client to connect to a chain.
	let smoldot_light::AddChainSuccess { chain_id, json_rpc_responses } = client
		.add_chain(smoldot_light::AddChainConfig {
			specification: chain_spec,
			json_rpc: smoldot_light::AddChainConfigJsonRpc::Enabled {
				max_pending_requests: NonZeroU32::new(MAX_PENDING_REQUESTS)
					.expect("Constant larger than 0; qed"),
				max_subscriptions: MAX_SUBSCRIPTIONS,
			},
			potential_relay_chains: core::iter::empty(),
			database_content: "",
			user_data: (),
		})
		.map_err(|e| RelayChainError::GenericError(e.to_string()))?;

	Ok((client, chain_id, json_rpc_responses.expect("JSON RPC is enabled; qed")))
}

/// Worker to process incoming [`RpcDispatcherMessage`] requests.
/// On startup, this worker opens subscriptions for imported, best and finalized
/// heads. Incoming notifications are distributed to registered listeners.
pub struct LightClientRpcWorker {
	client_receiver: Receiver<RpcDispatcherMessage>,
	imported_header_listeners: Vec<Sender<RelayHeader>>,
	finalized_header_listeners: Vec<Sender<RelayHeader>>,
	best_header_listeners: Vec<Sender<RelayHeader>>,
	smoldot_client: Arc<JsonRpseeClient>,
}

fn handle_notification(
	maybe_header: Option<Result<RelayHeader, Error>>,
	senders: &mut Vec<Sender<RelayHeader>>,
) -> Result<(), ()> {
	match maybe_header {
		Some(Ok(header)) => {
			crate::rpc_client::distribute_header(header, senders);
			Ok(())
		},
		None => {
			tracing::error!(target: LOG_TARGET, "Subscription closed.");
			Err(())
		},
		Some(Err(error)) => {
			tracing::error!(target: LOG_TARGET, ?error, "Error in RPC subscription.");
			Err(())
		},
	}
}

impl LightClientRpcWorker {
	/// Create new light-client worker.
	///
	/// Returns the worker itself and a channel to send messages.
	pub fn new(
		smoldot_client: smoldot_light::Client<TokioPlatform, ()>,
		json_rpc_responses: JsonRpcResponses,
		chain_id: ChainId,
	) -> (LightClientRpcWorker, TokioSender<RpcDispatcherMessage>) {
		let (tx, rx) = tokio_channel(100);
		let smoldot_adapter_sender = SimpleStringSender { inner: smoldot_client, chain_id };
		let smoldot_adapter_receiver = SimpleStringReceiver { inner: json_rpc_responses };

		// Build jsonrpsee client that will talk to the inprocess smoldot node
		let smoldot_jsonrpsee_client = Arc::new(
			ClientBuilder::default()
				.build_with_tokio(smoldot_adapter_sender, smoldot_adapter_receiver),
		);

		let worker = LightClientRpcWorker {
			client_receiver: rx,
			imported_header_listeners: Default::default(),
			finalized_header_listeners: Default::default(),
			best_header_listeners: Default::default(),
			smoldot_client: smoldot_jsonrpsee_client,
		};
		(worker, tx)
	}

	// Main worker loop.
	//
	// Does the following:
	// 1. Initialize notification streams
	// 2. Enter main loop
	// 	 a. On listening request, register listener for respective notification stream
	// 	 b. On incoming notification, distribute notification to listeners
	// 	 c. On incoming request, forward request to smoldot
	// 	 d. Advance execution of pending requests
	pub async fn run(mut self) {
		let mut pending_requests = FuturesUnordered::new();

		let Ok(mut new_head_subscription) = <JsonRpseeClient as ChainApiClient<
			RelayNumber,
			RelayHash,
			RelayHeader,
			SignedBlock<RelayBlock>,
		>>::subscribe_new_heads(&self.smoldot_client)
		.await
		else {
			tracing::error!(
				target: LOG_TARGET,
				"Unable to initialize new heads subscription"
			);
			return
		};

		let Ok(mut finalized_head_subscription) =
			<JsonRpseeClient as ChainApiClient<
				RelayNumber,
				RelayHash,
				RelayHeader,
				SignedBlock<RelayBlock>,
			>>::subscribe_finalized_heads(&self.smoldot_client)
			.await
		else {
			tracing::error!(
				target: LOG_TARGET,
				"Unable to initialize finalized heads subscription"
			);
			return
		};

		let Ok(mut all_head_subscription) = <JsonRpseeClient as ChainApiClient<
			RelayNumber,
			RelayHash,
			RelayHeader,
			SignedBlock<RelayBlock>,
		>>::subscribe_all_heads(&self.smoldot_client)
		.await
		else {
			tracing::error!(
				target: LOG_TARGET,
				"Unable to initialize all heads subscription"
			);
			return
		};

		loop {
			tokio::select! {
				evt = self.client_receiver.recv() => match evt {
					Some(RpcDispatcherMessage::RegisterBestHeadListener(tx)) => {
						self.best_header_listeners.push(tx);
					},
					Some(RpcDispatcherMessage::RegisterImportListener(tx)) => {
						self.imported_header_listeners.push(tx)
					},
					Some(RpcDispatcherMessage::RegisterFinalizationListener(tx)) => {
						self.finalized_header_listeners.push(tx)
					},
					Some(RpcDispatcherMessage::Request(method, params, response_sender)) => {
						let closure_client = self.smoldot_client.clone();
						tracing::debug!(
							target: LOG_TARGET,
							len = pending_requests.len(),
							method,
							"Request"
						);
						pending_requests.push(async move {
							let response = closure_client.request(&method, params).await;
								tracing::debug!(
									target: LOG_TARGET,
									method,
									?response,
									"Response"
								);
							if let Err(err) = response_sender.send(response) {
								tracing::debug!(
									target: LOG_TARGET,
									?err,
									"Recipient no longer interested in request result"
								);
							};
						});
					},
					None => {
						tracing::error!(target: LOG_TARGET, "RPC client receiver closed. Stopping RPC Worker.");
						return;
					}
				},
				_ = pending_requests.next(), if !pending_requests.is_empty() => {},
				import_event = all_head_subscription.next() => {
					if handle_notification(import_event, &mut self.imported_header_listeners).is_err() {
						return
					}
				},
				best_header_event = new_head_subscription.next() => {
					if handle_notification(best_header_event, &mut self.best_header_listeners).is_err() {
						return
					}
				}
				finalized_event = finalized_head_subscription.next() => {
					if handle_notification(finalized_event, &mut self.finalized_header_listeners).is_err() {
						return
					}
				}
			}
		}
	}
}
