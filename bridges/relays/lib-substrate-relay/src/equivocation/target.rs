// Copyright 2019-2023 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Default generic implementation of equivocation source for basic Substrate client.

use crate::{
	equivocation::{
		EquivocationDetectionPipelineAdapter, FinalityProoffOf, FinalityVerificationContextfOf,
		SubstrateEquivocationDetectionPipeline,
	},
	finality_base::{best_synced_header_id, engine::Engine},
};

use async_trait::async_trait;
use bp_header_chain::HeaderFinalityInfo;
use bp_runtime::{BlockNumberOf, HashOf};
use equivocation_detector::TargetClient;
use relay_substrate_client::{Client, Error};
use relay_utils::relay_loop::Client as RelayClient;
use sp_runtime::traits::Header;
use std::marker::PhantomData;

/// Substrate node as equivocation source.
pub struct SubstrateEquivocationTarget<P: SubstrateEquivocationDetectionPipeline> {
	client: Client<P::TargetChain>,

	_phantom: PhantomData<P>,
}

impl<P: SubstrateEquivocationDetectionPipeline> SubstrateEquivocationTarget<P> {
	/// Create new instance of `SubstrateEquivocationTarget`.
	pub fn new(client: Client<P::TargetChain>) -> Self {
		Self { client, _phantom: Default::default() }
	}
}

impl<P: SubstrateEquivocationDetectionPipeline> Clone for SubstrateEquivocationTarget<P> {
	fn clone(&self) -> Self {
		Self { client: self.client.clone(), _phantom: Default::default() }
	}
}

#[async_trait]
impl<P: SubstrateEquivocationDetectionPipeline> RelayClient for SubstrateEquivocationTarget<P> {
	type Error = Error;

	async fn reconnect(&mut self) -> Result<(), Error> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P: SubstrateEquivocationDetectionPipeline>
	TargetClient<EquivocationDetectionPipelineAdapter<P>> for SubstrateEquivocationTarget<P>
{
	async fn best_finalized_header_number(
		&self,
	) -> Result<BlockNumberOf<P::TargetChain>, Self::Error> {
		self.client.best_finalized_header_number().await
	}

	async fn best_synced_header_hash(
		&self,
		at: BlockNumberOf<P::TargetChain>,
	) -> Result<Option<HashOf<P::SourceChain>>, Self::Error> {
		Ok(best_synced_header_id::<P::SourceChain, P::TargetChain>(
			&self.client,
			self.client.header_by_number(at).await?.hash(),
		)
		.await?
		.map(|id| id.hash()))
	}

	async fn finality_verification_context(
		&self,
		at: BlockNumberOf<P::TargetChain>,
	) -> Result<FinalityVerificationContextfOf<P>, Self::Error> {
		P::FinalityEngine::finality_verification_context(
			&self.client,
			self.client.header_by_number(at).await?.hash(),
		)
		.await
	}

	async fn synced_headers_finality_info(
		&self,
		at: BlockNumberOf<P::TargetChain>,
	) -> Result<
		Vec<HeaderFinalityInfo<FinalityProoffOf<P>, FinalityVerificationContextfOf<P>>>,
		Self::Error,
	> {
		P::FinalityEngine::synced_headers_finality_info(
			&self.client,
			self.client.header_by_number(at).await?.hash(),
		)
		.await
	}
}
