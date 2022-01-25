// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use cumulus_primitives_core::{
	relay_chain::{
		v1::{CommittedCandidateReceipt, OccupiedCoreAssumption, SessionIndex, ValidatorId},
		v2::ParachainHost,
		Block as PBlock, BlockId, Hash as PHash, Header as PHeader, InboundHrmpMessage,
	},
	InboundDownwardMessage, ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use futures::{FutureExt, Stream, StreamExt};
use parking_lot::Mutex;
use polkadot_client::{ClientHandle, ExecuteWithClient, FullBackend};
use polkadot_service::{
	AuxStore, BabeApi, CollatorPair, Configuration, Handle, NewFull, Role, TaskManager,
};
use sc_client_api::{
	blockchain::BlockStatus, Backend, BlockchainEvents, HeaderBackend, ImportNotifications,
	StorageProof, UsageProvider,
};
use sc_telemetry::TelemetryWorkerHandle;
use sp_api::ProvideRuntimeApi;
use sp_consensus::SyncOracle;
use sp_core::{sp_std::collections::btree_map::BTreeMap, Pair};
use sp_state_machine::{Backend as StateBackend, StorageValue};

/// The timeout in seconds after that the waiting for a block should be aborted.
const TIMEOUT_IN_SECONDS: u64 = 6;

/// Provides an implementation of the [`RelayChainInterface`] using a local in-process relay chain node.
pub struct RelayChainLocal<Client> {
	full_client: Arc<Client>,
	backend: Arc<FullBackend>,
	sync_oracle: Arc<Mutex<Box<dyn SyncOracle + Send + Sync>>>,
	overseer_handle: Option<Handle>,
}

impl<Client> RelayChainLocal<Client> {
	/// Create a new instance of [`RelayChainLocal`]
	pub fn new(
		full_client: Arc<Client>,
		backend: Arc<FullBackend>,
		sync_oracle: Arc<Mutex<Box<dyn SyncOracle + Send + Sync>>>,
		overseer_handle: Option<Handle>,
	) -> Self {
		Self { full_client, backend, sync_oracle, overseer_handle }
	}
}

impl<T> Clone for RelayChainLocal<T> {
	fn clone(&self) -> Self {
		Self {
			full_client: self.full_client.clone(),
			backend: self.backend.clone(),
			sync_oracle: self.sync_oracle.clone(),
			overseer_handle: self.overseer_handle.clone(),
		}
	}
}

