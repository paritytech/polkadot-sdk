// Copyright (C) Parity Technologies (UK) Ltd.
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
		runtime_api::ParachainHost, Block as PBlock, BlockId, CommittedCandidateReceipt,
		Hash as PHash, Header as PHeader, InboundHrmpMessage, OccupiedCoreAssumption, SessionIndex,
		ValidatorId,
	},
	InboundDownwardMessage, ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use futures::{FutureExt, Stream, StreamExt};
use polkadot_service::{
	CollatorPair, Configuration, FullBackend, FullClient, Handle, NewFull, TaskManager,
};
use sc_cli::SubstrateCli;
use sc_client_api::{
	blockchain::BlockStatus, Backend, BlockchainEvents, HeaderBackend, ImportNotifications,
	StorageProof,
};
use sc_telemetry::TelemetryWorkerHandle;
use sp_api::ProvideRuntimeApi;
use sp_consensus::SyncOracle;
use sp_core::{sp_std::collections::btree_map::BTreeMap, Pair};
use sp_state_machine::{Backend as StateBackend, StorageValue};

/// The timeout in seconds after that the waiting for a block should be aborted.
const TIMEOUT_IN_SECONDS: u64 = 6;

/// Provides an implementation of the [`RelayChainInterface`] using a local in-process relay chain
/// node.
#[derive(Clone)]
pub struct RelayChainInProcessInterface {
	full_client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	sync_oracle: Arc<dyn SyncOracle + Send + Sync>,
	overseer_handle: Handle,
}

impl RelayChainInProcessInterface {
	/// Create a new instance of [`RelayChainInProcessInterface`]
	pub fn new(
		full_client: Arc<FullClient>,
		backend: Arc<FullBackend>,
		sync_oracle: Arc<dyn SyncOracle + Send + Sync>,
		overseer_handle: Handle,
	) -> Self {
		Self { full_client, backend, sync_oracle, overseer_handle }
	}
}

