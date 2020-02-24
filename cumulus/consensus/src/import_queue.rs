// Copyright 2019 Parity Technologies (UK) Ltd.
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

use std::sync::Arc;

use sc_client::Client;
use sc_client_api::{Backend, CallExecutor, TransactionFor};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::Result as ClientResult;
use sp_consensus::{
	error::Error as ConsensusError,
	import_queue::{BasicQueue, CacheKeyId, Verifier as VerifierT},
	BlockImport, BlockImportParams, BlockOrigin, ForkChoiceStrategy,
};
use sp_inherents::InherentDataProviders;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
	Justification,
};

/// A verifier that just checks the inherents.
struct Verifier<B, E, Block: BlockT, RA> {
	client: Arc<Client<B, E, Block, RA>>,
	inherent_data_providers: InherentDataProviders,
}

impl<B, E, Block, RA> VerifierT<Block> for Verifier<B, E, Block, RA>
where
	Block: BlockT,
	B: Backend<Block> + 'static,
	E: CallExecutor<Block> + 'static + Clone + Send + Sync,
	RA: Send + Sync,
	Client<B, E, Block, RA>: ProvideRuntimeApi<Block> + Send + Sync,
	<Client<B, E, Block, RA> as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block>,
{
	fn verify(
		&mut self,
		origin: BlockOrigin,
		header: Block::Header,
		justification: Option<Justification>,
		mut body: Option<Vec<Block::Extrinsic>>,
	) -> Result<
		(
			BlockImportParams<Block, ()>,
			Option<Vec<(CacheKeyId, Vec<u8>)>>,
		),
		String,
	> {
		if let Some(inner_body) = body.take() {
			let inherent_data = self
				.inherent_data_providers
				.create_inherent_data()
				.map_err(|e| e.into_string())?;

			let block = Block::new(header.clone(), inner_body);

			let inherent_res = self
				.client
				.runtime_api()
				.check_inherents(
					&BlockId::Hash(*header.parent_hash()),
					block.clone(),
					inherent_data,
				)
				.map_err(|e| format!("{:?}", e))?;

			if !inherent_res.ok() {
				inherent_res.into_errors().try_for_each(|(i, e)| {
					Err(self.inherent_data_providers.error_to_string(&i, &e))
				})?;
			}

			let (_, inner_body) = block.deconstruct();
			body = Some(inner_body);
		}

		let post_hash = Some(header.hash());
		let mut block_import_params = BlockImportParams::new(origin, header);
		block_import_params.body = body;
		block_import_params.justification = justification;
		block_import_params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		block_import_params.post_hash = post_hash;

		Ok((block_import_params, None))
	}
}

/// Start an import queue for a Cumulus collator that does not uses any special authoring logic.
pub fn import_queue<B, E, Block: BlockT, I, RA>(
	client: Arc<Client<B, E, Block, RA>>,
	block_import: I,
	inherent_data_providers: InherentDataProviders,
) -> ClientResult<BasicQueue<Block, TransactionFor<B, Block>>>
where
	B: Backend<Block> + 'static,
	I: BlockImport<Block, Error = ConsensusError, Transaction = TransactionFor<B, Block>>
		+ Send
		+ Sync
		+ 'static,
	E: CallExecutor<Block> + Clone + Send + Sync + 'static,
	RA: Send + Sync + 'static,
	Client<B, E, Block, RA>: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	<Client<B, E, Block, RA> as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block>,
{
	let verifier = Verifier {
		client,
		inherent_data_providers,
	};

	Ok(BasicQueue::new(
		verifier,
		Box::new(block_import),
		None,
		None,
	))
}
