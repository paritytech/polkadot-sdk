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

use crate::{
	parachains::{ParachainsPipelineAdapter, SubstrateParachainsPipeline},
	proofs::to_raw_storage_proof,
};
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use bp_parachains::parachain_head_storage_key_at_source;
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
use bp_runtime::HeaderIdProvider;
use codec::Decode;
use parachains_relay::parachains_loop::{AvailableHeader, SourceClient};
use relay_substrate_client::{
	is_ancient_block, Chain, Client, Error as SubstrateError, HeaderIdOf, HeaderOf, ParachainBase,
	RelayChain,
};
use relay_utils::relay_loop::Client as RelayClient;

/// Shared updatable reference to the maximal parachain header id that we want to sync from the
/// source.
pub type RequiredHeaderIdRef<C> = Arc<Mutex<AvailableHeader<HeaderIdOf<C>>>>;

/// Substrate client as parachain heads source.
#[derive(Clone)]
pub struct ParachainsSource<P: SubstrateParachainsPipeline, SourceRelayClnt> {
	client: SourceRelayClnt,
	max_head_id: RequiredHeaderIdRef<P::SourceParachain>,
}

impl<P: SubstrateParachainsPipeline, SourceRelayClnt: Client<P::SourceRelayChain>>
	ParachainsSource<P, SourceRelayClnt>
{
	/// Creates new parachains source client.
	pub fn new(
		client: SourceRelayClnt,
		max_head_id: RequiredHeaderIdRef<P::SourceParachain>,
	) -> Self {
		ParachainsSource { client, max_head_id }
	}

	/// Returns reference to the underlying RPC client.
	pub fn client(&self) -> &SourceRelayClnt {
		&self.client
	}

	/// Return decoded head of given parachain.
	pub async fn on_chain_para_head_id(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
	) -> Result<Option<HeaderIdOf<P::SourceParachain>>, SubstrateError> {
		let para_id = ParaId(P::SourceParachain::PARACHAIN_ID);
		let storage_key =
			parachain_head_storage_key_at_source(P::SourceRelayChain::PARAS_PALLET_NAME, para_id);
		let para_head: Option<ParaHead> =
			self.client.storage_value(at_block.hash(), storage_key).await?;
		let para_head = match para_head {
			Some(para_head) => para_head,
			None => return Ok(None),
		};
		let para_head: HeaderOf<P::SourceParachain> = Decode::decode(&mut &para_head.0[..])?;
		Ok(Some(para_head.id()))
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline, SourceRelayClnt: Client<P::SourceRelayChain>> RelayClient
	for ParachainsSource<P, SourceRelayClnt>
{
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P: SubstrateParachainsPipeline, SourceRelayClnt: Client<P::SourceRelayChain>>
	SourceClient<ParachainsPipelineAdapter<P>> for ParachainsSource<P, SourceRelayClnt>
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
	) -> Result<AvailableHeader<HeaderIdOf<P::SourceParachain>>, Self::Error> {
		// if requested relay header is ancient, then we don't even want to try to read the
		// parachain head - we simply return `Unavailable`
		let best_block_number = self.client.best_finalized_header_number().await?;
		if is_ancient_block(at_block.number(), best_block_number) {
			log::trace!(
				target: "bridge",
				"{} block {:?} is ancient. Cannot prove the {} header there",
				P::SourceRelayChain::NAME,
				at_block,
				P::SourceParachain::NAME,
			);
			return Ok(AvailableHeader::Unavailable)
		}

		// else - try to read head from the source client
		let mut para_head_id = AvailableHeader::Missing;
		if let Some(on_chain_para_head_id) = self.on_chain_para_head_id(at_block).await? {
			// Never return head that is larger than requested. This way we'll never sync
			// headers past `max_header_id`.
			para_head_id = match *self.max_head_id.lock().await {
				AvailableHeader::Unavailable => AvailableHeader::Unavailable,
				AvailableHeader::Missing => {
					// `max_header_id` is not set. There is no limit.
					AvailableHeader::Available(on_chain_para_head_id)
				},
				AvailableHeader::Available(max_head_id) if on_chain_para_head_id >= max_head_id => {
					// We report at most `max_header_id`.
					AvailableHeader::Available(std::cmp::min(on_chain_para_head_id, max_head_id))
				},
				AvailableHeader::Available(_) => {
					// the `max_head_id` is not yet available at the source chain => wait and avoid
					// syncing extra headers
					AvailableHeader::Unavailable
				},
			}
		}

		Ok(para_head_id)
	}

	async fn prove_parachain_head(
		&self,
		at_block: HeaderIdOf<P::SourceRelayChain>,
	) -> Result<(ParaHeadsProof, ParaHash), Self::Error> {
		let parachain = ParaId(P::SourceParachain::PARACHAIN_ID);
		let storage_key =
			parachain_head_storage_key_at_source(P::SourceRelayChain::PARAS_PALLET_NAME, parachain);

		let storage_proof =
			self.client.prove_storage(at_block.hash(), vec![storage_key.clone()]).await?;

		// why we're reading parachain head here once again (it has already been read at the
		// `parachain_head`)? that's because `parachain_head` sometimes returns obsolete parachain
		// head and loop sometimes asks to prove this obsolete head and gets other (actual) head
		// instead
		//
		// => since we want to provide proper hashes in our `submit_parachain_heads` call, we're
		// rereading actual value here
		let parachain_head = self
			.client
			.storage_value::<ParaHead>(at_block.hash(), storage_key)
			.await?
			.ok_or_else(|| {
				SubstrateError::Custom(format!(
					"Failed to read expected parachain {parachain:?} head at {at_block:?}"
				))
			})?;
		let parachain_head_hash = parachain_head.hash();

		Ok((
			ParaHeadsProof {
				storage_proof: to_raw_storage_proof::<P::SourceRelayChain>(storage_proof),
			},
			parachain_head_hash,
		))
	}
}
