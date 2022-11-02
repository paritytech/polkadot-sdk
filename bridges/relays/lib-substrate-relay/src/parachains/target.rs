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

//! Parachain heads target.

use crate::{
	parachains::{
		ParachainsPipelineAdapter, SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
	},
	TransactionParams,
};

use async_trait::async_trait;
use bp_parachains::{BestParaHeadHash, ImportedParaHeadsKeyProvider, ParasInfoKeyProvider};
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use bp_runtime::HeaderIdProvider;
use codec::Decode;
use parachains_relay::{
	parachains_loop::TargetClient, parachains_loop_metrics::ParachainsLoopMetrics,
};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, Client, Error as SubstrateError, HashOf,
	HeaderIdOf, HeaderOf, RelayChain, SignParam, TransactionEra, TransactionTracker,
	UnsignedTransaction,
};
use relay_utils::{relay_loop::Client as RelayClient, HeaderId};
use sp_core::{Bytes, Pair};
use sp_runtime::traits::Header as HeaderT;

/// Substrate client as parachain heads source.
pub struct ParachainsTarget<P: SubstrateParachainsPipeline> {
	client: Client<P::TargetChain>,
	transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
}

impl<P: SubstrateParachainsPipeline> ParachainsTarget<P> {
	/// Creates new parachains target client.
	pub fn new(
		client: Client<P::TargetChain>,
		transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	) -> Self {
		ParachainsTarget { client, transaction_params }
	}

	/// Returns reference to the underlying RPC client.
	pub fn client(&self) -> &Client<P::TargetChain> {
		&self.client
	}
}

impl<P: SubstrateParachainsPipeline> Clone for ParachainsTarget<P> {
	fn clone(&self) -> Self {
		ParachainsTarget {
			client: self.client.clone(),
			transaction_params: self.transaction_params.clone(),
		}
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline> RelayClient for ParachainsTarget<P> {
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P> TargetClient<ParachainsPipelineAdapter<P>> for ParachainsTarget<P>
where
	P: SubstrateParachainsPipeline,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
{
	type TransactionTracker = TransactionTracker<P::TargetChain, Client<P::TargetChain>>;

	async fn best_block(&self) -> Result<HeaderIdOf<P::TargetChain>, Self::Error> {
		let best_header = self.client.best_header().await?;
		let best_id = best_header.id();

		Ok(best_id)
	}

	async fn best_finalized_source_block(
		&self,
		at_block: &HeaderIdOf<P::TargetChain>,
	) -> Result<HeaderIdOf<P::SourceRelayChain>, Self::Error> {
		let encoded_best_finalized_source_block = self
			.client
			.state_call(
				P::SourceRelayChain::BEST_FINALIZED_HEADER_ID_METHOD.into(),
				Bytes(Vec::new()),
				Some(at_block.1),
			)
			.await?;

		Option::<HeaderId<HashOf<P::SourceRelayChain>, BlockNumberOf<P::SourceRelayChain>>>::decode(
			&mut &encoded_best_finalized_source_block.0[..],
		)
		.map_err(SubstrateError::ResponseParseFailed)?
		.map(Ok)
		.unwrap_or(Err(SubstrateError::BridgePalletIsNotInitialized))
	}

	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::TargetChain>,
		metrics: Option<&ParachainsLoopMetrics>,
		para_id: ParaId,
	) -> Result<Option<BestParaHeadHash>, Self::Error> {
		let best_para_head_hash: Option<BestParaHeadHash> = self
			.client
			.storage_map_value::<ParasInfoKeyProvider>(
				P::SourceRelayChain::PARACHAINS_FINALITY_PALLET_NAME,
				&para_id,
				Some(at_block.1),
			)
			.await?
			.map(|para_info| para_info.best_head_hash);

		if let (Some(metrics), &Some(ref best_para_head_hash)) = (metrics, &best_para_head_hash) {
			let imported_para_head = self
				.client
				.storage_double_map_value::<ImportedParaHeadsKeyProvider>(
					P::SourceRelayChain::PARACHAINS_FINALITY_PALLET_NAME,
					&para_id,
					&best_para_head_hash.head_hash,
					Some(at_block.1),
				)
				.await
				.and_then(|maybe_encoded_head| match maybe_encoded_head {
					Some(encoded_head) =>
						HeaderOf::<P::SourceParachain>::decode(&mut &encoded_head.0[..])
							.map(Some)
							.map_err(Self::Error::ResponseParseFailed),
					None => Ok(None),
				})
				.map_err(|e| {
					log::error!(
						target: "bridge-metrics",
						"Failed to read or decode {} parachain header at {}: {:?}. Metric will have obsolete value",
						P::SourceParachain::NAME,
						P::TargetChain::NAME,
						e,
					);
					e
				})
				.unwrap_or(None);
			if let Some(imported_para_head) = imported_para_head {
				metrics
					.update_best_parachain_block_at_target(para_id, *imported_para_head.number());
			}
		}

		Ok(best_para_head_hash)
	}

	async fn submit_parachain_heads_proof(
		&self,
		at_relay_block: HeaderIdOf<P::SourceRelayChain>,
		updated_parachains: Vec<(ParaId, ParaHash)>,
		proof: ParaHeadsProof,
	) -> Result<Self::TransactionTracker, Self::Error> {
		let genesis_hash = *self.client.genesis_hash();
		let transaction_params = self.transaction_params.clone();
		let (spec_version, transaction_version) = self.client.simple_runtime_version().await?;
		let call = P::SubmitParachainHeadsCallBuilder::build_submit_parachain_heads_call(
			at_relay_block,
			updated_parachains,
			proof,
		);
		self.client
			.submit_and_watch_signed_extrinsic(
				self.transaction_params.signer.public().into(),
				SignParam::<P::TargetChain> {
					spec_version,
					transaction_version,
					genesis_hash,
					signer: transaction_params.signer,
				},
				move |best_block_id, transaction_nonce| {
					Ok(UnsignedTransaction::new(call.into(), transaction_nonce)
						.era(TransactionEra::new(best_block_id, transaction_params.mortality)))
				},
			)
			.await
	}
}
