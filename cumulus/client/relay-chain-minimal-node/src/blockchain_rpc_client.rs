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

use std::pin::Pin;

use cumulus_relay_chain_interface::{RelayChainError, RelayChainResult};
use cumulus_relay_chain_rpc_interface::RelayChainRpcClient;
use futures::{Stream, StreamExt};
use polkadot_core_primitives::{Block, BlockNumber, Hash, Header};
use polkadot_overseer::RuntimeApiSubsystemClient;
use polkadot_primitives::{
	slashing,
	vstaging::{AsyncBackingParams, BackingState},
};
use sc_authority_discovery::{AuthorityDiscovery, Error as AuthorityDiscoveryError};
use sp_api::{ApiError, RuntimeApiInfo};

#[derive(Clone)]
pub struct BlockChainRpcClient {
	rpc_client: RelayChainRpcClient,
}

impl BlockChainRpcClient {
	pub fn new(rpc_client: RelayChainRpcClient) -> Self {
		Self { rpc_client }
	}

	pub async fn chain_get_header(
		&self,
		hash: Option<Hash>,
	) -> Result<Option<Header>, RelayChainError> {
		self.rpc_client.chain_get_header(hash).await
	}

	pub async fn block_get_hash(
		&self,
		number: Option<BlockNumber>,
	) -> Result<Option<Hash>, RelayChainError> {
		self.rpc_client.chain_get_block_hash(number).await
	}
}