#[async_trait]
impl<Client> RelayChainInterface for RelayChainLocal<Client>
where
	Client: ProvideRuntimeApi<PBlock>
		+ BlockchainEvents<PBlock>
		+ AuxStore
		+ UsageProvider<PBlock>
		+ Sync
		+ Send,
	Client::Api: ParachainHost<PBlock> + BabeApi<PBlock>,
{
	async fn retrieve_dmq_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		Ok(self.full_client.runtime_api().dmq_contents_with_context(
			&BlockId::hash(relay_parent),
			sp_core::ExecutionContext::Importing,
			para_id,
		)?)
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		Ok(self.full_client.runtime_api().inbound_hrmp_channels_contents_with_context(
			&BlockId::hash(relay_parent),
			sp_core::ExecutionContext::Importing,
			para_id,
		)?)
	}

	async fn persisted_validation_data(
		&self,
		block_id: &BlockId,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		Ok(self.full_client.runtime_api().persisted_validation_data(
			block_id,
			para_id,
			occupied_core_assumption,
		)?)
	}

	async fn candidate_pending_availability(
		&self,
		block_id: &BlockId,
		para_id: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		Ok(self
			.full_client
			.runtime_api()
			.candidate_pending_availability(block_id, para_id)?)
	}

	async fn session_index_for_child(&self, block_id: &BlockId) -> RelayChainResult<SessionIndex> {
		Ok(self.full_client.runtime_api().session_index_for_child(block_id)?)
	}

	async fn validators(&self, block_id: &BlockId) -> RelayChainResult<Vec<ValidatorId>> {
		Ok(self.full_client.runtime_api().validators(block_id)?)
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let notification_stream = self
			.full_client
			.import_notification_stream()
			.map(|notification| notification.header);
		Ok(Box::pin(notification_stream))
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let notification_stream = self
			.full_client
			.finality_notification_stream()
			.map(|notification| notification.header);
		Ok(Box::pin(notification_stream))
	}

	async fn best_block_hash(&self) -> RelayChainResult<PHash> {
		Ok(self.backend.blockchain().info().best_hash)
	}

	async fn block_status(&self, block_id: BlockId) -> RelayChainResult<BlockStatus> {
		Ok(self.backend.blockchain().status(block_id)?)
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		let mut network = self.sync_oracle.lock();
		Ok(network.is_major_syncing())
	}

	fn overseer_handle(&self) -> RelayChainResult<Option<Handle>> {
		Ok(self.overseer_handle.clone())
	}

	async fn get_storage_by_key(
		&self,
		block_id: &BlockId,
		key: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		let state = self.backend.state_at(*block_id)?;
		state.storage(key).map_err(RelayChainError::GenericError)
	}

	async fn prove_read(
		&self,
		block_id: &BlockId,
		relevant_keys: &Vec<Vec<u8>>,
	) -> RelayChainResult<StorageProof> {
		let state_backend = self.backend.state_at(*block_id)?;

		sp_state_machine::prove_read(state_backend, relevant_keys)
			.map_err(RelayChainError::StateMachineError)
	}

	/// Wait for a given relay chain block in an async way.
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
	async fn wait_for_block(&self, hash: PHash) -> RelayChainResult<()> {
		let mut listener =
			match check_block_in_chain(self.backend.clone(), self.full_client.clone(), hash)? {
				BlockCheckStatus::InChain => return Ok(()),
				BlockCheckStatus::Unknown(listener) => listener,
			};

		let mut timeout = futures_timer::Delay::new(Duration::from_secs(TIMEOUT_IN_SECONDS)).fuse();

		loop {
			futures::select! {
				_ = timeout => return Err(RelayChainError::WaitTimeout(hash)),
				evt = listener.next() => match evt {
					Some(evt) if evt.hash == hash => return Ok(()),
					// Not the event we waited on.
					Some(_) => continue,
					None => return Err(RelayChainError::ImportListenerClosed(hash)),
				}
			}
		}
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let notifications_stream =
			self.full_client
				.import_notification_stream()
				.filter_map(|notification| async move {
					notification.is_new_best.then(|| notification.header)
				});
		Ok(Box::pin(notifications_stream))
	}
}

pub enum BlockCheckStatus {
	/// Block is in chain
	InChain,
	/// Block status is unknown, listener can be used to wait for notification
	Unknown(ImportNotifications<PBlock>),
}

// Helper function to check if a block is in chain.
pub fn check_block_in_chain<Client>(
	backend: Arc<FullBackend>,
	client: Arc<Client>,
	hash: PHash,
) -> RelayChainResult<BlockCheckStatus>
where
	Client: BlockchainEvents<PBlock>,
{
	let _lock = backend.get_import_lock().read();

	let block_id = BlockId::Hash(hash);
	match backend.blockchain().status(block_id)? {
		BlockStatus::InChain => return Ok(BlockCheckStatus::InChain),
		_ => {},
	}

	let listener = client.import_notification_stream();

	Ok(BlockCheckStatus::Unknown(listener))
}

/// Builder for a concrete relay chain interface, created from a full node. Builds
/// a [`RelayChainLocal`] to access relay chain data necessary for parachain operation.
///
/// The builder takes a [`polkadot_client::Client`]
/// that wraps a concrete instance. By using [`polkadot_client::ExecuteWithClient`]
/// the builder gets access to this concrete instance and instantiates a [`RelayChainLocal`] with it.
struct RelayChainLocalBuilder {
	polkadot_client: polkadot_client::Client,
	backend: Arc<FullBackend>,
	sync_oracle: Arc<Mutex<Box<dyn SyncOracle + Send + Sync>>>,
	overseer_handle: Option<Handle>,
}

impl RelayChainLocalBuilder {
	pub fn build(self) -> Arc<dyn RelayChainInterface> {
		self.polkadot_client.clone().execute_with(self)
	}
}

impl ExecuteWithClient for RelayChainLocalBuilder {
	type Output = Arc<dyn RelayChainInterface>;

	fn execute_with_client<Client, Api, Backend>(self, client: Arc<Client>) -> Self::Output
	where
		Client: ProvideRuntimeApi<PBlock>
			+ BlockchainEvents<PBlock>
			+ AuxStore
			+ UsageProvider<PBlock>
			+ 'static
			+ Sync
			+ Send,
		Client::Api: ParachainHost<PBlock> + BabeApi<PBlock>,
	{
		Arc::new(RelayChainLocal::new(client, self.backend, self.sync_oracle, self.overseer_handle))
	}
}

