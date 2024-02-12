// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! A RPC handler to create sync states for light clients.
//!
//! Currently only usable with BABE + GRANDPA.
//!
//! # Usage
//!
//! To use the light sync state, it needs to be added as an extension to the chain spec:
//!
//! ```
//! use sc_sync_state_rpc::LightSyncStateExtension;
//!
//! #[derive(Default, Clone, serde::Serialize, serde::Deserialize, sc_chain_spec::ChainSpecExtension)]
//! #[serde(rename_all = "camelCase")]
//! pub struct Extensions {
//!    light_sync_state: LightSyncStateExtension,
//! }
//!
//! type ChainSpec = sc_chain_spec::GenericChainSpec<(), Extensions>;
//! ```
//!
//! If the [`LightSyncStateExtension`] is not added as an extension to the chain spec,
//! the [`SyncState`] will fail at instantiation.

#![deny(unused_crate_dependencies)]

use jsonrpsee::{
	core::async_trait,
	proc_macros::rpc,
	types::{ErrorObject, ErrorObjectOwned},
};

use sc_client_api::{ProofProvider, StorageData};
use sc_consensus_babe::{BabeWorkerHandle, Error as BabeError};
use sp_api::StorageProof;
use sp_blockchain::HeaderBackend;
use sp_core::storage::well_known_keys;
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::sync::Arc;

type SharedAuthoritySet<TBl> =
	sc_consensus_grandpa::SharedAuthoritySet<<TBl as BlockT>::Hash, NumberFor<TBl>>;

/// Error type used by this crate.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error<Block: BlockT> {
	#[error(transparent)]
	Blockchain(#[from] sp_blockchain::Error),

	#[error("Failed to load the block weight for block {0:?}")]
	LoadingBlockWeightFailed(Block::Hash),

	#[error("Failed to load the BABE epoch data: {0}")]
	LoadingEpochDataFailed(BabeError<Block>),

	#[error("JsonRpc error: {0}")]
	JsonRpc(String),

	#[error(
		"The light sync state extension is not provided by the chain spec. \
		Read the `sc-sync-state-rpc` crate docs on how to do this!"
	)]
	LightSyncStateExtensionNotFound,

	#[error(
		"The checkpoint extension is not provided by the chain spec. \
		Read the `sc-sync-state-rpc` crate docs on how to do this!"
	)]
	CheckpointExtensionNotFound,
}

impl<Block: BlockT> From<Error<Block>> for ErrorObjectOwned {
	fn from(error: Error<Block>) -> Self {
		let message = match error {
			Error::JsonRpc(s) => s,
			_ => error.to_string(),
		};
		ErrorObject::owned(1, message, None::<()>)
	}
}

/// Serialize the given `val` by encoding it with SCALE codec and serializing it as hex.
fn serialize_encoded<S: serde::Serializer, T: codec::Encode>(
	val: &T,
	s: S,
) -> Result<S::Ok, S::Error> {
	let encoded = StorageData(val.encode());
	serde::Serialize::serialize(&encoded, s)
}

/// The light sync state extension.
///
/// This represents a JSON serialized [`LightSyncState`]. It is required to be added to the
/// chain-spec as an extension.
pub type LightSyncStateExtension = Option<serde_json::Value>;

/// Hardcoded information that allows light clients to sync quickly.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct LightSyncState<Block: BlockT> {
	/// The header of the best finalized block.
	#[serde(serialize_with = "serialize_encoded")]
	pub finalized_block_header: <Block as BlockT>::Header,
	/// The epoch changes tree for babe.
	#[serde(serialize_with = "serialize_encoded")]
	pub babe_epoch_changes: sc_consensus_epochs::EpochChangesFor<Block, sc_consensus_babe::Epoch>,
	/// The babe weight of the finalized block.
	pub babe_finalized_block_weight: sc_consensus_babe::BabeBlockWeight,
	/// The authority set for grandpa.
	#[serde(serialize_with = "serialize_encoded")]
	pub grandpa_authority_set:
		sc_consensus_grandpa::AuthoritySet<<Block as BlockT>::Hash, NumberFor<Block>>,
}

/// The checkpoint extension.
///
/// This represents a [`Checkpoint`]. It is required to be added to the
/// chain-spec as an extension.
pub type CheckpointExtension = Option<SerdePassThrough<serde_json::Value>>;