#[async_trait::async_trait]
impl RuntimeApiSubsystemClient for BlockChainRpcClient {
	async fn validators(
		&self,
		at: Hash,
	) -> Result<Vec<polkadot_primitives::ValidatorId>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_validators(at).await?)
	}

	async fn validator_groups(
		&self,
		at: Hash,
	) -> Result<
		(
			Vec<Vec<polkadot_primitives::ValidatorIndex>>,
			polkadot_primitives::GroupRotationInfo<polkadot_core_primitives::BlockNumber>,
		),
		sp_api::ApiError,
	> {
		Ok(self.rpc_client.parachain_host_validator_groups(at).await?)
	}

	async fn availability_cores(
		&self,
		at: Hash,
	) -> Result<
		Vec<polkadot_primitives::CoreState<Hash, polkadot_core_primitives::BlockNumber>>,
		sp_api::ApiError,
	> {
		Ok(self.rpc_client.parachain_host_availability_cores(at).await?)
	}

	async fn persisted_validation_data(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		assumption: polkadot_primitives::OccupiedCoreAssumption,
	) -> Result<
		Option<
			cumulus_primitives_core::PersistedValidationData<
				Hash,
				polkadot_core_primitives::BlockNumber,
			>,
		>,
		sp_api::ApiError,
	> {
		Ok(self
			.rpc_client
			.parachain_host_persisted_validation_data(at, para_id, assumption)
			.await?)
	}

	async fn assumed_validation_data(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		expected_persisted_validation_data_hash: Hash,
	) -> Result<
		Option<(
			cumulus_primitives_core::PersistedValidationData<
				Hash,
				polkadot_core_primitives::BlockNumber,
			>,
			polkadot_primitives::ValidationCodeHash,
		)>,
		sp_api::ApiError,
	> {
		Ok(self
			.rpc_client
			.parachain_host_assumed_validation_data(
				at,
				para_id,
				expected_persisted_validation_data_hash,
			)
			.await?)
	}

	async fn check_validation_outputs(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		outputs: polkadot_primitives::CandidateCommitments,
	) -> Result<bool, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_check_validation_outputs(at, para_id, outputs)
			.await?)
	}

	async fn session_index_for_child(
		&self,
		at: Hash,
	) -> Result<polkadot_primitives::SessionIndex, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_session_index_for_child(at).await?)
	}

	async fn validation_code(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		assumption: polkadot_primitives::OccupiedCoreAssumption,
	) -> Result<Option<polkadot_primitives::ValidationCode>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_validation_code(at, para_id, assumption).await?)
	}

	async fn candidate_pending_availability(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
	) -> Result<Option<polkadot_primitives::CommittedCandidateReceipt<Hash>>, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_candidate_pending_availability(at, para_id)
			.await?)
	}

	async fn candidate_events(
		&self,
		at: Hash,
	) -> Result<Vec<polkadot_primitives::CandidateEvent<Hash>>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_candidate_events(at).await?)
	}

	async fn dmq_contents(
		&self,
		at: Hash,
		recipient: cumulus_primitives_core::ParaId,
	) -> Result<
		Vec<cumulus_primitives_core::InboundDownwardMessage<polkadot_core_primitives::BlockNumber>>,
		sp_api::ApiError,
	> {
		Ok(self.rpc_client.parachain_host_dmq_contents(recipient, at).await?)
	}

	async fn inbound_hrmp_channels_contents(
		&self,
		at: Hash,
		recipient: cumulus_primitives_core::ParaId,
	) -> Result<
		std::collections::BTreeMap<
			cumulus_primitives_core::ParaId,
			Vec<
				polkadot_core_primitives::InboundHrmpMessage<polkadot_core_primitives::BlockNumber>,
			>,
		>,
		sp_api::ApiError,
	> {
		Ok(self
			.rpc_client
			.parachain_host_inbound_hrmp_channels_contents(recipient, at)
			.await?)
	}

	async fn validation_code_by_hash(
		&self,
		at: Hash,
		validation_code_hash: polkadot_primitives::ValidationCodeHash,
	) -> Result<Option<polkadot_primitives::ValidationCode>, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_validation_code_by_hash(at, validation_code_hash)
			.await?)
	}

	async fn on_chain_votes(
		&self,
		at: Hash,
	) -> Result<Option<polkadot_primitives::ScrapedOnChainVotes<Hash>>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_on_chain_votes(at).await?)
	}

	async fn session_info(
		&self,
		at: Hash,
		index: polkadot_primitives::SessionIndex,
	) -> Result<Option<polkadot_primitives::SessionInfo>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_session_info(at, index).await?)
	}

	async fn session_executor_params(
		&self,
		at: Hash,
		session_index: polkadot_primitives::SessionIndex,
	) -> Result<Option<polkadot_primitives::ExecutorParams>, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_session_executor_params(at, session_index)
			.await?)
	}

	async fn submit_pvf_check_statement(
		&self,
		at: Hash,
		stmt: polkadot_primitives::PvfCheckStatement,
		signature: polkadot_primitives::ValidatorSignature,
	) -> Result<(), sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_submit_pvf_check_statement(at, stmt, signature)
			.await?)
	}

	async fn pvfs_require_precheck(
		&self,
		at: Hash,
	) -> Result<Vec<polkadot_primitives::ValidationCodeHash>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_pvfs_require_precheck(at).await?)
	}

	async fn validation_code_hash(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		assumption: polkadot_primitives::OccupiedCoreAssumption,
	) -> Result<Option<polkadot_primitives::ValidationCodeHash>, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_validation_code_hash(at, para_id, assumption)
			.await?)
	}

	async fn current_epoch(&self, at: Hash) -> Result<sp_consensus_babe::Epoch, sp_api::ApiError> {
		Ok(self.rpc_client.babe_api_current_epoch(at).await?)
	}

	async fn authorities(
		&self,
		at: Hash,
	) -> std::result::Result<Vec<polkadot_primitives::AuthorityDiscoveryId>, sp_api::ApiError> {
		Ok(self.rpc_client.authority_discovery_authorities(at).await?)
	}

	async fn api_version_parachain_host(&self, at: Hash) -> Result<Option<u32>, sp_api::ApiError> {
		let api_id = <dyn polkadot_primitives::runtime_api::ParachainHost<Block>>::ID;
		Ok(self.rpc_client.runtime_version(at).await.map(|v| v.api_version(&api_id))?)
	}

	async fn disputes(
		&self,
		at: Hash,
	) -> Result<
		Vec<(
			polkadot_primitives::SessionIndex,
			polkadot_primitives::CandidateHash,
			polkadot_primitives::DisputeState<polkadot_primitives::BlockNumber>,
		)>,
		ApiError,
	> {
		Ok(self.rpc_client.parachain_host_disputes(at).await?)
	}

	async fn unapplied_slashes(
		&self,
		at: Hash,
	) -> Result<
		Vec<(
			polkadot_primitives::SessionIndex,
			polkadot_primitives::CandidateHash,
			slashing::PendingSlashes,
		)>,
		ApiError,
	> {
		Ok(self.rpc_client.parachain_host_unapplied_slashes(at).await?)
	}

	async fn key_ownership_proof(
		&self,
		at: Hash,
		validator_id: polkadot_primitives::ValidatorId,
	) -> Result<Option<slashing::OpaqueKeyOwnershipProof>, ApiError> {
		Ok(self.rpc_client.parachain_host_key_ownership_proof(at, validator_id).await?)
	}

	async fn submit_report_dispute_lost(
		&self,
		at: Hash,
		dispute_proof: slashing::DisputeProof,
		key_ownership_proof: slashing::OpaqueKeyOwnershipProof,
	) -> Result<Option<()>, ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_submit_report_dispute_lost(at, dispute_proof, key_ownership_proof)
			.await?)
	}

	async fn minimum_backing_votes(
		&self,
		at: Hash,
		session_index: polkadot_primitives::SessionIndex,
	) -> Result<u32, ApiError> {
		Ok(self.rpc_client.parachain_host_minimum_backing_votes(at, session_index).await?)
	}

	async fn staging_async_backing_params(&self, at: Hash) -> Result<AsyncBackingParams, ApiError> {
		Ok(self.rpc_client.parachain_host_staging_async_backing_params(at).await?)
	}

	async fn staging_para_backing_state(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
	) -> Result<Option<BackingState>, ApiError> {
		Ok(self.rpc_client.parachain_host_staging_para_backing_state(at, para_id).await?)
	}
}

#[async_trait::async_trait]
impl AuthorityDiscovery<Block> for BlockChainRpcClient {
	async fn authorities(
		&self,
		at: Hash,
	) -> std::result::Result<Vec<polkadot_primitives::AuthorityDiscoveryId>, sp_api::ApiError> {
		let result = self.rpc_client.authority_discovery_authorities(at).await?;
		Ok(result)
	}

	async fn best_hash(&self) -> std::result::Result<Hash, AuthorityDiscoveryError> {
		self.block_get_hash(None)
			.await
			.ok()
			.flatten()
			.ok_or_else(|| AuthorityDiscoveryError::BestBlockFetchingError)
	}
}

impl BlockChainRpcClient {
	pub async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = Header> + Send>>> {
		Ok(self.rpc_client.get_imported_heads_stream()?.boxed())
	}

	pub async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = Header> + Send>>> {
		Ok(self.rpc_client.get_finalized_heads_stream()?.boxed())
	}
}
