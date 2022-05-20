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

use async_trait::async_trait;
use bp_parachains::parachain_head_storage_key_at_source;
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
use codec::Decode;
use parachains_relay::{parachains_loop::SourceClient, ParachainsPipeline};
use relay_substrate_client::{Client, Error as SubstrateError, HeaderIdOf};
use relay_utils::relay_loop::Client as RelayClient;

/// Substrate client as parachain heads source.
#[derive(Clone)]
pub struct ParachainsSource<P: ParachainsPipeline> {
	client: Client<P::SourceChain>,
	paras_pallet_name: String,
}

impl<P: ParachainsPipeline> ParachainsSource<P> {
	/// Creates new parachains source client.
	pub fn new(client: Client<P::SourceChain>, paras_pallet_name: String) -> Self {
		ParachainsSource { client, paras_pallet_name }
	}
}

#[async_trait]
impl<P: ParachainsPipeline> RelayClient for ParachainsSource<P> {
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P: ParachainsPipeline> SourceClient<P> for ParachainsSource<P> {
	async fn ensure_synced(&self) -> Result<bool, Self::Error> {
		match self.client.ensure_synced().await {
			Ok(_) => Ok(true),
			Err(SubstrateError::ClientNotSynced(_)) => Ok(false),
			Err(e) => Err(e),
		}
	}

	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::SourceChain>,
		para_id: ParaId,
	) -> Result<Option<ParaHash>, Self::Error> {
		let storage_key = parachain_head_storage_key_at_source(&self.paras_pallet_name, para_id);
		let para_head = self.client.raw_storage_value(storage_key, Some(at_block.1)).await?;
		let para_head = para_head.map(|h| ParaHead::decode(&mut &h.0[..])).transpose()?;
		let para_hash = para_head.map(|h| h.hash());

		Ok(para_hash)
	}

	async fn prove_parachain_heads(
		&self,
		at_block: HeaderIdOf<P::SourceChain>,
		parachains: &[ParaId],
	) -> Result<ParaHeadsProof, Self::Error> {
		let storage_keys = parachains
			.iter()
			.map(|para_id| parachain_head_storage_key_at_source(&self.paras_pallet_name, *para_id))
			.collect();
		let parachain_heads_proof = self
			.client
			.prove_storage(storage_keys, at_block.1)
			.await?
			.iter_nodes()
			.collect();

		Ok(parachain_heads_proof)
	}
}
