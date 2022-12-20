// Copyright 2022 Parity Technologies (UK) Ltd.
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

use std::{pin::Pin, str::FromStr};

use cumulus_relay_chain_interface::{RelayChainError, RelayChainResult};
use cumulus_relay_chain_rpc_interface::RelayChainRpcClient;
use futures::{Future, Stream, StreamExt};
use polkadot_core_primitives::{Block, BlockId, Hash, Header};
use polkadot_overseer::RuntimeApiSubsystemClient;
use polkadot_service::{AuxStore, HeaderBackend};
use sc_authority_discovery::AuthorityDiscovery;

use sc_network_common::config::MultiaddrWithPeerId;
use sp_api::{ApiError, RuntimeApiInfo};
use sp_blockchain::Info;

const LOG_TARGET: &str = "blockchain-rpc-client";

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
		number: Option<polkadot_service::BlockNumber>,
	) -> Result<Option<Hash>, RelayChainError> {
		self.rpc_client.chain_get_block_hash(number).await
	}
}

// Implementation required by Availability-Distribution subsystem
// but never called in our case.
impl AuxStore for BlockChainRpcClient {
	fn insert_aux<
		'a,
		'b: 'a,
		'c: 'a,
		I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
		D: IntoIterator<Item = &'a &'b [u8]>,
	>(
		&self,
		_insert: I,
		_delete: D,
	) -> sp_blockchain::Result<()> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn get_aux(&self, _key: &[u8]) -> sp_blockchain::Result<Option<Vec<u8>>> {
		unimplemented!("Not supported on the RPC collator")
	}
}

#[async_trait::async_trait]
impl RuntimeApiSubsystemClient for BlockChainRpcClient {
	async fn validators(
		&self,
		at: Hash,
	) -> Result<Vec<polkadot_primitives::v2::ValidatorId>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_validators(at).await?)
	}

	async fn validator_groups(
		&self,
		at: Hash,
	) -> Result<
		(
			Vec<Vec<polkadot_primitives::v2::ValidatorIndex>>,
			polkadot_primitives::v2::GroupRotationInfo<polkadot_core_primitives::BlockNumber>,
		),
		sp_api::ApiError,
	> {
		Ok(self.rpc_client.parachain_host_validator_groups(at).await?)
	}

	async fn availability_cores(
		&self,
		at: Hash,
	) -> Result<
		Vec<polkadot_primitives::v2::CoreState<Hash, polkadot_core_primitives::BlockNumber>>,
		sp_api::ApiError,
	> {
		Ok(self.rpc_client.parachain_host_availability_cores(at).await?)
	}

	async fn persisted_validation_data(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		assumption: polkadot_primitives::v2::OccupiedCoreAssumption,
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
			polkadot_primitives::v2::ValidationCodeHash,
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
		outputs: polkadot_primitives::v2::CandidateCommitments,
	) -> Result<bool, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_check_validation_outputs(at, para_id, outputs)
			.await?)
	}

	async fn session_index_for_child(
		&self,
		at: Hash,
	) -> Result<polkadot_primitives::v2::SessionIndex, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_session_index_for_child(at).await?)
	}

	async fn validation_code(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		assumption: polkadot_primitives::v2::OccupiedCoreAssumption,
	) -> Result<Option<polkadot_primitives::v2::ValidationCode>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_validation_code(at, para_id, assumption).await?)
	}

	async fn candidate_pending_availability(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
	) -> Result<Option<polkadot_primitives::v2::CommittedCandidateReceipt<Hash>>, sp_api::ApiError>
	{
		Ok(self
			.rpc_client
			.parachain_host_candidate_pending_availability(at, para_id)
			.await?)
	}

	async fn candidate_events(
		&self,
		at: Hash,
	) -> Result<Vec<polkadot_primitives::v2::CandidateEvent<Hash>>, sp_api::ApiError> {
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
		validation_code_hash: polkadot_primitives::v2::ValidationCodeHash,
	) -> Result<Option<polkadot_primitives::v2::ValidationCode>, sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_validation_code_by_hash(at, validation_code_hash)
			.await?)
	}

	async fn on_chain_votes(
		&self,
		at: Hash,
	) -> Result<Option<polkadot_primitives::v2::ScrapedOnChainVotes<Hash>>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_on_chain_votes(at).await?)
	}

	async fn session_info(
		&self,
		at: Hash,
		index: polkadot_primitives::v2::SessionIndex,
	) -> Result<Option<polkadot_primitives::v2::SessionInfo>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_session_info(at, index).await?)
	}

	async fn session_info_before_version_2(
		&self,
		at: Hash,
		index: polkadot_primitives::v2::SessionIndex,
	) -> Result<Option<polkadot_primitives::v2::OldV1SessionInfo>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_session_info_before_version_2(at, index).await?)
	}

	async fn submit_pvf_check_statement(
		&self,
		at: Hash,
		stmt: polkadot_primitives::v2::PvfCheckStatement,
		signature: polkadot_primitives::v2::ValidatorSignature,
	) -> Result<(), sp_api::ApiError> {
		Ok(self
			.rpc_client
			.parachain_host_submit_pvf_check_statement(at, stmt, signature)
			.await?)
	}

	async fn pvfs_require_precheck(
		&self,
		at: Hash,
	) -> Result<Vec<polkadot_primitives::v2::ValidationCodeHash>, sp_api::ApiError> {
		Ok(self.rpc_client.parachain_host_pvfs_require_precheck(at).await?)
	}

	async fn validation_code_hash(
		&self,
		at: Hash,
		para_id: cumulus_primitives_core::ParaId,
		assumption: polkadot_primitives::v2::OccupiedCoreAssumption,
	) -> Result<Option<polkadot_primitives::v2::ValidationCodeHash>, sp_api::ApiError> {
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
	) -> std::result::Result<Vec<polkadot_primitives::v2::AuthorityDiscoveryId>, sp_api::ApiError> {
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
			polkadot_primitives::v2::SessionIndex,
			polkadot_primitives::v2::CandidateHash,
			polkadot_primitives::v2::DisputeState<polkadot_primitives::v2::BlockNumber>,
		)>,
		ApiError,
	> {
		Ok(self.rpc_client.parachain_host_staging_get_disputes(at).await?)
	}
}

