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

//! Types and functions intended to ease adding of new Substrate -> Substrate
//! parachain finality proofs synchronization pipelines.

use async_trait::async_trait;
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use pallet_bridge_parachains::{
	Call as BridgeParachainsCall, Config as BridgeParachainsConfig, RelayBlockHash,
	RelayBlockHasher, RelayBlockNumber,
};
use parachains_relay::ParachainsPipeline;
use relay_substrate_client::{
	CallOf, Chain, ChainWithTransactions, HeaderIdOf, Parachain, RelayChain,
};
use std::{fmt::Debug, marker::PhantomData};

pub mod source;
pub mod target;

/// Substrate -> Substrate parachain finality proofs synchronization pipeline.
///
/// This is currently restricted to the single parachain, because it is how it
/// will be used (at least) initially.
#[async_trait]
pub trait SubstrateParachainsPipeline: 'static + Clone + Debug + Send + Sync {
	/// Headers of this parachain are submitted to the `Self::TargetChain`.
	type SourceParachain: Parachain;
	/// Relay chain that is storing headers of `Self::SourceParachain`.
	type SourceRelayChain: RelayChain;
	/// Target chain where `Self::SourceParachain` headers are submitted.
	type TargetChain: ChainWithTransactions;

	/// How submit parachains heads call is built?
	type SubmitParachainHeadsCallBuilder: SubmitParachainHeadsCallBuilder<Self>;
}

/// Adapter that allows all `SubstrateParachainsPipeline` to act as `ParachainsPipeline`.
#[derive(Clone, Debug)]
pub struct ParachainsPipelineAdapter<P: SubstrateParachainsPipeline> {
	_phantom: PhantomData<P>,
}

impl<P: SubstrateParachainsPipeline> ParachainsPipeline for ParachainsPipelineAdapter<P> {
	type SourceParachain = P::SourceParachain;
	type SourceRelayChain = P::SourceRelayChain;
	type TargetChain = P::TargetChain;
}

/// Different ways of building `submit_parachain_heads` calls.
pub trait SubmitParachainHeadsCallBuilder<P: SubstrateParachainsPipeline>:
	'static + Send + Sync
{
	/// Given parachains and their heads proof, build call of `submit_parachain_heads`
	/// function of bridge parachains module at the target chain.
	fn build_submit_parachain_heads_call(
		at_relay_block: HeaderIdOf<P::SourceRelayChain>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
		is_free_execution_expected: bool,
	) -> CallOf<P::TargetChain>;
}

/// Building `submit_parachain_heads` call when you have direct access to the target
/// chain runtime.
pub struct DirectSubmitParachainHeadsCallBuilder<P, R, I> {
	_phantom: PhantomData<(P, R, I)>,
}

impl<P, R, I> SubmitParachainHeadsCallBuilder<P> for DirectSubmitParachainHeadsCallBuilder<P, R, I>
where
	P: SubstrateParachainsPipeline,
	P::SourceRelayChain: Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber>,
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
		at_relay_block: HeaderIdOf<P::SourceRelayChain>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
		_is_free_execution_expected: bool,
	) -> CallOf<P::TargetChain> {
		BridgeParachainsCall::<R, I>::submit_parachain_heads {
			at_relay_block: (at_relay_block.0, at_relay_block.1),
			parachains,
			parachain_heads_proof,
		}
		.into()
	}
}