/// A serde wrapper that passes through the given value.
///
/// This is introduced to distinguish between the `LightSyncStateExtension`
/// and `CheckpointExtension` extension types.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct SerdePassThrough<T>(T);

/// Checkpoint information that allows light clients to sync quickly.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Checkpoint<Block: BlockT> {
	/// The header of the best finalized block.
	#[serde(serialize_with = "serialize_encoded")]
	pub header: <Block as BlockT>::Header,
	/// The epoch changes tree for babe.
	#[serde(serialize_with = "serialize_encoded")]
	pub call_proof: StorageProof,
}

/// An api for sync state RPC calls.
#[rpc(client, server)]
pub trait SyncStateApi<B: BlockT> {
	/// Returns the JSON serialized chainspec running the node, with a sync state.
	#[method(name = "sync_state_genSyncSpec")]
	async fn system_gen_sync_spec(&self, raw: bool) -> Result<serde_json::Value, Error<B>>;
}

/// An api for sync state RPC calls.
pub struct SyncState<Block: BlockT, Client> {
	chain_spec: Box<dyn sc_chain_spec::ChainSpec>,
	client: Arc<Client>,
	shared_authority_set: SharedAuthoritySet<Block>,
	babe_worker_handle: BabeWorkerHandle<Block>,
}

impl<Block, Client> SyncState<Block, Client>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + sc_client_api::AuxStore + ProofProvider<Block> + 'static,
{
	/// Create a new sync state RPC helper.
	pub fn new(
		chain_spec: Box<dyn sc_chain_spec::ChainSpec>,
		client: Arc<Client>,
		shared_authority_set: SharedAuthoritySet<Block>,
		babe_worker_handle: BabeWorkerHandle<Block>,
	) -> Result<Self, Error<Block>> {
		if sc_chain_spec::get_extension::<CheckpointExtension>(chain_spec.extensions()).is_none() {
			return Err(Error::<Block>::CheckpointExtensionNotFound)
		}
		if sc_chain_spec::get_extension::<LightSyncStateExtension>(chain_spec.extensions())
			.is_none()
		{
			return Err(Error::<Block>::LightSyncStateExtensionNotFound)
		}

		Ok(Self { chain_spec, client, shared_authority_set, babe_worker_handle })
	}

	async fn build_sync_state(&self) -> Result<LightSyncState<Block>, Error<Block>> {
		let epoch_changes = self
			.babe_worker_handle
			.epoch_data()
			.await
			.map_err(Error::LoadingEpochDataFailed)?;

		let finalized_hash = self.client.info().finalized_hash;
		let finalized_header = self
			.client
			.header(finalized_hash)?
			.ok_or_else(|| sp_blockchain::Error::MissingHeader(finalized_hash.to_string()))?;

		let finalized_block_weight =
			sc_consensus_babe::aux_schema::load_block_weight(&*self.client, finalized_hash)?
				.ok_or(Error::LoadingBlockWeightFailed(finalized_hash))?;

		Ok(LightSyncState {
			finalized_block_header: finalized_header,
			babe_epoch_changes: epoch_changes,
			babe_finalized_block_weight: finalized_block_weight,
			grandpa_authority_set: self.shared_authority_set.clone_inner(),
		})
	}

	fn build_checkpoint(&self) -> Result<Checkpoint<Block>, sp_blockchain::Error> {
		let finalized_hash = self.client.info().finalized_hash;
		let finalized_header = self
			.client
			.header(finalized_hash)?
			.ok_or_else(|| sp_blockchain::Error::MissingHeader(finalized_hash.to_string()))?;

		let call_proof = generate_checkpoint_proof(&self.client, finalized_hash)?;

		Ok(Checkpoint { header: finalized_header, call_proof })
	}
}

