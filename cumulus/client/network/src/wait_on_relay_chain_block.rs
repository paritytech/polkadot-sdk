// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Provides the [`WaitOnRelayChainBlock`] type.

use futures::{future::ready, Future, FutureExt, StreamExt};
use polkadot_primitives::v1::{Block as PBlock, Hash as PHash};
use sc_client_api::{
	blockchain::{self, BlockStatus, HeaderBackend},
	Backend, BlockchainEvents,
};
use sp_runtime::{generic::BlockId, traits::HashFor};
use std::{sync::Arc, time::Duration};

/// The timeout in seconds after that the waiting for a block should be aborted.
const TIMEOUT_IN_SECONDS: u64 = 6;

/// Custom error type used by [`WaitOnRelayChainBlock`].
#[derive(Debug, derive_more::Display)]
pub enum Error {
	#[display(
		fmt = "Timeout while waiting for relay-chain block `{}` to be imported.",
		_0
	)]
	Timeout(PHash),
	#[display(
		fmt = "Import listener closed while waiting for relay-chain block `{}` to be imported.",
		_0
	)]
	ImportListenerClosed(PHash),
	#[display(
		fmt = "Blockchain returned an error while waiting for relay-chain block `{}` to be imported: {:?}",
		_0,
		_1
	)]
	BlockchainError(PHash, blockchain::Error),
}

/// A helper to wait for a given relay chain block in an async way.
///
/// The caller needs to pass the hash of a block it waits for and the function will return when the
/// block is available or an error occurred.
///
/// The waiting for the block is implemented as follows:
///
/// 1. Get a read lock on the import lock from the backend.
///
/// 2. Check if the block is already imported. If yes, return from the function.
///
/// 3. If the block isn't imported yet, add an import notification listener.
///
/// 4. Poll the import notification listener until the block is imported or the timeout is fired.
///
/// The timeout is set to 6 seconds. This should be enough time to import the block in the current
/// round and if not, the new round of the relay chain already started anyway.
pub struct WaitOnRelayChainBlock<B, BCE> {
	block_chain_events: Arc<BCE>,
	backend: Arc<B>,
}

impl<B, BCE> Clone for WaitOnRelayChainBlock<B, BCE> {
	fn clone(&self) -> Self {
		Self {
			backend: self.backend.clone(),
			block_chain_events: self.block_chain_events.clone(),
		}
	}
}

impl<B, BCE> WaitOnRelayChainBlock<B, BCE> {
	/// Creates a new instance of `Self`.
	pub fn new(backend: Arc<B>, block_chain_events: Arc<BCE>) -> Self {
		Self {
			backend,
			block_chain_events,
		}
	}
}

