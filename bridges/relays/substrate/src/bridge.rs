// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

use crate::error::Error;
use crate::rpc::{self, SubstrateRPC};
use crate::params::{RPCUrlParam, Params};

use futures::{prelude::*, channel::{mpsc, oneshot}, future, select};
use jsonrpsee::{
	raw::client::{RawClient, RawClientError, RawClientEvent, RawClientRequestId, RawClientSubscription},
	transport::{
		TransportClient,
		ws::{WsTransportClient, WsConnecError},
	},
};
use node_primitives::{Hash, Header};
use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use sp_core::Bytes;

type ChainId = Hash;

struct BridgeState {
	channel: mpsc::Sender<Event>,
	locally_finalized_head_on_bridged_chain: Header,
}

struct ChainState {
	current_finalized_head: Header,
	bridges: HashMap<ChainId, BridgeState>,
}

enum Event {
	SubmitExtrinsic(Bytes),
}

struct Chain {
	url: String,
	client: RawClient<WsTransportClient>,
	sender: mpsc::Sender<Event>,
	receiver: mpsc::Receiver<Event>,
	genesis_hash: Hash,
	state: ChainState,
}

async fn init_rpc_connection(url: &RPCUrlParam) -> Result<Chain, Error> {
	let url_str = url.to_string();
	log::debug!("Connecting to {}", url_str);

	// Skip the leading "ws://" and trailing "/".
	let url_without_scheme = &url_str[5..(url_str.len() - 1)];
	let transport = WsTransportClient::new(url_without_scheme)
		.await
		.map_err(|err| Error::WsConnectionError(err.to_string()))?;

	let mut client = RawClient::new(transport);
	let genesis_hash = rpc::genesis_block_hash(&mut client)
		.await
		.map_err(|e| Error::RPCError(e.to_string()))?
		.ok_or_else(|| Error::InvalidChainState(format!(
			"chain with RPC URL {} is missing a genesis block hash",
			url_str,
		)))?;

	let latest_finalized_hash = SubstrateRPC::chain_finalized_head(&mut client)
		.await
		.map_err(|e| Error::RPCError(e.to_string()))?;
	let latest_finalized_header = SubstrateRPC::chain_header(
		&mut client,
		Some(latest_finalized_hash)
	)
		.await
		.map_err(|e| Error::RPCError(e.to_string()))?
		.ok_or_else(|| Error::InvalidChainState(format!(
			"chain {} is missing header for finalized block hash {}",
			genesis_hash, latest_finalized_hash
		)))?;

	let (sender, receiver) = mpsc::channel(0);

	Ok(Chain {
		url: url_str,
		client,
		sender,
		receiver,
		genesis_hash,
		state: ChainState {
			current_finalized_head: latest_finalized_header,
			bridges: HashMap::new(),
		}
	})
}

/// Returns IDs of the bridged chains.
async fn read_bridges(chain: &mut Chain, chain_ids: &[Hash])
	-> Result<Vec<Hash>, Error>
{
	// This should make an RPC call to read this information from the bridge pallet state.
	// For now, just pretend every chain is bridged to every other chain.
	//
	// TODO: The correct thing.
	Ok(
		chain_ids
			.iter()
			.cloned()
			.filter(|&chain_id| chain_id != chain.genesis_hash)
			.collect()
	)
}

pub async fn run_async(
	params: Params,
	exit: Box<dyn Future<Output=()> + Unpin + Send>
) -> Result<(), Error>
{
	let chains = init_chains(&params).await?;

	let (chain_tasks, exit_signals) = chains.into_iter()
		.map(|(chain_id, chain_cell)| {
			let chain = chain_cell.into_inner();
			let (task_exit_signal, task_exit_receiver) = oneshot::channel();
			let task_exit = Box::new(task_exit_receiver.map(|result| {
				result.expect("task_exit_signal is not dropped before send() is called")
			}));
			let chain_task = async_std::task::spawn(async move {
				if let Err(err) = chain_task(chain_id, chain, task_exit).await {
					log::error!("Error in task for chain {}: {}", chain_id, err);
				}
			});
			(chain_task, task_exit_signal)
		})
		.unzip::<_, _, Vec<_>, Vec<_>>();

	async_std::task::spawn(async move {
		exit.await;
		for exit_signal in exit_signals {
			let _ = exit_signal.send(());
		}
	});

	future::join_all(chain_tasks).await;
	Ok(())
}

