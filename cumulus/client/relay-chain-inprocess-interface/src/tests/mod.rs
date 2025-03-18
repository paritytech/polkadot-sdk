// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use super::*;

use polkadot_primitives::Block as PBlock;
use polkadot_test_client::{
	construct_transfer_extrinsic, BlockBuilderExt, Client, ClientBlockImportExt,
	DefaultTestClientBuilderExt, InitPolkadotBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use sp_consensus::{BlockOrigin, SyncOracle};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

use futures::{executor::block_on, poll, task::Poll};

mod dummy;
use dummy::DummySyncOracle;

fn build_client_backend_and_block() -> (Arc<Client>, PBlock, RelayChainInProcessInterface) {
	let builder = TestClientBuilder::new();
	let backend = builder.backend();
	let client = Arc::new(builder.build());

	let block_builder = client.init_polkadot_block_builder();
	let block = block_builder.build().expect("Finalizes the block").block;
	let dummy_syncing: Arc<dyn SyncOracle + Sync + Send> = Arc::new(DummySyncOracle {});

	let (tx, _rx) = metered::channel(30);
	let mock_handle = Handle::new(tx);
	(
		client.clone(),
		block,
		RelayChainInProcessInterface::new(client, backend, dummy_syncing, mock_handle),
	)
}

#[test]
fn returns_directly_for_available_block() {
	let (client, block, relay_chain_interface) = build_client_backend_and_block();
	let hash = block.hash();

	block_on(client.import(BlockOrigin::Own, block)).expect("Imports the block");

	block_on(async move {
		// Should be ready on the first poll
		assert!(matches!(poll!(relay_chain_interface.wait_for_block(hash)), Poll::Ready(Ok(()))));
	});
}

#[test]
fn resolve_after_block_import_notification_was_received() {
	let (client, block, relay_chain_interface) = build_client_backend_and_block();
	let hash = block.hash();

	block_on(async move {
		let mut future = relay_chain_interface.wait_for_block(hash);
		// As the block is not yet imported, the first poll should return `Pending`
		assert!(poll!(&mut future).is_pending());

		// Import the block that should fire the notification
		client.import(BlockOrigin::Own, block).await.expect("Imports the block");

		// Now it should have received the notification and report that the block was imported
		assert!(matches!(poll!(future), Poll::Ready(Ok(()))));
	});
}

#[test]
fn wait_for_block_time_out_when_block_is_not_imported() {
	let (_, block, relay_chain_interface) = build_client_backend_and_block();
	let hash = block.hash();

	assert!(matches!(
		block_on(relay_chain_interface.wait_for_block(hash)),
		Err(RelayChainError::WaitTimeout(_))
	));
}

#[test]
fn do_not_resolve_after_different_block_import_notification_was_received() {
	let (client, block, relay_chain_interface) = build_client_backend_and_block();
	let hash = block.hash();

	let ext = construct_transfer_extrinsic(
		&client,
		sp_keyring::Sr25519Keyring::Alice,
		sp_keyring::Sr25519Keyring::Bob,
		1000,
	);
	let mut block_builder = client.init_polkadot_block_builder();
	// Push an extrinsic to get a different block hash.
	block_builder.push_polkadot_extrinsic(ext).expect("Push extrinsic");
	let block2 = block_builder.build().expect("Build second block").block;
	let hash2 = block2.hash();

	block_on(async move {
		let mut future = relay_chain_interface.wait_for_block(hash);
		let mut future2 = relay_chain_interface.wait_for_block(hash2);
		// As the block is not yet imported, the first poll should return `Pending`
		assert!(poll!(&mut future).is_pending());
		assert!(poll!(&mut future2).is_pending());

		// Import the block that should fire the notification
		client.import(BlockOrigin::Own, block2).await.expect("Imports the second block");

		// The import notification of the second block should not make this one finish
		assert!(poll!(&mut future).is_pending());
		// Now it should have received the notification and report that the block was imported
		assert!(matches!(poll!(future2), Poll::Ready(Ok(()))));

		client.import(BlockOrigin::Own, block).await.expect("Imports the first block");

		// Now it should be ready
		assert!(matches!(poll!(future), Poll::Ready(Ok(()))));
	});
}
