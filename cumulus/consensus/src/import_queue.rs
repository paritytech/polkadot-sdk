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

use substrate_primitives::{H256, Blake2Hasher};

use sr_primitives::{
	traits::{Block as BlockT, ProvideRuntimeApi, Header as HeaderT}, Justification,
	generic::BlockId,
};

use substrate_client::{
	block_builder::api::BlockBuilder as BlockBuilderApi, backend::Backend, CallExecutor, Client,
	error::Result as ClientResult,
};

use substrate_consensus_common::{
	import_queue::{Verifier as VerifierT, BasicQueue, CacheKeyId}, BlockImportParams,
	ForkChoiceStrategy, BlockOrigin, error::Error as ConsensusError, BlockImport,
};

use substrate_inherents::InherentDataProviders;

/// A verifier that just checks the inherents.
struct Verifier<B, E, Block: BlockT, RA> {
	client: Arc<Client<B, E, Block, RA>>,
	inherent_data_providers: InherentDataProviders,
}

impl<B, E, Block, RA> VerifierT<Block> for Verifier<B, E, Block, RA> where
	Block: BlockT<Hash=H256>,
	B: Backend<Block, Blake2Hasher> + 'static,
	E: CallExecutor<Block, Blake2Hasher> + 'static + Clone + Send + Sync,
	RA: Send + Sync,
	Client<B, E, Block, RA>: ProvideRuntimeApi + Send + Sync,
	<Client<B, E, Block, RA> as ProvideRuntimeApi>::Api: BlockBuilderApi<Block>
{
	fn verify(
		&mut self,
		origin: BlockOrigin,
		header: Block::Header,
		justification: Option<Justification>,
		mut body: Option<Vec<Block::Extrinsic>>,
	) -> Result<(BlockImportParams<Block>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String> {
		if let Some(inner_body) = body.take() {
			let inherent_data = self.inherent_data_providers
				.create_inherent_data()
				.map_err(String::from)?;

			let block = Block::new(header.clone(), inner_body);

			let inherent_res = self.client.runtime_api().check_inherents(
				&BlockId::Hash(*header.parent_hash()),
				block.clone(),
				inherent_data,
			).map_err(|e| format!("{:?}", e))?;

			if !inherent_res.ok() {
				inherent_res
					.into_errors()
					.try_for_each(|(i, e)| {
						Err(self.inherent_data_providers.error_to_string(&i, &e))
					})?;
			}

			let (_, inner_body) = block.deconstruct();
			body = Some(inner_body);
		}


		let block_import_params = BlockImportParams {
			origin,
			header,
			post_digests: Vec::new(),
			body,
			finalized: false,
			justification,
			auxiliary: Vec::new(),
			fork_choice: ForkChoiceStrategy::LongestChain,
		};

		Ok((block_import_params, None))
	}
}

/// Start an import queue for a Cumulus collator that does not uses any special authoring logic.
pub fn import_queue<B, E, Block: BlockT<Hash=H256>, I, RA>(
	client: Arc<Client<B, E, Block, RA>>,
	block_import: I,
	inherent_data_providers: InherentDataProviders,
) -> ClientResult<BasicQueue<Block>>
	where
		B: Backend<Block, Blake2Hasher> + 'static,
		I: BlockImport<Block,Error=ConsensusError> + Send + Sync + 'static,
		E: CallExecutor<Block, Blake2Hasher> + Clone + Send + Sync + 'static,
		RA: Send + Sync + 'static,
		Client<B, E, Block, RA>: ProvideRuntimeApi + Send + Sync + 'static,
		<Client<B, E, Block, RA> as ProvideRuntimeApi>::Api: BlockBuilderApi<Block>,
{
	let verifier = Verifier {
		client,
		inherent_data_providers
	};

	Ok(BasicQueue::new(
		verifier,
		Box::new(block_import),
		None,
		None,
	))
}
