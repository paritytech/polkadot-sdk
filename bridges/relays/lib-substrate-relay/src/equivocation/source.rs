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
		EquivocationDetectionPipelineAdapter, EquivocationProofOf, ReportEquivocationCallBuilder,
		SubstrateEquivocationDetectionPipeline,
	},
	finality_base::{engine::Engine, finality_proofs, SubstrateFinalityProofsStream},
	TransactionParams,
};

use async_trait::async_trait;
use bp_runtime::{HashOf, TransactionEra};
use equivocation_detector::SourceClient;
use finality_relay::SourceClientBase;
use relay_substrate_client::{
	AccountKeyPairOf, Client, Error, TransactionTracker, UnsignedTransaction,
};
use relay_utils::relay_loop::Client as RelayClient;

/// Substrate node as equivocation source.
pub struct SubstrateEquivocationSource<P: SubstrateEquivocationDetectionPipeline> {
	client: Client<P::SourceChain>,
	transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
}

impl<P: SubstrateEquivocationDetectionPipeline> SubstrateEquivocationSource<P> {
	/// Create new instance of `SubstrateEquivocationSource`.
	pub fn new(
		client: Client<P::SourceChain>,
		transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	) -> Self {
		Self { client, transaction_params }
	}
}

impl<P: SubstrateEquivocationDetectionPipeline> Clone for SubstrateEquivocationSource<P> {
	fn clone(&self) -> Self {
		Self { client: self.client.clone(), transaction_params: self.transaction_params.clone() }
	}
}

#[async_trait]
impl<P: SubstrateEquivocationDetectionPipeline> RelayClient for SubstrateEquivocationSource<P> {
	type Error = Error;

	async fn reconnect(&mut self) -> Result<(), Error> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P: SubstrateEquivocationDetectionPipeline>
	SourceClientBase<EquivocationDetectionPipelineAdapter<P>> for SubstrateEquivocationSource<P>
{
	type FinalityProofsStream = SubstrateFinalityProofsStream<P>;

	async fn finality_proofs(&self) -> Result<Self::FinalityProofsStream, Error> {
		finality_proofs::<P>(&self.client).await
	}
}

#[async_trait]
impl<P: SubstrateEquivocationDetectionPipeline>
	SourceClient<EquivocationDetectionPipelineAdapter<P>> for SubstrateEquivocationSource<P>
{
	type TransactionTracker = TransactionTracker<P::SourceChain, Client<P::SourceChain>>;

	async fn report_equivocation(
		&self,
		at: HashOf<P::SourceChain>,
		equivocation: EquivocationProofOf<P>,
	) -> Result<Self::TransactionTracker, Self::Error> {
		let key_owner_proof =
			P::FinalityEngine::generate_source_key_ownership_proof(&self.client, at, &equivocation)
				.await?;

		let mortality = self.transaction_params.mortality;
		let call = P::ReportEquivocationCallBuilder::build_report_equivocation_call(
			equivocation,
			key_owner_proof,
		);
		self.client
			.submit_and_watch_signed_extrinsic(
				&self.transaction_params.signer,
				move |best_block_id, transaction_nonce| {
					Ok(UnsignedTransaction::new(call.into(), transaction_nonce)
						.era(TransactionEra::new(best_block_id, mortality)))
				},
			)
			.await
	}
}
