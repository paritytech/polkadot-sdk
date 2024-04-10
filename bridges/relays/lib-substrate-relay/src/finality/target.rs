// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Substrate client as Substrate finality proof target.

use crate::{
	finality::{
		engine::Engine, source::SubstrateFinalityProof, FinalitySyncPipelineAdapter,
		SubmitFinalityProofCallBuilder, SubstrateFinalitySyncPipeline,
	},
	TransactionParams,
};

use async_trait::async_trait;
use finality_relay::TargetClient;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Client, Error, HeaderIdOf, HeaderOf, SyncHeader, TransactionEra,
	TransactionTracker, UnsignedTransaction,
};
use relay_utils::relay_loop::Client as RelayClient;
use sp_core::Pair;

/// Substrate client as Substrate finality target.
pub struct SubstrateFinalityTarget<P: SubstrateFinalitySyncPipeline> {
	client: Client<P::TargetChain>,
	transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
}

impl<P: SubstrateFinalitySyncPipeline> SubstrateFinalityTarget<P> {
	/// Create new Substrate headers target.
	pub fn new(
		client: Client<P::TargetChain>,
		transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	) -> Self {
		SubstrateFinalityTarget { client, transaction_params }
	}

	/// Ensure that the bridge pallet at target chain is active.
	pub async fn ensure_pallet_active(&self) -> Result<(), Error> {
		let is_halted = P::FinalityEngine::is_halted(&self.client).await?;
		if is_halted {
			return Err(Error::BridgePalletIsHalted)
		}

		let is_initialized = P::FinalityEngine::is_initialized(&self.client).await?;
		if !is_initialized {
			return Err(Error::BridgePalletIsNotInitialized)
		}

		Ok(())
	}
}

impl<P: SubstrateFinalitySyncPipeline> Clone for SubstrateFinalityTarget<P> {
	fn clone(&self) -> Self {
		SubstrateFinalityTarget {
			client: self.client.clone(),
			transaction_params: self.transaction_params.clone(),
		}
	}
}

#[async_trait]
impl<P: SubstrateFinalitySyncPipeline> RelayClient for SubstrateFinalityTarget<P> {
	type Error = Error;

	async fn reconnect(&mut self) -> Result<(), Error> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P: SubstrateFinalitySyncPipeline> TargetClient<FinalitySyncPipelineAdapter<P>>
	for SubstrateFinalityTarget<P>
where
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
{
	type TransactionTracker = TransactionTracker<P::TargetChain, Client<P::TargetChain>>;

	async fn best_finalized_source_block_id(&self) -> Result<HeaderIdOf<P::SourceChain>, Error> {
		// we can't continue to relay finality if target node is out of sync, because
		// it may have already received (some of) headers that we're going to relay
		self.client.ensure_synced().await?;
		// we can't relay finality if bridge pallet at target chain is halted
		self.ensure_pallet_active().await?;

		Ok(crate::messages_source::read_client_state::<P::TargetChain, P::SourceChain>(
			&self.client,
			None,
		)
		.await?
		.best_finalized_peer_at_best_self
		.ok_or(Error::BridgePalletIsNotInitialized)?)
	}

	async fn submit_finality_proof(
		&self,
		header: SyncHeader<HeaderOf<P::SourceChain>>,
		proof: SubstrateFinalityProof<P>,
	) -> Result<Self::TransactionTracker, Error> {
		let transaction_params = self.transaction_params.clone();
		let call =
			P::SubmitFinalityProofCallBuilder::build_submit_finality_proof_call(header, proof);
		self.client
			.submit_and_watch_signed_extrinsic(
				&self.transaction_params.signer,
				move |best_block_id, transaction_nonce| {
					Ok(UnsignedTransaction::new(call.into(), transaction_nonce)
						.era(TransactionEra::new(best_block_id, transaction_params.mortality)))
				},
			)
			.await
	}
}
