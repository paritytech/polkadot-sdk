// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use cumulus_primitives_core::ParaId;
use cumulus_test_service::{initial_head_data, run_relay_chain_validator_node, Keyring::*};
use futures::join;
use sc_service::TaskExecutor;
use sc_client_api::client::BlockchainEvents;
use futures::StreamExt;
use sp_api::ProvideRuntimeApi;
use cumulus_test_runtime::GetUpgradeDetection;
use sp_runtime::generic::BlockId;

#[substrate_test_utils::test]
async fn test_runtime_upgrade(task_executor: TaskExecutor) {
	let mut builder = sc_cli::LoggerBuilder::new("runtime=debug");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(100);

	// start alice
	let alice = run_relay_chain_validator_node(task_executor.clone(), Alice, || {}, vec![]);

	// start bob
	let bob =
		run_relay_chain_validator_node(task_executor.clone(), Bob, || {}, vec![alice.addr.clone()]);

	// register parachain
	alice
		.register_parachain(
			para_id,
			cumulus_test_runtime::WASM_BINARY
				.expect("You need to build the WASM binary to run this test!")
				.to_vec(),
			initial_head_data(para_id),
		)
		.await
		.unwrap();

	// run cumulus charlie (a parachain collator)
	let charlie =
		cumulus_test_service::TestNodeBuilder::new(para_id, task_executor.clone(), Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build()
			.await;

	// run cumulus dave (a parachain full node) and wait for it to sync some blocks
	let dave = cumulus_test_service::TestNodeBuilder::new(para_id, task_executor.clone(), Dave)
		.connect_to_parachain_node(&charlie)
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.build()
		.await;

	let mut import_notification_stream = charlie.client.import_notification_stream();

	while let Some(notification) = import_notification_stream.next().await {
		if notification.is_new_best {
			let res = charlie.client.runtime_api()
				.has_upgraded(&BlockId::Hash(notification.hash));
			if matches!(res, Ok(false)) {
				break;
			}
		}
	}

	// schedule runtime upgrade
	charlie.schedule_upgrade(cumulus_test_runtime_upgrade::WASM_BINARY.unwrap().to_vec())
		.await
		.unwrap();

	while let Some(notification) = import_notification_stream.next().await {
		if notification.is_new_best {
			let res = charlie.client.runtime_api()
				.has_upgraded(&BlockId::Hash(notification.hash));
			if res.unwrap_or(false) {
				break;
			}
		}
	}

	join!(
		alice.task_manager.clean_shutdown(),
		bob.task_manager.clean_shutdown(),
		charlie.task_manager.clean_shutdown(),
		dave.task_manager.clean_shutdown(),
	);
}
