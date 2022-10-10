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
use cumulus_test_service::{initial_head_data, Keyring::*};
use futures::join;
use std::sync::Arc;

/// Tests the PoV recovery.
///
/// If there is a block of the parachain included/backed by the relay chain that isn't circulated in
/// the parachain network, we need to recover the PoV from the relay chain. Using this PoV we can
/// recover the block, import it and share it with the other nodes of the parachain network.
#[substrate_test_utils::test(flavor = "multi_thread")]
#[ignore]
async fn pov_recovery() {
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(100);
	let tokio_handle = tokio::runtime::Handle::current();

	// Start alice
	let ws_port = portpicker::pick_unused_port().expect("No free ports");
	let alice = cumulus_test_service::run_relay_chain_validator_node(
		tokio_handle.clone(),
		Alice,
		|| {},
		Vec::new(),
		Some(ws_port),
	);

	// Start bob
	let bob = cumulus_test_service::run_relay_chain_validator_node(
		tokio_handle.clone(),
		Bob,
		|| {},
		vec![alice.addr.clone()],
		None,
	);

	// Register parachain
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

	// Run charlie as parachain collator
	let charlie =
		cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.wrap_announce_block(|_| {
				// Never announce any block
				Arc::new(|_, _| {})
			})
			.build()
			.await;

	// Run dave as parachain collator and eve as parachain full node
	//
	// They will need to recover the pov blocks through availability recovery.
	let dave = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Dave)
		.enable_collator()
		.use_null_consensus()
		.connect_to_parachain_node(&charlie)
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.wrap_announce_block(|_| {
			// Never announce any block
			Arc::new(|_, _| {})
		})
		.build()
		.await;

	let eve = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Eve)
		.use_null_consensus()
		.connect_to_parachain_node(&charlie)
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.wrap_announce_block(|_| {
			// Never announce any block
			Arc::new(|_, _| {})
		})
		.build()
		.await;

	// Run ferdie as parachain RPC collator and one as parachain RPC full node
	//
	// They will need to recover the pov blocks through availability recovery.
	let ferdie = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Ferdie)
		.use_null_consensus()
		.connect_to_parachain_node(&charlie)
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.use_external_relay_chain_node_at_port(ws_port)
		.wrap_announce_block(|_| {
			// Never announce any block
			Arc::new(|_, _| {})
		})
		.build()
		.await;

	let one = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle, One)
		.enable_collator()
		.use_null_consensus()
		.connect_to_parachain_node(&charlie)
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		.use_external_relay_chain_node_at_port(ws_port)
		.wrap_announce_block(|_| {
			// Never announce any block
			Arc::new(|_, _| {})
		})
		.build()
		.await;

	join!(
		dave.wait_for_blocks(7),
		eve.wait_for_blocks(7),
		ferdie.wait_for_blocks(7),
		one.wait_for_blocks(7)
	);
}