/// Build the Polkadot full node using the given `config`.
#[sc_tracing::logging::prefix_logs_with("Relaychain")]
fn build_polkadot_full_node(
	config: Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
) -> Result<(NewFull<polkadot_client::Client>, CollatorPair), polkadot_service::Error> {
	let is_light = matches!(config.role, Role::Light);
	if is_light {
		Err(polkadot_service::Error::Sub("Light client not supported.".into()))
	} else {
		let collator_key = CollatorPair::generate().0;

		let relay_chain_full_node = polkadot_service::build_full(
			config,
			polkadot_service::IsCollator::Yes(collator_key.clone()),
			None,
			true,
			None,
			telemetry_worker_handle,
			polkadot_service::RealOverseerGen,
		)?;

		Ok((relay_chain_full_node, collator_key))
	}
}

/// Builds a relay chain interface by constructing a full relay chain node
pub fn build_relay_chain_interface(
	polkadot_config: Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
	task_manager: &mut TaskManager,
) -> Result<(Arc<(dyn RelayChainInterface + 'static)>, CollatorPair), polkadot_service::Error> {
	let (full_node, collator_key) =
		build_polkadot_full_node(polkadot_config, telemetry_worker_handle).map_err(
			|e| match e {
				polkadot_service::Error::Sub(x) => x,
				s => format!("{}", s).into(),
			},
		)?;

	let sync_oracle: Box<dyn SyncOracle + Send + Sync> = Box::new(full_node.network.clone());
	let sync_oracle = Arc::new(Mutex::new(sync_oracle));
	let relay_chain_interface_builder = RelayChainLocalBuilder {
		polkadot_client: full_node.client.clone(),
		backend: full_node.backend.clone(),
		sync_oracle,
		overseer_handle: full_node.overseer_handle.clone(),
	};
	task_manager.add_child(full_node.task_manager);

	Ok((relay_chain_interface_builder.build(), collator_key))
}

#[cfg(test)]
mod tests {
	use parking_lot::Mutex;

	use super::*;

	use polkadot_primitives::v1::Block as PBlock;
	use polkadot_test_client::{
		construct_transfer_extrinsic, BlockBuilderExt, Client, ClientBlockImportExt,
		DefaultTestClientBuilderExt, ExecutionStrategy, InitPolkadotBlockBuilder,
		TestClientBuilder, TestClientBuilderExt,
	};
	use sc_service::Arc;
	use sp_consensus::{BlockOrigin, SyncOracle};
	use sp_runtime::traits::Block as BlockT;

	use futures::{executor::block_on, poll, task::Poll};

	struct DummyNetwork {}

	impl SyncOracle for DummyNetwork {
		fn is_major_syncing(&mut self) -> bool {
			unimplemented!("Not needed for test")
		}

		fn is_offline(&mut self) -> bool {
			unimplemented!("Not needed for test")
		}
	}

	fn build_client_backend_and_block() -> (Arc<Client>, PBlock, RelayChainLocal<Client>) {
		let builder =
			TestClientBuilder::new().set_execution_strategy(ExecutionStrategy::NativeWhenPossible);
		let backend = builder.backend();
		let client = Arc::new(builder.build());

		let block_builder = client.init_polkadot_block_builder();
		let block = block_builder.build().expect("Finalizes the block").block;
		let dummy_network: Box<dyn SyncOracle + Sync + Send> = Box::new(DummyNetwork {});

		(
			client.clone(),
			block,
			RelayChainLocal::new(
				client,
				backend.clone(),
				Arc::new(Mutex::new(dummy_network)),
				None,
			),
		)
	}

	#[test]
	fn returns_directly_for_available_block() {
		let (mut client, block, relay_chain_interface) = build_client_backend_and_block();
		let hash = block.hash();

		block_on(client.import(BlockOrigin::Own, block)).expect("Imports the block");

		block_on(async move {
			// Should be ready on the first poll
			assert!(matches!(
				poll!(relay_chain_interface.wait_for_block(hash)),
				Poll::Ready(Ok(()))
			));
		});
	}

	#[test]
	fn resolve_after_block_import_notification_was_received() {
		let (mut client, block, relay_chain_interface) = build_client_backend_and_block();
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
		let (mut client, block, relay_chain_interface) = build_client_backend_and_block();
		let hash = block.hash();

		let ext = construct_transfer_extrinsic(
			&*client,
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
}
