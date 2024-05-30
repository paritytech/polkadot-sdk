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
use bp_parachains::{
	ImportedParaHeadsKeyProvider, ParaInfo, ParaStoredHeaderData, ParasInfoKeyProvider,
};
use bp_polkadot_core::{
	parachains::{ParaHash, ParaHeadsProof, ParaId},
	BlockNumber as RelayBlockNumber,
};
use bp_runtime::{
	Chain as ChainBase, HeaderId, HeaderIdProvider, StorageDoubleMapKeyProvider,
	StorageMapKeyProvider,
};
use parachains_relay::parachains_loop::TargetClient;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, Client, Error as SubstrateError,
	HeaderIdOf, ParachainBase, RelayChain, TransactionEra, TransactionTracker, UnsignedTransaction,
};
use relay_utils::relay_loop::Client as RelayClient;
use sp_core::Pair;

/// Substrate client as parachain heads source.
pub struct ParachainsTarget<P: SubstrateParachainsPipeline> {
	source_client: Client<P::SourceRelayChain>,
	target_client: Client<P::TargetChain>,
	transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
}

impl<P: SubstrateParachainsPipeline> ParachainsTarget<P> {
	/// Creates new parachains target client.
	pub fn new(
		source_client: Client<P::SourceRelayChain>,
		target_client: Client<P::TargetChain>,
		transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	) -> Self {
		ParachainsTarget { source_client, target_client, transaction_params }
	}

	/// Returns reference to the underlying RPC client.
	pub fn target_client(&self) -> &Client<P::TargetChain> {
		&self.target_client
	}
}

impl<P: SubstrateParachainsPipeline> Clone for ParachainsTarget<P> {
	fn clone(&self) -> Self {
		ParachainsTarget {
			source_client: self.source_client.clone(),
			target_client: self.target_client.clone(),
			transaction_params: self.transaction_params.clone(),
		}
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline> RelayClient for ParachainsTarget<P> {
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.target_client.reconnect().await?;
		self.source_client.reconnect().await?;
		Ok(())
	}
}

#[async_trait]
impl<P> TargetClient<ParachainsPipelineAdapter<P>> for ParachainsTarget<P>
where
	P: SubstrateParachainsPipeline,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
	P::SourceParachain: ChainBase<Hash = ParaHash>,
	P::SourceRelayChain: ChainBase<BlockNumber = RelayBlockNumber>,
{
	type TransactionTracker = TransactionTracker<P::TargetChain, Client<P::TargetChain>>;

	async fn best_block(&self) -> Result<HeaderIdOf<P::TargetChain>, Self::Error> {
		let best_header = self.target_client.best_header().await?;
		let best_id = best_header.id();

		Ok(best_id)
	}

	async fn best_finalized_source_relay_chain_block(
		&self,
		at_block: &HeaderIdOf<P::TargetChain>,
	) -> Result<HeaderIdOf<P::SourceRelayChain>, Self::Error> {
		self.target_client
			.typed_state_call::<_, Option<HeaderIdOf<P::SourceRelayChain>>>(
				P::SourceRelayChain::BEST_FINALIZED_HEADER_ID_METHOD.into(),
				(),
				Some(at_block.1),
			)
			.await?
			.map(Ok)
			.unwrap_or(Err(SubstrateError::BridgePalletIsNotInitialized))
	}

	async fn free_source_relay_headers_interval(
		&self,
	) -> Result<Option<BlockNumberOf<P::SourceRelayChain>>, Self::Error> {
		Ok(self
			.target_client
			.typed_state_call(P::SourceRelayChain::FREE_HEADERS_INTERVAL_METHOD.into(), (), None)
			.await
			.unwrap_or_else(|e| {
				log::info!(
					target: "bridge",
					"Call of {} at {} has failed with an error: {:?}. Treating as `None`",
					P::SourceRelayChain::FREE_HEADERS_INTERVAL_METHOD,
					P::TargetChain::NAME,
					e,
				);
				None
			}))
	}

	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::TargetChain>,
	) -> Result<
		Option<(HeaderIdOf<P::SourceRelayChain>, HeaderIdOf<P::SourceParachain>)>,
		Self::Error,
	> {
		// read best parachain head from the target bridge-parachains pallet
		let storage_key = ParasInfoKeyProvider::final_key(
			P::SourceRelayChain::WITH_CHAIN_BRIDGE_PARACHAINS_PALLET_NAME,
			&P::SourceParachain::PARACHAIN_ID.into(),
		);
		let storage_value: Option<ParaInfo> =
			self.target_client.storage_value(storage_key, Some(at_block.hash())).await?;
		let para_info = match storage_value {
			Some(para_info) => para_info,
			None => return Ok(None),
		};

		// now we need to get full header ids. For source relay chain it is simple, because we
		// are connected
		let relay_header_id = self
			.source_client
			.header_by_number(para_info.best_head_hash.at_relay_block_number)
			.await?
			.id();

		// for parachain, we need to read from the target chain runtime storage
		let storage_key = ImportedParaHeadsKeyProvider::final_key(
			P::SourceRelayChain::WITH_CHAIN_BRIDGE_PARACHAINS_PALLET_NAME,
			&P::SourceParachain::PARACHAIN_ID.into(),
			&para_info.best_head_hash.head_hash,
		);
		let storage_value: Option<ParaStoredHeaderData> =
			self.target_client.storage_value(storage_key, Some(at_block.hash())).await?;
		let para_head_number = match storage_value {
			Some(para_head_data) =>
				para_head_data.decode_parachain_head_data::<P::SourceParachain>()?.number,
			None => return Ok(None),
		};

		let para_head_id = HeaderId(para_head_number, para_info.best_head_hash.head_hash);
		Ok(Some((relay_header_id, para_head_id)))
	}

	async fn submit_parachain_head_proof(
		&self,
		at_relay_block: HeaderIdOf<P::SourceRelayChain>,
		updated_head_hash: ParaHash,
		proof: ParaHeadsProof,
		is_free_execution_expected: bool,
	) -> Result<Self::TransactionTracker, Self::Error> {
		let transaction_params = self.transaction_params.clone();
		let call = P::SubmitParachainHeadsCallBuilder::build_submit_parachain_heads_call(
			at_relay_block,
			vec![(ParaId(P::SourceParachain::PARACHAIN_ID), updated_head_hash)],
			proof,
			is_free_execution_expected,
		);
		self.target_client
			.submit_and_watch_signed_extrinsic(
				&transaction_params.signer,
				move |best_block_id, transaction_nonce| {
					Ok(UnsignedTransaction::new(call.into(), transaction_nonce)
						.era(TransactionEra::new(best_block_id, transaction_params.mortality)))
				},
			)
			.await
	}
}
