// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use std::{marker::PhantomData, sync::Arc};

use sc_consensus::{import_queue::Verifier as VerifierT, BlockImportParams};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::BlockOrigin;
use sp_inherents::CreateInherentDataProviders;
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
		&self,
		mut block_params: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		let is_sync_selected = matches!(
			block_params.origin,
			BlockOrigin::NetworkInitialSync |
				BlockOrigin::WarpSync |
				BlockOrigin::StateSync { .. } |
				BlockOrigin::GapSync
		);
		block_params.fork_choice = Some(sc_consensus::ForkChoiceStrategy::Custom(is_sync_selected));

		// Skip checks that include execution, if being told so, or when importing only state.
		//
		// This is done for example when gap syncing and it is expected that the block after the gap
		// was checked/chosen properly, e.g. by warp syncing to this block using a finality proof.
		if block_params.state_action.skip_execution_checks() || block_params.with_state() {
			return Ok(block_params);
		}

		if let Some(inner_body) = block_params.body.take() {
			let inherent_data_providers = self
				.create_inherent_data_providers
				.create_inherent_data_providers(*block_params.header.parent_hash(), ())
				.await
				.map_err(|e| e.to_string())?;

			let block = Block::new(block_params.header.clone(), inner_body);
			sp_block_builder::check_inherents(
				self.client.clone(),
				*block.header().parent_hash(),
				block.clone(),
				&inherent_data_providers,
			)
			.await
			.map_err(|e| format!("Error checking block inherents {:?}", e))?;

			let (_, inner_body) = block.deconstruct();
			block_params.body = Some(inner_body);
		}

		block_params.post_hash = Some(block_params.header.hash());
		Ok(block_params)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use cumulus_test_client::{
		runtime::Block as TestBlock, Client, TestClientBuilder, TestClientBuilderExt,
	};
	use futures::FutureExt;
	use sc_client_api::HeaderBackend;
	use sc_consensus::{ForkChoiceStrategy, StateAction};
	use std::sync::Arc;

	fn new_verifier_and_header() -> (
		Verifier<Client, TestBlock, impl CreateInherentDataProviders<TestBlock, ()>>,
		<TestBlock as BlockT>::Header,
	) {
		let client = Arc::new(TestClientBuilder::default().build());
		let header = client.header(client.info().best_hash).unwrap().unwrap();

		let verifier = Verifier::new(client, |_, _| async move {
			Ok(sp_timestamp::InherentDataProvider::from_system_time())
		});

		(verifier, header)
	}

	#[test]
	fn sync_origins_use_custom_fork_choice() {
		let (verifier, header) = new_verifier_and_header();

		// These origins indicate that the imported block was chosen by a sync strategy
		// (warp/state/gap sync, or initial sync) and should become the new best block
		// without going through the normal longest-chain fork choice.
		for origin in [
			BlockOrigin::NetworkInitialSync,
			BlockOrigin::WarpSync,
			BlockOrigin::StateSync { is_verified: false },
			BlockOrigin::StateSync { is_verified: true },
			BlockOrigin::GapSync,
		] {
			let mut params = BlockImportParams::new(origin, header.clone());
			// Force the early-return path so this test only exercises the fork choice
			// selection, without requiring a full block body and valid inherents.
			params.state_action = StateAction::Skip;

			let params = verifier.verify(params).now_or_never().unwrap().unwrap();
			assert_eq!(
				params.fork_choice,
				Some(ForkChoiceStrategy::Custom(true)),
				"expected custom (sync) fork choice for {origin:?}",
			);
		}
	}

	#[test]
	fn non_sync_origins_do_not_use_custom_fork_choice() {
		let (verifier, header) = new_verifier_and_header();

		for origin in [
			BlockOrigin::Genesis,
			BlockOrigin::Own,
			BlockOrigin::NetworkBroadcast,
			BlockOrigin::ConsensusBroadcast,
			BlockOrigin::File,
		] {
			let mut params = BlockImportParams::new(origin, header.clone());
			params.state_action = StateAction::Skip;

			let params = verifier.verify(params).now_or_never().unwrap().unwrap();
			assert_eq!(
				params.fork_choice,
				Some(ForkChoiceStrategy::Custom(false)),
				"expected non-sync fork choice for {origin:?}",
			);
		}
	}
}