fn initial_next_events<'a>(chains: &'a HashMap<ChainId, RefCell<Chain>>)
	-> Vec<Pin<Box<dyn Future<Output=Result<(ChainId, RawClientEvent), Error>> + 'a>>>
{
	chains.values()
		.map(|chain_cell| async move {
			let mut chain = chain_cell.borrow_mut();
			let event = chain.client.next_event()
				.await
				.map_err(|err| Error::RPCError(err.to_string()))?;
			Ok((chain.genesis_hash, event))
		})
		.map(|fut| Box::pin(fut) as Pin<Box<dyn Future<Output=_>>>)
		.collect()
}

async fn next_event<'a>(
	next_events: Vec<Pin<Box<dyn Future<Output=Result<(ChainId, RawClientEvent), Error>> + 'a>>>,
	chains: &'a HashMap<ChainId, RefCell<Chain>>,
)
	-> (
		Result<(Hash, RawClientEvent), Error>,
		Vec<Pin<Box<dyn Future<Output=Result<(ChainId, RawClientEvent), Error>> +'a>>>
	)
{
	let (result, _, mut rest) = future::select_all(next_events).await;

	match result {
		Ok((chain_id, _)) => {
			let fut = async move {
				let chain_cell = chains.get(&chain_id)
					.expect("chain must be in the map as a function precondition; qed");
				let mut chain = chain_cell.borrow_mut();
				let event = chain.client.next_event()
					.await
					.map_err(|err| Error::RPCError(err.to_string()))?;
				Ok((chain_id, event))
			};
			rest.push(Box::pin(fut));
		}
		Err(ref err) => log::warn!("error in RPC connection with a chain: {}", err),
	}

	(result, rest)
}

async fn init_chains(params: &Params) -> Result<HashMap<ChainId, RefCell<Chain>>, Error> {
	let chains = future::join_all(params.rpc_urls.iter().map(init_rpc_connection))
		.await
		.into_iter()
		.map(|result| result.map(|chain| (chain.genesis_hash, RefCell::new(chain))))
		.collect::<Result<HashMap<_, _>, _>>()?;

	// TODO: Remove when read_bridges is implemented correctly.
	let chain_ids = chains.keys()
		.cloned()
		.collect::<Vec<_>>();
	// let chain_ids_slice = chain_ids.as_slice();

	for (&chain_id, chain_cell) in chains.iter() {
		let mut chain = chain_cell.borrow_mut();
		for bridged_chain_id in read_bridges(&mut chain, &chain_ids).await? {
			if chain_id == bridged_chain_id {
				log::warn!("chain {} has a bridge to itself", chain_id);
				continue;
			}

			if let Some(bridged_chain_cell) = chains.get(&bridged_chain_id) {
				let bridged_chain = bridged_chain_cell.borrow_mut();

				// TODO: Get this from RPC to runtime API.
				let genesis_head = SubstrateRPC::chain_header(&mut chain.client, chain_id)
					.await
					.map_err(|e| Error::RPCError(e.to_string()))?
					.ok_or_else(|| Error::InvalidChainState(format!(
						"chain {} is missing a genesis block header", chain_id
					)))?;

				let channel = chain.sender.clone();
				chain.state.bridges.insert(bridged_chain_id, BridgeState {
					channel,
					locally_finalized_head_on_bridged_chain: genesis_head,
				});

				// The conditional ensures that we don't log twice per pair of chains.
				if chain_id.as_ref() < bridged_chain_id.as_ref() {
					log::info!("initialized bridge between {} and {}", chain_id, bridged_chain_id);
				}
			}
		}
	}

	Ok(chains)
}

async fn setup_subscriptions(chain: &mut Chain)
	-> Result<(RawClientRequestId, RawClientRequestId), RawClientError<WsConnecError>>
{
	let new_heads_subscription_id = chain.client
		.start_subscription(
			"chain_subscribeNewHeads",
			jsonrpsee::common::Params::None,
		)
		.await
		.map_err(RawClientError::Inner)?;

	let finalized_heads_subscription_id = chain.client
		.start_subscription(
			"chain_subscribeFinalizedHeads",
			jsonrpsee::common::Params::None,
		)
		.await
		.map_err(RawClientError::Inner)?;

	let new_heads_subscription =
		chain.client.subscription_by_id(new_heads_subscription_id)
			.expect("subscription_id was returned from start_subscription above; qed");
	let new_heads_subscription = match new_heads_subscription {
		RawClientSubscription::Active(_) => {}
		RawClientSubscription::Pending(subscription) => {
			subscription.wait().await?;
		}
	};

	let finalized_heads_subscription =
		chain.client.subscription_by_id(finalized_heads_subscription_id)
			.expect("subscription_id was returned from start_subscription above; qed");
	let finalized_heads_subscription = match finalized_heads_subscription {
		RawClientSubscription::Active(subscription) => {}
		RawClientSubscription::Pending(subscription) => {
			subscription.wait().await?;
		}
	};

	Ok((new_heads_subscription_id, finalized_heads_subscription_id))
}