#[async_trait]
impl RelayChainInterface for RelayChainInProcessInterface {
	async fn retrieve_dmq_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		Ok(self.full_client.runtime_api().dmq_contents(relay_parent, para_id)?)
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		Ok(self
			.full_client
			.runtime_api()
			.inbound_hrmp_channels_contents(relay_parent, para_id)?)
	}

	async fn header(&self, block_id: BlockId) -> RelayChainResult<Option<PHeader>> {
		let hash = match block_id {
			BlockId::Hash(hash) => hash,
			BlockId::Number(num) =>
				if let Some(hash) = self.full_client.hash(num)? {
					hash
				} else {
					return Ok(None)
				},
		};
		let header = self.full_client.header(hash)?;

		Ok(header)
	}

	async fn persisted_validation_data(
		&self,
		hash: PHash,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		Ok(self.full_client.runtime_api().persisted_validation_data(
			hash,
			para_id,
			occupied_core_assumption,
		)?)
	}

	async fn candidate_pending_availability(
		&self,
		hash: PHash,
		para_id: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		Ok(self.full_client.runtime_api().candidate_pending_availability(hash, para_id)?)
	}

	async fn session_index_for_child(&self, hash: PHash) -> RelayChainResult<SessionIndex> {
		Ok(self.full_client.runtime_api().session_index_for_child(hash)?)
	}

	async fn validators(&self, hash: PHash) -> RelayChainResult<Vec<ValidatorId>> {
		Ok(self.full_client.runtime_api().validators(hash)?)
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

	async fn finalized_block_hash(&self) -> RelayChainResult<PHash> {
		Ok(self.backend.blockchain().info().finalized_hash)
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		Ok(self.sync_oracle.is_major_syncing())
	}

	fn overseer_handle(&self) -> RelayChainResult<Handle> {
		Ok(self.overseer_handle.clone())
	}

	async fn get_storage_by_key(
		&self,
		relay_parent: PHash,
		key: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		let state = self.backend.state_at(relay_parent)?;
		state.storage(key).map_err(RelayChainError::GenericError)
	}

	async fn prove_read(
		&self,
		relay_parent: PHash,
		relevant_keys: &Vec<Vec<u8>>,
	) -> RelayChainResult<StorageProof> {
		let state_backend = self.backend.state_at(relay_parent)?;

		sp_state_machine::prove_read(state_backend, relevant_keys)
			.map_err(RelayChainError::StateMachineError)
	}

	/// Wait for a given relay chain block in an async way.
	///
	/// The caller needs to pass the hash of a block it waits for and the function will return when
	/// the block is available or an error occurred.
	///
	/// The waiting for the block is implemented as follows:
	///
	/// 1. Get a read lock on the import lock from the backend.
	///
	/// 2. Check if the block is already imported. If yes, return from the function.
	///
	/// 3. If the block isn't imported yet, add an import notification listener.
	///
	/// 4. Poll the import notification listener until the block is imported or the timeout is
	/// fired.
	///
	/// The timeout is set to 6 seconds. This should be enough time to import the block in the
	/// current round and if not, the new round of the relay chain already started anyway.
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
					notification.is_new_best.then_some(notification.header)
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
pub fn check_block_in_chain(
	backend: Arc<FullBackend>,
	client: Arc<FullClient>,
	hash: PHash,
) -> RelayChainResult<BlockCheckStatus> {
	let _lock = backend.get_import_lock().read();

	if backend.blockchain().status(hash)? == BlockStatus::InChain {
		return Ok(BlockCheckStatus::InChain)
	}

	let listener = client.import_notification_stream();

	Ok(BlockCheckStatus::Unknown(listener))
}

/// Build the Polkadot full node using the given `config`.
#[sc_tracing::logging::prefix_logs_with("Relaychain")]
fn build_polkadot_full_node(
	config: Configuration,
	parachain_config: &Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> Result<(NewFull, Option<CollatorPair>), polkadot_service::Error> {
	let (is_parachain_node, maybe_collator_key) = if parachain_config.role.is_authority() {
		let collator_key = CollatorPair::generate().0;
		(polkadot_service::IsParachainNode::Collator(collator_key.clone()), Some(collator_key))
	} else {
		(polkadot_service::IsParachainNode::FullNode, None)
	};

	let relay_chain_full_node = polkadot_service::build_full(
		config,
		polkadot_service::NewFullParams {
			is_parachain_node,
			grandpa_pause: None,
			// Disable BEEFY. It should not be required by the internal relay chain node.
			enable_beefy: false,
			jaeger_agent: None,
			telemetry_worker_handle,

			// Cumulus doesn't spawn PVF workers, so we can disable version checks.
			node_version: None,
			workers_path: None,
			workers_names: None,

			overseer_gen: polkadot_service::RealOverseerGen,
			overseer_message_channel_capacity_override: None,
			malus_finality_delay: None,
			hwbench,
		},
	)?;

	Ok((relay_chain_full_node, maybe_collator_key))
}

/// Builds a relay chain interface by constructing a full relay chain node
pub fn build_inprocess_relay_chain(
	mut polkadot_config: Configuration,
	parachain_config: &Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
	task_manager: &mut TaskManager,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	// This is essentially a hack, but we want to ensure that we send the correct node version
	// to the telemetry.
	polkadot_config.impl_version = polkadot_cli::Cli::impl_version();
	polkadot_config.impl_name = polkadot_cli::Cli::impl_name();

	let (full_node, collator_key) = build_polkadot_full_node(
		polkadot_config,
		parachain_config,
		telemetry_worker_handle,
		hwbench,
	)
	.map_err(|e| RelayChainError::Application(Box::new(e) as Box<_>))?;

	let relay_chain_interface = Arc::new(RelayChainInProcessInterface::new(
		full_node.client,
		full_node.backend,
		full_node.sync_service,
		full_node.overseer_handle.clone().ok_or(RelayChainError::GenericError(
			"Overseer not running in full node.".to_string(),
		))?,
	));

	task_manager.add_child(full_node.task_manager);

	Ok((relay_chain_interface, collator_key))
}

#[cfg(test)]
mod tests {
	use super::*;

	use polkadot_primitives::Block as PBlock;
	use polkadot_test_client::{
		construct_transfer_extrinsic, BlockBuilderExt, Client, ClientBlockImportExt,
		DefaultTestClientBuilderExt, InitPolkadotBlockBuilder, TestClientBuilder,
		TestClientBuilderExt,
	};
	use sp_consensus::{BlockOrigin, SyncOracle};
	use sp_runtime::traits::Block as BlockT;
	use std::sync::Arc;

	use futures::{executor::block_on, poll, task::Poll};

	struct DummyNetwork {}

	impl SyncOracle for DummyNetwork {
		fn is_major_syncing(&self) -> bool {
			unimplemented!("Not needed for test")
		}

		fn is_offline(&self) -> bool {
			unimplemented!("Not needed for test")
		}
	}

	fn build_client_backend_and_block() -> (Arc<Client>, PBlock, RelayChainInProcessInterface) {
		let builder = TestClientBuilder::new();
		let backend = builder.backend();
		let client = Arc::new(builder.build());

		let block_builder = client.init_polkadot_block_builder();
		let block = block_builder.build().expect("Finalizes the block").block;
		let dummy_network: Arc<dyn SyncOracle + Sync + Send> = Arc::new(DummyNetwork {});

		let (tx, _rx) = metered::channel(30);
		let mock_handle = Handle::new(tx);
		(
			client.clone(),
			block,
			RelayChainInProcessInterface::new(client, backend, dummy_network, mock_handle),
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
}
