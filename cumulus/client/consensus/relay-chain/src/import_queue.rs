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

use std::{marker::PhantomData, sync::Arc};

use cumulus_client_consensus_common::ParachainBlockImportMarker;

use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImport, BlockImportParams,
};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::Result as ClientResult;
use sp_consensus::error::Error as ConsensusError;
use sp_inherents::{CreateInherentDataProviders, InherentDataProvider};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

/// A verifier that just checks the inherents.
pub struct Verifier<Client, Block, CIDP> {
	client: Arc<Client>,
	create_inherent_data_providers: CIDP,
	_marker: PhantomData<Block>,
}

impl<Client, Block, CIDP> Verifier<Client, Block, CIDP> {
	/// Create a new instance.
	pub fn new(client: Arc<Client>, create_inherent_data_providers: CIDP) -> Self {
		Self { client, create_inherent_data_providers, _marker: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Client, Block, CIDP> VerifierT<Block> for Verifier<Client, Block, CIDP>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	<Client as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block>,
	CIDP: CreateInherentDataProviders<Block, ()>,
{
	async fn verify(
		&mut self,
		mut block_params: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		block_params.fork_choice = Some(sc_consensus::ForkChoiceStrategy::Custom(
			block_params.origin == sp_consensus::BlockOrigin::NetworkInitialSync,
		));

		// Skip checks that include execution, if being told so, or when importing only state.
		//
		// This is done for example when gap syncing and it is expected that the block after the gap
		// was checked/chosen properly, e.g. by warp syncing to this block using a finality proof.
		if block_params.state_action.skip_execution_checks() || block_params.with_state() {
			return Ok(block_params)
		}

		if let Some(inner_body) = block_params.body.take() {
			let inherent_data_providers = self
				.create_inherent_data_providers
				.create_inherent_data_providers(*block_params.header.parent_hash(), ())
				.await
				.map_err(|e| e.to_string())?;

			let inherent_data = inherent_data_providers
				.create_inherent_data()
				.await
				.map_err(|e| format!("{:?}", e))?;

			let block = Block::new(block_params.header.clone(), inner_body);

			let inherent_res = self
				.client
				.runtime_api()
				.check_inherents(*block.header().parent_hash(), block.clone(), inherent_data)
				.map_err(|e| format!("{:?}", e))?;

			if !inherent_res.ok() {
				for (i, e) in inherent_res.into_errors() {
					match inherent_data_providers.try_handle_error(&i, &e).await {
						Some(r) => r.map_err(|e| format!("{:?}", e))?,
						None => Err(format!(
							"Unhandled inherent error from `{}`.",
							String::from_utf8_lossy(&i)
						))?,
					}
				}
			}

			let (_, inner_body) = block.deconstruct();
			block_params.body = Some(inner_body);
		}

		block_params.post_hash = Some(block_params.header.hash());
		Ok(block_params)
	}
}

/// Start an import queue for a Cumulus collator that does not uses any special authoring logic.
pub fn import_queue<Client, Block: BlockT, I, CIDP>(
	client: Arc<Client>,
	block_import: I,
	create_inherent_data_providers: CIDP,
	spawner: &impl sp_core::traits::SpawnEssentialNamed,
	registry: Option<&substrate_prometheus_endpoint::Registry>,
) -> ClientResult<BasicQueue<Block>>
where
	I: BlockImport<Block, Error = ConsensusError>
		+ ParachainBlockImportMarker
		+ Send
		+ Sync
		+ 'static,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	<Client as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block>,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
{
	let verifier = Verifier::new(client, create_inherent_data_providers);

	Ok(BasicQueue::new(verifier, Box::new(block_import), None, spawner, registry))
}