async fn handle_rpc_event(
	chain_id: ChainId,
	chain: &mut Chain,
	event: RawClientEvent,
	new_heads_subscription_id: RawClientRequestId,
	finalized_heads_subscription_id: RawClientRequestId,
) -> Result<(), Error>
{
	match event {
		RawClientEvent::SubscriptionNotif { request_id, result } =>
			if request_id == new_heads_subscription_id {
				let header: Header = serde_json::from_value(result)
					.map_err(Error::SerializationError)?;
				log::info!("Received new head {:?} on chain {}", header, chain_id);
			} else if request_id == finalized_heads_subscription_id {
				let header: Header = serde_json::from_value(result)
					.map_err(Error::SerializationError)?;
				log::info!("Received finalized head {:?} on chain {}", header, chain_id);

				// let old_finalized_head = chain_state.current_finalized_head;
				chain.state.current_finalized_head = header;
				for (bridged_chain_id, bridged_chain) in chain.state.bridges.iter_mut() {
					if bridged_chain.locally_finalized_head_on_bridged_chain.number <
						chain.state.current_finalized_head.number {
						// Craft and submit an extrinsic over RPC
						log::info!("Sending command to submit extrinsic to chain {}", chain_id);
						let mut send_event = bridged_chain.channel
							.send(Event::SubmitExtrinsic(Bytes(Vec::new())))
							.fuse();

						// Continue processing events from other chain tasks while waiting to send
						// event to other chain task in order to prevent deadlocks.
						loop {
							select! {
								result = send_event => {
									result.map_err(Error::ChannelError)?;
									break;
								}
								event = chain.receiver.next().fuse() => {
									let event = event
										.expect("stream will never close as the chain has an mpsc Sender");
									handle_bridge_event(chain_id, &mut chain.client, event)
										.await?;
								}
								// TODO: exit
							}
						}
					}
				}
			} else {
				return Err(Error::RPCError(format!(
					"unexpected subscription response with request ID {:?}", request_id
				)));
			},
		_ => return Err(Error::RPCError(format!(
			"unexpected RPC event from chain {}: {:?}", chain_id, event
		))),
	}
	Ok(())
}

// Let's say this never sends over a channel (ie. cannot block on another task).
async fn handle_bridge_event<R: TransportClient>(
	chain_id: ChainId,
	rpc_client: &mut RawClient<R>,
	event: Event,
) -> Result<(), Error>
{
	match event {
		Event::SubmitExtrinsic(data) => {
			log::info!("Submitting extrinsic to chain {}", chain_id);
			if let Err(err) = SubstrateRPC::author_submit_extrinsic(rpc_client, data).await {
				log::error!("failed to submit extrinsic: {}", err);
			}
		}
	}
	Ok(())
}

async fn chain_task(
	chain_id: ChainId,
	mut chain: Chain,
	exit: impl Future<Output=()> + Unpin + Send
) -> Result<(), Error>
{
	let (new_heads_subscription_id, finalized_heads_subscription_id) =
		setup_subscriptions(&mut chain)
			.await
			.map_err(|e| Error::RPCError(e.to_string()))?;

	let mut exit = exit.fuse();
	loop {
		select! {
			result = chain.client.next_event().fuse() => {
				let event = result.map_err(|e| Error::RPCError(e.to_string()))?;
				handle_rpc_event(
					chain_id,
					&mut chain,
					event,
					new_heads_subscription_id,
					finalized_heads_subscription_id,
				).await?;
			}
			event = chain.receiver.next().fuse() => {
				let event = event
					.expect("stream will never close as the chain has an mpsc Sender");
				handle_bridge_event(chain_id, &mut chain.client, event)
					.await?;
			}
			_ = exit => {
				log::debug!("Received exit signal, shutting down task for chain {}", chain_id);
				break;
			}
		}
	}
	Ok(())
}
