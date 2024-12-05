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

use std::{collections::BTreeMap, pin::Pin, sync::Arc};

use polkadot_overseer::prometheus::PrometheusError;
use sc_client_api::StorageProof;

use futures::Stream;

use async_trait::async_trait;
use jsonrpsee_core::Error as JsonRpcError;
use parity_scale_codec::Error as CodecError;
use sp_api::ApiError;

use cumulus_primitives_core::relay_chain::BlockId;
pub use cumulus_primitives_core::{
	relay_chain::{
		CommittedCandidateReceipt, Hash as PHash, Header as PHeader, InboundHrmpMessage,
		OccupiedCoreAssumption, SessionIndex, ValidatorId,
	},
	InboundDownwardMessage, ParaId, PersistedValidationData,
};
pub use polkadot_overseer::Handle as OverseerHandle;
pub use sp_state_machine::StorageValue;

pub type RelayChainResult<T> = Result<T, RelayChainError>;

#[derive(thiserror::Error, Debug)]
pub enum RelayChainError {
	#[error("Error occured while calling relay chain runtime: {0}")]
	ApiError(#[from] ApiError),
	#[error("Timeout while waiting for relay-chain block `{0}` to be imported.")]
	WaitTimeout(PHash),
	#[error("Import listener closed while waiting for relay-chain block `{0}` to be imported.")]
	ImportListenerClosed(PHash),
	#[error(
		"Blockchain returned an error while waiting for relay-chain block `{0}` to be imported: {1}"
	)]
	WaitBlockchainError(PHash, sp_blockchain::Error),
	#[error("Blockchain returned an error: {0}")]
	BlockchainError(#[from] sp_blockchain::Error),
	#[error("State machine error occured: {0}")]
	StateMachineError(Box<dyn sp_state_machine::Error>),
	#[error("Unable to call RPC method '{0}'")]
	RpcCallError(String),
	#[error("RPC Error: '{0}'")]
	JsonRpcError(#[from] JsonRpcError),
	#[error("Unable to communicate with RPC worker: {0}")]
	WorkerCommunicationError(String),
	#[error("Scale codec deserialization error: {0}")]
	DeserializationError(CodecError),
	#[error(transparent)]
	Application(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
	#[error("Prometheus error: {0}")]
	PrometheusError(#[from] PrometheusError),
	#[error("Unspecified error occured: {0}")]
	GenericError(String),
}

impl From<RelayChainError> for ApiError {
	fn from(r: RelayChainError) -> Self {
		sp_api::ApiError::Application(Box::new(r))
	}
}

impl From<CodecError> for RelayChainError {
	fn from(e: CodecError) -> Self {
		RelayChainError::DeserializationError(e)
	}
}

impl From<RelayChainError> for sp_blockchain::Error {
	fn from(r: RelayChainError) -> Self {
		sp_blockchain::Error::Application(Box::new(r))
	}
}

impl<T: std::error::Error + Send + Sync + 'static> From<Box<T>> for RelayChainError {
	fn from(r: Box<T>) -> Self {
		RelayChainError::Application(r)
	}
}

/// Trait that provides all necessary methods for interaction between collator and relay chain.
#[async_trait]
pub trait RelayChainInterface: Send + Sync {
	/// Fetch a storage item by key.
	async fn get_storage_by_key(
		&self,
		relay_parent: PHash,
		key: &[u8],
	) -> RelayChainResult<Option<StorageValue>>;

	/// Fetch a vector of current validators.
	async fn validators(&self, block_id: PHash) -> RelayChainResult<Vec<ValidatorId>>;

	/// Get the hash of the current best block.
	async fn best_block_hash(&self) -> RelayChainResult<PHash>;

	/// Fetch the block header of a given hash or height, if it exists.
	async fn header(&self, block_id: BlockId) -> RelayChainResult<Option<PHeader>>;

	/// Get the hash of the finalized block.
	async fn finalized_block_hash(&self) -> RelayChainResult<PHash>;

	/// Returns the whole contents of the downward message queue for the parachain we are collating
	/// for.
	///
	/// Returns `None` in case of an error.
	async fn retrieve_dmq_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>>;

	/// Returns channels contents for each inbound HRMP channel addressed to the parachain we are
	/// collating for.
	///
	/// Empty channels are also included.
	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>>;

	/// Yields the persisted validation data for the given `ParaId` along with an assumption that
	/// should be used if the para currently occupies a core.
	///
	/// Returns `None` if either the para is not registered or the assumption is `Freed`
	/// and the para already occupies a core.
	async fn persisted_validation_data(
		&self,
		block_id: PHash,
		para_id: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>>;

	/// Get the receipt of a candidate pending availability. This returns `Some` for any paras
	/// assigned to occupied cores in `availability_cores` and `None` otherwise.
	async fn candidate_pending_availability(
		&self,
		block_id: PHash,
		para_id: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>>;

	/// Returns the session index expected at a child of the block.
	async fn session_index_for_child(&self, block_id: PHash) -> RelayChainResult<SessionIndex>;

	/// Get a stream of import block notifications.
	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>>;

	/// Get a stream of new best block notifications.
	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>>;

	/// Wait for a block with a given hash in the relay chain.
	///
	/// This method returns immediately on error or if the block is already
	/// reported to be in chain. Otherwise, it waits for the block to arrive.
	async fn wait_for_block(&self, hash: PHash) -> RelayChainResult<()>;

	/// Get a stream of finality notifications.
	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>>;

	/// Whether the synchronization service is undergoing major sync.
	/// Returns true if so.
	async fn is_major_syncing(&self) -> RelayChainResult<bool>;

	/// Get a handle to the overseer.
	fn overseer_handle(&self) -> RelayChainResult<OverseerHandle>;

	/// Generate a storage read proof.
	async fn prove_read(
		&self,
		relay_parent: PHash,
		relevant_keys: &Vec<Vec<u8>>,
	) -> RelayChainResult<StorageProof>;
}

#[async_trait]
impl<T> RelayChainInterface for Arc<T>
where
	T: RelayChainInterface + ?Sized,
{
	async fn retrieve_dmq_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		(**self).retrieve_dmq_contents(para_id, relay_parent).await
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		(**self).retrieve_all_inbound_hrmp_channel_contents(para_id, relay_parent).await
	}

	async fn persisted_validation_data(
		&self,
		block_id: PHash,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		(**self)
			.persisted_validation_data(block_id, para_id, occupied_core_assumption)
			.await
	}

	async fn candidate_pending_availability(
		&self,
		block_id: PHash,
		para_id: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		(**self).candidate_pending_availability(block_id, para_id).await
	}

	async fn session_index_for_child(&self, block_id: PHash) -> RelayChainResult<SessionIndex> {
		(**self).session_index_for_child(block_id).await
	}

	async fn validators(&self, block_id: PHash) -> RelayChainResult<Vec<ValidatorId>> {
		(**self).validators(block_id).await
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		(**self).import_notification_stream().await
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		(**self).finality_notification_stream().await
	}

	async fn best_block_hash(&self) -> RelayChainResult<PHash> {
		(**self).best_block_hash().await
	}

	async fn finalized_block_hash(&self) -> RelayChainResult<PHash> {
		(**self).finalized_block_hash().await
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		(**self).is_major_syncing().await
	}

	fn overseer_handle(&self) -> RelayChainResult<OverseerHandle> {
		(**self).overseer_handle()
	}

	async fn get_storage_by_key(
		&self,
		relay_parent: PHash,
		key: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		(**self).get_storage_by_key(relay_parent, key).await
	}

	async fn prove_read(
		&self,
		relay_parent: PHash,
		relevant_keys: &Vec<Vec<u8>>,
	) -> RelayChainResult<StorageProof> {
		(**self).prove_read(relay_parent, relevant_keys).await
	}

	async fn wait_for_block(&self, hash: PHash) -> RelayChainResult<()> {
		(**self).wait_for_block(hash).await
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		(**self).new_best_notification_stream().await
	}

	async fn header(&self, block_id: BlockId) -> RelayChainResult<Option<PHeader>> {
		(**self).header(block_id).await
	}
}