#[async_trait]
impl<Block, Backend> SyncStateApiServer<Block> for SyncState<Block, Backend>
where
	Block: BlockT,
	Backend: HeaderBackend<Block> + sc_client_api::AuxStore + ProofProvider<Block> + 'static,
{
	async fn system_gen_sync_spec(&self, raw: bool) -> Result<serde_json::Value, Error<Block>> {
		// Build data to pass to the chainSpec as extensions.
		// TODO: Both these states could be cached to avoid recomputation.
		let current_sync_state = self.build_sync_state().await?;
		let checkpoint_state = self.build_checkpoint()?;

		let mut chain_spec = self.chain_spec.cloned_box();

		// Populate the LightSyncState extension.
		let extension = sc_chain_spec::get_extension_mut::<LightSyncStateExtension>(
			chain_spec.extensions_mut(),
		)
		.ok_or(Error::<Block>::LightSyncStateExtensionNotFound)?;
		let val = serde_json::to_value(&current_sync_state)
			.map_err(|e| Error::<Block>::JsonRpc(e.to_string()))?;
		*extension = Some(val);

		// Populate the Checkpoint extension.
		let extension =
			sc_chain_spec::get_extension_mut::<CheckpointExtension>(chain_spec.extensions_mut())
				.ok_or(Error::<Block>::CheckpointExtensionNotFound)?;
		let val = serde_json::to_value(&checkpoint_state)
			.map_err(|e| Error::<Block>::JsonRpc(e.to_string()))?;
		*extension = Some(SerdePassThrough(val));

		let json_str = chain_spec.as_json(raw).map_err(|e| Error::<Block>::JsonRpc(e))?;
		serde_json::from_str(&json_str).map_err(|e| Error::<Block>::JsonRpc(e.to_string()))
	}
}

/// The runtime functions we'd like to prove in the storage proof.
const RUNTIME_FUNCTIONS_TO_PROVE: [&str; 5] = [
	"BabeApi_current_epoch",
	"BabeApi_next_epoch",
	"BabeApi_configuration",
	"GrandpaApi_grandpa_authorities",
	"GrandpaApi_current_set_id",
];

/// The checkpoint proof is a single storage proof that helps the lightclient
/// synchronize to the head of the chain faster.
///
/// The lightclient trusts this chekpoint after verifing the proof.
/// With the verified proof, the lightclient is able to reconstruct what was
/// previously called as `lightSyncState`.
///
/// The checkpoint proof consist of the following proofs merged together:
/// - `:code` and `:heappages` storage proofs
/// - `BabeApi_current_epoch`, `BabeApi_next_epoch`, `BabeApi_configuration`,
///   `GrandpaApi_grandpa_authorities`, and `GrandpaApi_current_set_id` call proofs
pub fn generate_checkpoint_proof<Client, Block>(
	client: &Arc<Client>,
	at: Block::Hash,
) -> sp_blockchain::Result<StorageProof>
where
	Block: BlockT + 'static,
	Client: ProofProvider<Block> + 'static,
{
	// Extract only the proofs.
	let mut proofs = RUNTIME_FUNCTIONS_TO_PROVE
		.iter()
		.map(|func| Ok(client.execution_proof(at, func, Default::default())?.1))
		.collect::<Result<Vec<_>, sp_blockchain::Error>>()?;

	// Fetch the `:code` and `:heap_pages` in one go.
	let code_and_heap = client.read_proof(
		at,
		&mut [well_known_keys::CODE, well_known_keys::HEAP_PAGES].iter().map(|v| *v),
	)?;
	proofs.push(code_and_heap);

	Ok(StorageProof::merge(proofs))
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_block_builder::BlockBuilderBuilder;
	use sc_client_api::{StorageKey, StorageProvider};
	use sp_consensus::BlockOrigin;
	use std::collections::HashMap;
	use substrate_test_runtime_client::{prelude::*, ClientBlockImportExt};

	#[tokio::test]
	async fn check_proof_has_code() {
		let builder = TestClientBuilder::new();
		let mut client = Arc::new(builder.build());

		// Import a new block.
		let block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().genesis_hash)
			.with_parent_block_number(0)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let best_hash = block.header.hash();

		client.import(BlockOrigin::Own, block.clone()).await.unwrap();
		client.finalize_block(best_hash, None).unwrap();

		let storage_proof = generate_checkpoint_proof(&client, best_hash).unwrap();

		// Inspect the contents of the proof.
		let memdb = storage_proof.to_memory_db::<sp_runtime::traits::BlakeTwo256>().drain();
		let storage: HashMap<_, _> =
			memdb.iter().map(|(key, (value, _n))| (key.as_bytes(), value)).collect();

		// The code entry must be present in the proof.
		let code = client
			.storage(best_hash, &StorageKey(well_known_keys::CODE.into()))
			.unwrap()
			.unwrap();

		let found_code = storage.iter().any(|(_, value)| value == &&code.0);
		assert!(found_code);
	}
}
