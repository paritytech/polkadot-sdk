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

#[substrate_test_utils::test]
#[ignore]
async fn sync_blocks_from_tip_without_being_connected_to_a_collator() {
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(100);
	let tokio_handle = tokio::runtime::Handle::current();

	// start alice
	let alice = run_relay_chain_validator_node(tokio_handle.clone(), Alice, || {}, Vec::new());

	// start bob
	let bob =
		run_relay_chain_validator_node(tokio_handle.clone(), Bob, || {}, vec![alice.addr.clone()]);

	// register parachain
	alice
		.register_parachain(
			para_id,
			cumulus_test_service::runtime::WASM_BINARY
				.expect("You need to build the WASM binary to run this test!")
				.to_vec(),
			initial_head_data(para_id),
		)
		.await
		.unwrap();

	// run charlie as parachain collator
	let charlie =
		cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build()
			.await;

	// run dave as parachain full node
	let dave = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Dave)
		.connect_to_parachain_node(&charlie)
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.build()
		.await;

	// run eve as parachain full node that is only connected to dave
	let eve = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle, Eve)
		.connect_to_parachain_node(&dave)
		.exclusively_connect_to_registered_parachain_nodes()
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.build()
		.await;

	eve.wait_for_blocks(7).await;

	join!(
		alice.task_manager.clean_shutdown(),
		bob.task_manager.clean_shutdown(),
		charlie.task_manager.clean_shutdown(),
		dave.task_manager.clean_shutdown(),
		eve.task_manager.clean_shutdown(),
	);
}
