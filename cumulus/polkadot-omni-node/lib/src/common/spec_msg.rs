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

//! Speculative messaging in `NodeSpec::start_node`.
//!
//! Every node with `--enable-spec-msg` archives the own chain's sends and serves them over
//! `/spec-msg/exchange/1`. A collator also runs the receiver: it fetches from the source chains'
//! static peers (`--spec-msg-peer`), feeds the messaging inherent and assembles the lifts of each
//! collation. All of it stays idle while the runtime has no `SpecMsgApi`.

use crate::common::{types::ParachainClient, ConstructNodeRuntimeApi, NodeBlock};
use cumulus_client_spec_msg::{
	exchange_protocol_config, run_archiver, run_receiver, run_request_handler, Archive,
	AuxArchiveStore, Budget, NetworkTransport, Receiver, Transport,
};
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::RelayChainInterface;
use parachains_common_types::Hash;
use sc_network::{
	config::{FullNetworkConfiguration, IncomingRequest, MultiaddrWithPeerId},
	service::traits::NetworkService,
	NetworkBackend, NetworkRequest,
};
use sc_service::TaskManager;
use std::{collections::BTreeMap, sync::Arc};

/// Speculative messaging configuration.
#[derive(Clone, Debug, Default)]
pub struct SpecMsgConfig {
	/// The collators to fetch each source chain's messages from, in the order to try them.
	pub peers: BTreeMap<ParaId, Vec<MultiaddrWithPeerId>>,
}

/// The receiver a collator feeds its inherent and collations from.
pub(crate) type SpecMsgReceiver = Arc<Receiver<Hash, dyn Transport>>;

/// Register `/spec-msg/exchange/1` in the network configuration. Returns the inbound requests
/// for [`start`].
pub(crate) fn register_protocol<Block: NodeBlock, Net: NetworkBackend<Block, Hash>>(
	net_config: &mut FullNetworkConfiguration<Block, Hash, Net>,
) -> async_channel::Receiver<IncomingRequest> {
	let (config, requests) = exchange_protocol_config::<Block, Net>();
	net_config.add_request_response_protocol(config);
	requests
}

/// Spawn the archiver and the request handler and, on a collator, the receiver.
pub(crate) fn start<Block, RuntimeApi>(
	config: &SpecMsgConfig,
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	network: Arc<dyn NetworkService>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	task_manager: &TaskManager,
	requests: async_channel::Receiver<IncomingRequest>,
	para_id: ParaId,
	collator: bool,
) -> Option<SpecMsgReceiver>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	let archive =
		Arc::new(parking_lot::RwLock::new(Archive::open(AuxArchiveStore::new(client.clone()))));
	let spawner = task_manager.spawn_handle();
	spawner.spawn_blocking(
		"spec-msg-archiver",
		Some("spec-msg"),
		run_archiver::<Block, _, _>(client.clone(), archive.clone()),
	);
	spawner.spawn_blocking(
		"spec-msg-request-handler",
		Some("spec-msg"),
		run_request_handler(archive, requests),
	);
	if !collator {
		return None;
	}

	let mut peers = BTreeMap::<ParaId, Vec<_>>::new();
	for (source, addresses) in &config.peers {
		for address in addresses {
			network.add_known_address(address.peer_id, address.multiaddr.clone());
			peers.entry(*source).or_default().push(address.peer_id);
		}
	}
	let network: Arc<dyn NetworkRequest + Send + Sync> = network;
	let transport: Arc<dyn Transport> = Arc::new(NetworkTransport::new(network, peers));
	let receiver: SpecMsgReceiver = Arc::new(Receiver::new(para_id, Budget::default(), transport));
	spawner.spawn(
		"spec-msg-receiver",
		Some("spec-msg"),
		run_receiver::<Block, _, _>(client, relay_chain_interface, receiver.clone()),
	);
	Some(receiver)
}
