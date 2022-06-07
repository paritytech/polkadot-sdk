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

//! Parachain heads source.

use crate::parachains::{ParachainsPipelineAdapter, SubstrateParachainsPipeline};

use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use bp_parachains::parachain_head_storage_key_at_source;
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
use codec::Decode;
use parachains_relay::parachains_loop::{ParaHashAtSource, SourceClient};
use relay_substrate_client::{
	Chain, Client, Error as SubstrateError, HeaderIdOf, HeaderOf, RelayChain,
};
use relay_utils::relay_loop::Client as RelayClient;
use sp_runtime::traits::Header as HeaderT;

/// Shared updatable reference to the maximal parachain header id that we want to sync from the
/// source.
pub type RequiredHeaderIdRef<C> = Arc<Mutex<Option<HeaderIdOf<C>>>>;

/// Substrate client as parachain heads source.
#[derive(Clone)]
pub struct ParachainsSource<P: SubstrateParachainsPipeline> {
	client: Client<P::SourceRelayChain>,
	maximal_header_id: Option<RequiredHeaderIdRef<P::SourceParachain>>,
}

impl<P: SubstrateParachainsPipeline> ParachainsSource<P> {
	/// Creates new parachains source client.
	pub fn new(
		client: Client<P::SourceRelayChain>,
		maximal_header_id: Option<RequiredHeaderIdRef<P::SourceParachain>>,
	) -> Self {
		ParachainsSource { client, maximal_header_id }
	}

	/// Returns reference to the underlying RPC client.
	pub fn client(&self) -> &Client<P::SourceRelayChain> {
		&self.client
	}

	/// Return decoded head of given parachain.
	pub async fn on_chain_parachain_header(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
		para_id: ParaId,
	) -> Result<Option<HeaderOf<P::SourceParachain>>, SubstrateError> {
		let storage_key =
			parachain_head_storage_key_at_source(P::SourceRelayChain::PARAS_PALLET_NAME, para_id);
		let para_head = self.client.raw_storage_value(storage_key, Some(at_block.1)).await?;
		let para_head = para_head.map(|h| ParaHead::decode(&mut &h.0[..])).transpose()?;
		let para_head = match para_head {
			Some(para_head) => para_head,
			None => return Ok(None),
		};

		Ok(Some(Decode::decode(&mut &para_head.0[..])?))
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline> RelayClient for ParachainsSource<P> {
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline> SourceClient<ParachainsPipelineAdapter<P>>
	for ParachainsSource<P>
where
	P::SourceParachain: Chain<Hash = ParaHash>,
{
	async fn ensure_synced(&self) -> Result<bool, Self::Error> {
		match self.client.ensure_synced().await {
			Ok(_) => Ok(true),
			Err(SubstrateError::ClientNotSynced(_)) => Ok(false),
			Err(e) => Err(e),
		}
	}

	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
		para_id: ParaId,
	) -> Result<ParaHashAtSource, Self::Error> {
		// we don't need to support many parachains now
		if para_id.0 != P::SOURCE_PARACHAIN_PARA_ID {
			return Err(SubstrateError::Custom(format!(
				"Parachain id {} is not matching expected {}",
				para_id.0,
				P::SOURCE_PARACHAIN_PARA_ID,
			)))
		}

		Ok(match self.on_chain_parachain_header(at_block, para_id).await? {
			Some(parachain_header) => {
				let mut parachain_head = ParaHashAtSource::Some(parachain_header.hash());
				// never return head that is larger than requested. This way we'll never sync
				// headers past `maximal_header_id`
				if let Some(ref maximal_header_id) = self.maximal_header_id {
					let maximal_header_id = *maximal_header_id.lock().await;
					match maximal_header_id {
						Some(maximal_header_id)
							if *parachain_header.number() > maximal_header_id.0 =>
						{
							// we don't want this header yet => let's report previously requested
							// header
							parachain_head = ParaHashAtSource::Some(maximal_header_id.1);
						},
						Some(_) => (),
						None => {
							// on-demand relay has not yet asked us to sync anything let's do that
							parachain_head = ParaHashAtSource::Unavailable;
						},
					}
				}

				parachain_head
			},
			None => ParaHashAtSource::None,
		})
	}

	async fn prove_parachain_heads(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
		parachains: &[ParaId],
	) -> Result<ParaHeadsProof, Self::Error> {
		let storage_keys = parachains
			.iter()
			.map(|para_id| {
				parachain_head_storage_key_at_source(
					P::SourceRelayChain::PARAS_PALLET_NAME,
					*para_id,
				)
			})
			.collect();
		let parachain_heads_proof = self
			.client
			.prove_storage(storage_keys, at_block.1)
			.await?
			.iter_nodes()
			.collect();

		Ok(ParaHeadsProof(parachain_heads_proof))
	}
}
