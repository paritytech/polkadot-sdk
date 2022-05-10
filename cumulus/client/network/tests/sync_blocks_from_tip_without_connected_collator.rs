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

#[substrate_test_utils::test(flavor = "multi_thread")]
#[ignore]
async fn sync_blocks_from_tip_without_being_connected_to_a_collator() {
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(100);
	let tokio_handle = tokio::runtime::Handle::current();

	let ws_port = portpicker::pick_unused_port().expect("No free ports");
	// start alice
	let alice = run_relay_chain_validator_node(
		tokio_handle.clone(),
		Alice,
		|| {},
		Vec::new(),
		Some(ws_port),
	);

	// start bob
	let bob = run_relay_chain_validator_node(
		tokio_handle.clone(),
		Bob,
		|| {},
		vec![alice.addr.clone()],
		None,
	);

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
	let eve = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Eve)
		.connect_to_parachain_node(&dave)
		.exclusively_connect_to_registered_parachain_nodes()
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.build()
		.await;

	// run eve as parachain full node that is only connected to dave
	let ferdie = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle, Ferdie)
		.connect_to_parachain_node(&dave)
		.exclusively_connect_to_registered_parachain_nodes()
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.use_external_relay_chain_node_at_port(ws_port)
		.build()
		.await;

	join!(ferdie.wait_for_blocks(7), eve.wait_for_blocks(7));
}