impl<B, BCE> WaitOnRelayChainBlock<B, BCE>
where
	B: Backend<PBlock>,
	BCE: BlockchainEvents<PBlock>,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<B, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
{
	pub fn wait_on_relay_chain_block(
		&self,
		hash: PHash,
	) -> impl Future<Output = Result<(), Error>> {
		let _lock = self.backend.get_import_lock().read();
		match self.backend.blockchain().status(BlockId::Hash(hash)) {
			Ok(BlockStatus::InChain) => {
				return ready(Ok(())).boxed();
			}
			Err(err) => return ready(Err(Error::BlockchainError(hash, err))).boxed(),
			_ => {}
		}

		let mut listener = self.block_chain_events.import_notification_stream();
		// Now it is safe to drop the lock, even when the block is now imported, it should show
		// up in our registered listener.
		drop(_lock);

		let mut timeout = futures_timer::Delay::new(Duration::from_secs(TIMEOUT_IN_SECONDS)).fuse();

		async move {
			loop {
				futures::select! {
					_ = timeout => return Err(Error::Timeout(hash)),
					evt = listener.next() => match evt {
						Some(evt) if evt.hash == hash => return Ok(()),
						// Not the event we waited on.
						Some(_) => continue,
						None => return Err(Error::ImportListenerClosed(hash)),
					}
				}
			}
		}
		.boxed()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use polkadot_test_client::{
		construct_transfer_extrinsic, BlockBuilderExt, Client, ClientBlockImportExt,
		DefaultTestClientBuilderExt, ExecutionStrategy, FullBackend, InitPolkadotBlockBuilder,
		TestClientBuilder, TestClientBuilderExt,
	};
	use sp_consensus::BlockOrigin;
	use sp_runtime::traits::Block as BlockT;

	use futures::{executor::block_on, poll, task::Poll};

	fn build_client_backend_and_block() -> (Arc<Client>, Arc<FullBackend>, PBlock) {
		let builder =
			TestClientBuilder::new().set_execution_strategy(ExecutionStrategy::NativeWhenPossible);
		let backend = builder.backend();
		let client = Arc::new(builder.build());

		let block_builder = client.init_polkadot_block_builder();
		let block = block_builder.build().expect("Finalizes the block").block;

		(client, backend, block)
	}

	#[test]
	fn returns_directly_for_available_block() {
		let (mut client, backend, block) = build_client_backend_and_block();
		let hash = block.hash();

		client
			.import(BlockOrigin::Own, block)
			.expect("Imports the block");

		let wait = WaitOnRelayChainBlock::new(backend, client);

		block_on(async move {
			// Should be ready on the first poll
			assert!(matches!(
				poll!(wait.wait_on_relay_chain_block(hash)),
				Poll::Ready(Ok(()))
			));
		});
	}

	#[test]
	fn resolve_after_block_import_notification_was_received() {
		let (mut client, backend, block) = build_client_backend_and_block();
		let hash = block.hash();

		let wait = WaitOnRelayChainBlock::new(backend, client.clone());

		block_on(async move {
			let mut future = wait.wait_on_relay_chain_block(hash);
			// As the block is not yet imported, the first poll should return `Pending`
			assert!(poll!(&mut future).is_pending());

			// Import the block that should fire the notification
			client
				.import(BlockOrigin::Own, block)
				.expect("Imports the block");

			// Now it should have received the notification and report that the block was imported
			assert!(matches!(poll!(future), Poll::Ready(Ok(()))));
		});
	}

	#[test]
	fn wait_for_block_time_out_when_block_is_not_imported() {
		let (client, backend, block) = build_client_backend_and_block();
		let hash = block.hash();

		let wait = WaitOnRelayChainBlock::new(backend, client.clone());

		assert!(matches!(
			block_on(wait.wait_on_relay_chain_block(hash)),
			Err(Error::Timeout(_))
		));
	}

	#[test]
	fn do_not_resolve_after_different_block_import_notification_was_received() {
		let (mut client, backend, block) = build_client_backend_and_block();
		let hash = block.hash();

		let ext = construct_transfer_extrinsic(
			&*client,
			sp_keyring::Sr25519Keyring::Alice,
			sp_keyring::Sr25519Keyring::Bob,
			1000,
		);
		let mut block_builder = client.init_polkadot_block_builder();
		// Push an extrinsic to get a different block hash.
		block_builder
			.push_polkadot_extrinsic(ext)
			.expect("Push extrinsic");
		let block2 = block_builder.build().expect("Build second block").block;
		let hash2 = block2.hash();

		let wait = WaitOnRelayChainBlock::new(backend, client.clone());

		block_on(async move {
			let mut future = wait.wait_on_relay_chain_block(hash);
			let mut future2 = wait.wait_on_relay_chain_block(hash2);
			// As the block is not yet imported, the first poll should return `Pending`
			assert!(poll!(&mut future).is_pending());
			assert!(poll!(&mut future2).is_pending());

			// Import the block that should fire the notification
			client
				.import(BlockOrigin::Own, block2)
				.expect("Imports the second block");

			// The import notification of the second block should not make this one finish
			assert!(poll!(&mut future).is_pending());
			// Now it should have received the notification and report that the block was imported
			assert!(matches!(poll!(future2), Poll::Ready(Ok(()))));

			client
				.import(BlockOrigin::Own, block)
				.expect("Imports the first block");

			// Now it should be ready
			assert!(matches!(poll!(future), Poll::Ready(Ok(()))));
		});
	}
}
