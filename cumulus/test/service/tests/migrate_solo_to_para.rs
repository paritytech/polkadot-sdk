// Copyright 2020-2021 Parity Technologies (UK) Ltd.
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

//! Test migration from parachain A to parachain B by returning the header of parachain B.
//!
//! This can be seen as a test of the fundamentals of the solo to parachain migration use case.
//! The prerequisite is to have a running solo chain and a running parachain. The idea is that
//! the solo chain is being stopped at a given point and sends its last header to the running parachain.
//! The parachain will return this header as part of the validation phase on the relay chain to enact
//! this header as its current latest state. As the old running parachain doesn't know this header, it will
//! stop to produce new blocks. However, the old solo chain can now produce blocks using the parachain slot.
//! (Be aware, that this is just a highlevel description and some parts are omitted.)

use codec::Encode;
use cumulus_primitives_core::ParaId;
use cumulus_test_service::{initial_head_data, run_relay_chain_validator_node, Keyring::*};
use sc_client_api::{BlockBackend, UsageProvider};
use sp_runtime::generic::BlockId;

#[substrate_test_utils::test]
#[ignore]
async fn test_migrate_solo_to_para() {
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(100);

	let tokio_handle = tokio::runtime::Handle::current();

	// start alice
	let alice =
		run_relay_chain_validator_node(tokio_handle.clone(), Alice, || {}, Vec::new(), None);

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
			cumulus_test_runtime::WASM_BINARY
				.expect("You need to build the WASM binary to run this test!")
				.to_vec(),
			initial_head_data(para_id),
		)
		.await
		.unwrap();

	// run the parachain that will be used to return the header of the solo chain.
	let parachain =
		cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build()
			.await;

	// run the solo chain (in our case this is also already a parachain, but as it has a different genesis it will not produce any blocks.)
	let solo = cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle, Dave)
		.enable_collator()
		.connect_to_relay_chain_nodes(vec![&alice, &bob])
		// Set some random value in the genesis state to create a different genesis hash.
		.update_storage_parachain(|| {
			sp_io::storage::set(b"test", b"test");
		})
		.build()
		.await;

	parachain.wait_for_blocks(2).await;

	// Ensure that both chains have a different genesis hash.
	assert_ne!(
		parachain.client.block_hash(0).ok().flatten().unwrap(),
		solo.client.block_hash(0).ok().flatten().unwrap(),
	);

	let solo_chain_header = solo.client.header(&BlockId::Number(0)).ok().flatten().unwrap();

	// Send the transaction to set the custom header, aka the header of the solo chain.
	parachain
		.send_extrinsic(
			cumulus_test_runtime::TestPalletCall::set_custom_validation_head_data {
				custom_header: solo_chain_header.encode(),
			},
			Alice,
		)
		.await
		.unwrap();

	// Wait until the solo chain produced a block now as a parachain.
	solo.wait_for_blocks(1).await;

	let parachain_best = parachain.client.usage_info().chain.best_number;

	// Wait for some more blocks and check that the old parachain doesn't produced/imported any new blocks.
	solo.wait_for_blocks(2).await;
	assert_eq!(parachain_best, parachain.client.usage_info().chain.best_number);
}