#[async_trait::async_trait]
impl AuthorityDiscovery<Block> for BlockChainRpcClient {
	async fn authorities(
		&self,
		at: Hash,
	) -> std::result::Result<Vec<polkadot_primitives::v2::AuthorityDiscoveryId>, sp_api::ApiError> {
		let result = self.rpc_client.authority_discovery_authorities(at).await?;
		Ok(result)
	}
}

impl BlockChainRpcClient {
	pub async fn local_listen_addresses(
		&self,
	) -> Result<Vec<MultiaddrWithPeerId>, RelayChainError> {
		let addresses = self.rpc_client.system_local_listen_addresses().await?;
		tracing::debug!(target: LOG_TARGET, ?addresses, "Fetched listen address from RPC node.");

		let mut result_vec = Vec::new();
		for address in addresses {
			match MultiaddrWithPeerId::from_str(&address) {
				Ok(addr) => result_vec.push(addr),
				Err(err) =>
					return Err(RelayChainError::GenericError(format!(
						"Failed to parse a local listen addresses from the RPC node: {}",
						err
					))),
			}
		}

		Ok(result_vec)
	}

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

fn block_local<T>(fut: impl Future<Output = T>) -> T {
	let tokio_handle = tokio::runtime::Handle::current();
	tokio::task::block_in_place(|| tokio_handle.block_on(fut))
}

impl HeaderBackend<Block> for BlockChainRpcClient {
	fn header(
		&self,
		hash: <Block as polkadot_service::BlockT>::Hash,
	) -> sp_blockchain::Result<Option<<Block as polkadot_service::BlockT>::Header>> {
		Ok(block_local(self.rpc_client.chain_get_header(Some(hash)))?)
	}

	fn info(&self) -> Info<Block> {
		let best_header = block_local(self.rpc_client.chain_get_header(None))
			.expect("Unable to get header from relay chain.")
			.unwrap();
		let genesis_hash = block_local(self.rpc_client.chain_get_head(Some(0)))
			.expect("Unable to get header from relay chain.");
		let finalized_head = block_local(self.rpc_client.chain_get_finalized_head())
			.expect("Unable to get finalized head from relay chain.");
		let finalized_header = block_local(self.rpc_client.chain_get_header(Some(finalized_head)))
			.expect("Unable to get finalized header from relay chain.")
			.unwrap();
		Info {
			best_hash: best_header.hash(),
			best_number: best_header.number,
			genesis_hash,
			finalized_hash: finalized_head,
			finalized_number: finalized_header.number,
			finalized_state: None,
			number_leaves: 1,
			block_gap: None,
		}
	}

	fn status(
		&self,
		id: sp_api::BlockId<Block>,
	) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
		let exists = match id {
			BlockId::Hash(hash) => self.header(hash)?.is_some(),
			BlockId::Number(n) => {
				let best_header = block_local(self.rpc_client.chain_get_header(None))?;
				if let Some(best) = best_header {
					n < best.number
				} else {
					false
				}
			},
		};

		if exists {
			Ok(sc_client_api::blockchain::BlockStatus::InChain)
		} else {
			Ok(sc_client_api::blockchain::BlockStatus::Unknown)
		}
	}

	fn number(
		&self,
		hash: <Block as polkadot_service::BlockT>::Hash,
	) -> sp_blockchain::Result<
		Option<<<Block as polkadot_service::BlockT>::Header as polkadot_service::HeaderT>::Number>,
	> {
		let result = block_local(self.rpc_client.chain_get_header(Some(hash)))?
			.map(|maybe_header| maybe_header.number);
		Ok(result)
	}

	fn hash(
		&self,
		number: polkadot_service::NumberFor<Block>,
	) -> sp_blockchain::Result<Option<<Block as polkadot_service::BlockT>::Hash>> {
		Ok(block_local(self.rpc_client.chain_get_block_hash(number.into()))?)
	}
}
