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

use crate::TransactionParams;

use async_trait::async_trait;
use bp_parachains::{parachain_head_storage_key_at_target, BestParaHeadHash};
use bp_polkadot_core::parachains::{ParaHeadsProof, ParaId};
use codec::{Decode, Encode};
use pallet_bridge_parachains::{
	Call as BridgeParachainsCall, Config as BridgeParachainsConfig, RelayBlockHash,
	RelayBlockHasher, RelayBlockNumber,
};
use parachains_relay::{parachains_loop::TargetClient, ParachainsPipeline};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, CallOf, Chain, Client, Error as SubstrateError,
	HashOf, HeaderIdOf, SignParam, TransactionEra, TransactionSignScheme, UnsignedTransaction,
};
use relay_utils::{relay_loop::Client as RelayClient, HeaderId};
use sp_core::{Bytes, Pair};
use sp_runtime::traits::Header as HeaderT;
use std::marker::PhantomData;

/// Different ways of building `submit_parachain_heads` calls.
pub trait SubmitParachainHeadsCallBuilder<P: ParachainsPipeline>: 'static + Send + Sync {
	/// Given parachains and their heads proof, build call of `submit_parachain_heads`
	/// function of bridge parachains module at the target chain.
	fn build_submit_parachain_heads_call(
		relay_block_hash: HashOf<P::SourceChain>,
		parachains: Vec<ParaId>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<P::TargetChain>;
}

/// Building `submit_parachain_heads` call when you have direct access to the target
/// chain runtime.
pub struct DirectSubmitParachainHeadsCallBuilder<P, R, I> {
	_phantom: PhantomData<(P, R, I)>,
}

impl<P, R, I> SubmitParachainHeadsCallBuilder<P> for DirectSubmitParachainHeadsCallBuilder<P, R, I>
where
	P: ParachainsPipeline,
	P::SourceChain: Chain<Hash = RelayBlockHash>,
	R: BridgeParachainsConfig<I> + Send + Sync,
	I: 'static + Send + Sync,
	R::BridgedChain: bp_runtime::Chain<
		BlockNumber = RelayBlockNumber,
		Hash = RelayBlockHash,
		Hasher = RelayBlockHasher,
	>,
	CallOf<P::TargetChain>: From<BridgeParachainsCall<R, I>>,
{
	fn build_submit_parachain_heads_call(
		relay_block_hash: HashOf<P::SourceChain>,
		parachains: Vec<ParaId>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<P::TargetChain> {
		BridgeParachainsCall::<R, I>::submit_parachain_heads {
			relay_block_hash,
			parachains,
			parachain_heads_proof,
		}
		.into()
	}
}

/// Substrate client as parachain heads source.
pub struct ParachainsTarget<P: ParachainsPipeline, S: TransactionSignScheme, CB> {
	client: Client<P::TargetChain>,
	transaction_params: TransactionParams<AccountKeyPairOf<S>>,
	bridge_paras_pallet_name: String,
	_phantom: PhantomData<CB>,
}

impl<P: ParachainsPipeline, S: TransactionSignScheme, CB> ParachainsTarget<P, S, CB> {
	/// Creates new parachains target client.
	pub fn new(
		client: Client<P::TargetChain>,
		transaction_params: TransactionParams<AccountKeyPairOf<S>>,
		bridge_paras_pallet_name: String,
	) -> Self {
		ParachainsTarget {
			client,
			transaction_params,
			bridge_paras_pallet_name,
			_phantom: Default::default(),
		}
	}
}

impl<P: ParachainsPipeline, S: TransactionSignScheme, CB> Clone for ParachainsTarget<P, S, CB> {
	fn clone(&self) -> Self {
		ParachainsTarget {
			client: self.client.clone(),
			transaction_params: self.transaction_params.clone(),
			bridge_paras_pallet_name: self.bridge_paras_pallet_name.clone(),
			_phantom: Default::default(),
		}
	}
}

#[async_trait]
impl<
		P: ParachainsPipeline,
		S: 'static + TransactionSignScheme,
		CB: SubmitParachainHeadsCallBuilder<P>,
	> RelayClient for ParachainsTarget<P, S, CB>
{
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		self.client.reconnect().await
	}
}

#[async_trait]
impl<P, S, CB> TargetClient<P> for ParachainsTarget<P, S, CB>
where
	P: ParachainsPipeline,
	S: 'static + TransactionSignScheme<Chain = P::TargetChain>,
	CB: SubmitParachainHeadsCallBuilder<P>,
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<S> as Pair>::Public>,
{
	async fn best_block(&self) -> Result<HeaderIdOf<P::TargetChain>, Self::Error> {
		let best_header = self.client.best_header().await?;
		let best_hash = best_header.hash();
		let best_id = HeaderId(*best_header.number(), best_hash);

		Ok(best_id)
	}

	async fn best_finalized_source_block(
		&self,
		at_block: &HeaderIdOf<P::TargetChain>,
	) -> Result<HeaderIdOf<P::SourceChain>, Self::Error> {
		let encoded_best_finalized_source_block = self
			.client
			.state_call(
				P::SourceChain::BEST_FINALIZED_HEADER_ID_METHOD.into(),
				Bytes(Vec::new()),
				Some(at_block.1),
			)
			.await?;
		let decoded_best_finalized_source_block: (
			BlockNumberOf<P::SourceChain>,
			HashOf<P::SourceChain>,
		) = Decode::decode(&mut &encoded_best_finalized_source_block.0[..])
			.map_err(SubstrateError::ResponseParseFailed)?;
		Ok(HeaderId(decoded_best_finalized_source_block.0, decoded_best_finalized_source_block.1))
	}

	async fn parachain_head(
		&self,
		at_block: HeaderIdOf<P::TargetChain>,
		para_id: ParaId,
	) -> Result<Option<BestParaHeadHash>, Self::Error> {
		let storage_key =
			parachain_head_storage_key_at_target(&self.bridge_paras_pallet_name, para_id);
		let para_head = self.client.storage_value(storage_key, Some(at_block.1)).await?;

		Ok(para_head)
	}

	async fn submit_parachain_heads_proof(
		&self,
		at_relay_block: HeaderIdOf<P::SourceChain>,
		updated_parachains: Vec<ParaId>,
		proof: ParaHeadsProof,
	) -> Result<(), Self::Error> {
		let genesis_hash = *self.client.genesis_hash();
		let transaction_params = self.transaction_params.clone();
		let (spec_version, transaction_version) = self.client.simple_runtime_version().await?;
		let call =
			CB::build_submit_parachain_heads_call(at_relay_block.1, updated_parachains, proof);
		self.client
			.submit_signed_extrinsic(
				self.transaction_params.signer.public().into(),
				move |best_block_id, transaction_nonce| {
					Ok(Bytes(
						S::sign_transaction(SignParam {
							spec_version,
							transaction_version,
							genesis_hash,
							signer: transaction_params.signer,
							era: TransactionEra::new(best_block_id, transaction_params.mortality),
							unsigned: UnsignedTransaction::new(call.into(), transaction_nonce),
						})?
						.encode(),
					))
				},
			)
			.await
			.map(drop)
	}
}
