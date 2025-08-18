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

use crate::common::{types::ParachainClient, ConstructNodeRuntimeApi, NodeBlock};
use parachains_common::Hash;
use sc_network::{
	config::FullNetworkConfiguration, service::traits::NetworkService, NetworkBackend,
};
use sc_service::{Configuration, TaskManager};
use sc_statement_store::Store;
use std::sync::Arc;

/// Helper function to setup the statement store in `NodeSpec::start_node`.
///
/// Functions are tailored for internal usage, types are unnecessary opinionated for usage in
/// `NodeSpec::start_node`.

/// Build the statement handler prototype. Register the notification protocol in the network
/// configuration.
pub(crate) fn new_statement_handler_proto<
	Block: NodeBlock,
	RuntimeApi,
	Net: NetworkBackend<Block, Hash>,
>(
	client: &ParachainClient<Block, RuntimeApi>,
	parachain_config: &Configuration,
	metrics: &sc_network::NotificationMetrics,
	net_config: &mut FullNetworkConfiguration<Block, Hash, Net>,
) -> sc_network_statement::StatementHandlerPrototype {
	let (statement_handler_proto, statement_config) =
		sc_network_statement::StatementHandlerPrototype::new::<_, _, Net>(
			client.chain_info().genesis_hash,
			parachain_config.chain_spec.fork_id(),
			metrics.clone(),
			Arc::clone(&net_config.peer_store_handle()),
		);
	net_config.add_notification_protocol(statement_config);
	statement_handler_proto
}

/// Build the statement store, spawn the tasks.
pub(crate) fn build_statement_store<
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
>(
	parachain_config: &Configuration,
	task_manager: &mut TaskManager,
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	network: Arc<dyn NetworkService + 'static>,
	sync_service: Arc<sc_network_sync::service::syncing_service::SyncingService<Block>>,
	local_keystore: Arc<sc_keystore::LocalKeystore>,
	statement_handler_proto: sc_network_statement::StatementHandlerPrototype,
) -> sc_service::error::Result<Arc<Store>> {
	let statement_store = sc_statement_store::Store::new_shared(
		&parachain_config.data_path,
		Default::default(),
		client,
		local_keystore,
		parachain_config.prometheus_registry(),
		&task_manager.spawn_handle(),
	)
	.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;
	let statement_protocol_executor = {
		let spawn_handle = task_manager.spawn_handle();
		Box::new(move |fut| {
			spawn_handle.spawn("network-statement-validator", Some("networking"), fut);
		})
	};
	let statement_handler = statement_handler_proto.build(
		network,
		sync_service,
		statement_store.clone(),
		parachain_config.prometheus_registry(),
		statement_protocol_executor,
	)?;
	task_manager.spawn_handle().spawn(
		"network-statement-handler",
		Some("networking"),
		statement_handler.run(),
	);

	Ok(statement_store)
}
