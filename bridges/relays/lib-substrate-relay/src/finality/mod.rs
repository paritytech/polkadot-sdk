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
//! finality proofs synchronization pipelines.

use crate::{
	finality::{source::SubstrateFinalitySource, target::SubstrateFinalityTarget},
	finality_base::{engine::Engine, SubstrateFinalityPipeline, SubstrateFinalityProof},
	TransactionParams,
};

use async_trait::async_trait;
use bp_header_chain::justification::{GrandpaJustification, JustificationVerificationContext};
use finality_relay::{
	FinalityPipeline, FinalitySyncPipeline, HeadersToRelay, SourceClient, TargetClient,
};
use pallet_bridge_grandpa::{Call as BridgeGrandpaCall, Config as BridgeGrandpaConfig};
use relay_substrate_client::{
	transaction_stall_timeout, AccountIdOf, AccountKeyPairOf, BlockNumberOf, CallOf, Chain,
	ChainWithTransactions, Client, HashOf, HeaderOf, SyncHeader,
};
use relay_utils::{metrics::MetricsParams, TrackedTransactionStatus, TransactionTracker};
use sp_core::Pair;
use std::{fmt::Debug, marker::PhantomData};

pub mod initialize;
pub mod source;
pub mod target;

/// Default limit of recent finality proofs.
///
/// Finality delay of 4096 blocks is unlikely to happen in practice in
/// Substrate+GRANDPA based chains (good to know).
pub(crate) const RECENT_FINALITY_PROOFS_LIMIT: usize = 4096;

/// Convenience trait that adds bounds to `SubstrateFinalitySyncPipeline`.
pub trait BaseSubstrateFinalitySyncPipeline:
	SubstrateFinalityPipeline<TargetChain = Self::BoundedTargetChain>
{
	/// Bounded `SubstrateFinalityPipeline::TargetChain`.
	type BoundedTargetChain: ChainWithTransactions<AccountId = Self::BoundedTargetChainAccountId>;

	/// Bounded `AccountIdOf<SubstrateFinalityPipeline::TargetChain>`.
	type BoundedTargetChainAccountId: From<<AccountKeyPairOf<Self::BoundedTargetChain> as Pair>::Public>
		+ Send;
}

impl<T> BaseSubstrateFinalitySyncPipeline for T
where
	T: SubstrateFinalityPipeline,
	T::TargetChain: ChainWithTransactions,
	AccountIdOf<T::TargetChain>: From<<AccountKeyPairOf<Self::TargetChain> as Pair>::Public>,
{
	type BoundedTargetChain = T::TargetChain;
	type BoundedTargetChainAccountId = AccountIdOf<T::TargetChain>;
}

/// Substrate -> Substrate finality proofs synchronization pipeline.
#[async_trait]
pub trait SubstrateFinalitySyncPipeline: BaseSubstrateFinalitySyncPipeline {
	/// How submit finality proof call is built?
	type SubmitFinalityProofCallBuilder: SubmitFinalityProofCallBuilder<Self>;

	/// Add relay guards if required.
	async fn start_relay_guards(
		target_client: &Client<Self::TargetChain>,
		enable_version_guard: bool,
	) -> relay_substrate_client::Result<()> {
		if enable_version_guard {
			relay_substrate_client::guard::abort_on_spec_version_change(
				target_client.clone(),
				target_client.simple_runtime_version().await?.spec_version,
			);
		}
		Ok(())
	}
}

/// Adapter that allows all `SubstrateFinalitySyncPipeline` to act as `FinalitySyncPipeline`.
#[derive(Clone, Debug)]
pub struct FinalitySyncPipelineAdapter<P: SubstrateFinalitySyncPipeline> {
	_phantom: PhantomData<P>,
}

impl<P: SubstrateFinalitySyncPipeline> FinalityPipeline for FinalitySyncPipelineAdapter<P> {
	const SOURCE_NAME: &'static str = P::SourceChain::NAME;
	const TARGET_NAME: &'static str = P::TargetChain::NAME;

	type Hash = HashOf<P::SourceChain>;
	type Number = BlockNumberOf<P::SourceChain>;
	type FinalityProof = SubstrateFinalityProof<P>;
}

impl<P: SubstrateFinalitySyncPipeline> FinalitySyncPipeline for FinalitySyncPipelineAdapter<P> {
	type ConsensusLogReader = <P::FinalityEngine as Engine<P::SourceChain>>::ConsensusLogReader;
	type Header = SyncHeader<HeaderOf<P::SourceChain>>;
}

/// Different ways of building `submit_finality_proof` calls.
pub trait SubmitFinalityProofCallBuilder<P: SubstrateFinalitySyncPipeline> {
	/// Given source chain header, its finality proof and the current authority set id, build call
	/// of `submit_finality_proof` function of bridge GRANDPA module at the target chain.
	fn build_submit_finality_proof_call(
		header: SyncHeader<HeaderOf<P::SourceChain>>,
		proof: SubstrateFinalityProof<P>,
		is_free_execution_expected: bool,
		context: <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<P::SourceChain>>::FinalityVerificationContext,
	) -> CallOf<P::TargetChain>;
}

/// Building `submit_finality_proof` call when you have direct access to the target
/// chain runtime.
pub struct DirectSubmitGrandpaFinalityProofCallBuilder<P, R, I> {
	_phantom: PhantomData<(P, R, I)>,
}

impl<P, R, I> SubmitFinalityProofCallBuilder<P>
	for DirectSubmitGrandpaFinalityProofCallBuilder<P, R, I>
where
	P: SubstrateFinalitySyncPipeline,
	R: BridgeGrandpaConfig<I>,
	I: 'static,
	R::BridgedChain: bp_runtime::Chain<Header = HeaderOf<P::SourceChain>>,
	CallOf<P::TargetChain>: From<BridgeGrandpaCall<R, I>>,
	P::FinalityEngine: Engine<
		P::SourceChain,
		FinalityProof = GrandpaJustification<HeaderOf<P::SourceChain>>,
		FinalityVerificationContext = JustificationVerificationContext,
	>,
{
	fn build_submit_finality_proof_call(
		header: SyncHeader<HeaderOf<P::SourceChain>>,
		proof: GrandpaJustification<HeaderOf<P::SourceChain>>,
		_is_free_execution_expected: bool,
		_context: JustificationVerificationContext,
	) -> CallOf<P::TargetChain> {
		BridgeGrandpaCall::<R, I>::submit_finality_proof {
			finality_target: Box::new(header.into_inner()),
			justification: proof,
		}
		.into()
	}
}

/// Macro that generates `SubmitFinalityProofCallBuilder` implementation for the case when
/// you only have an access to the mocked version of target chain runtime. In this case you
/// should provide "name" of the call variant for the bridge GRANDPA calls and the "name" of
/// the variant for the `submit_finality_proof` call within that first option.
#[rustfmt::skip]
#[macro_export]
macro_rules! generate_submit_finality_proof_call_builder {
	($pipeline:ident, $mocked_builder:ident, $bridge_grandpa:path, $submit_finality_proof:path) => {
		pub struct $mocked_builder;

		impl $crate::finality::SubmitFinalityProofCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_submit_finality_proof_call(
				header: relay_substrate_client::SyncHeader<
					relay_substrate_client::HeaderOf<
						<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::SourceChain
					>
				>,
				proof: bp_header_chain::justification::GrandpaJustification<
					relay_substrate_client::HeaderOf<
						<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::SourceChain
					>
				>,
				_is_free_execution_expected: bool,
				_context: bp_header_chain::justification::JustificationVerificationContext,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::TargetChain
			> {
				bp_runtime::paste::item! {
					$bridge_grandpa($submit_finality_proof {
						finality_target: Box::new(header.into_inner()),
						justification: proof
					})
				}
			}
		}
	};
}

/// Macro that generates `SubmitFinalityProofCallBuilder` implementation for the case when
/// you only have an access to the mocked version of target chain runtime. In this case you
/// should provide "name" of the call variant for the bridge GRANDPA calls and the "name" of
/// the variant for the `submit_finality_proof_ex` call within that first option.
#[rustfmt::skip]
#[macro_export]
macro_rules! generate_submit_finality_proof_ex_call_builder {
	($pipeline:ident, $mocked_builder:ident, $bridge_grandpa:path, $submit_finality_proof:path) => {
		pub struct $mocked_builder;

		impl $crate::finality::SubmitFinalityProofCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_submit_finality_proof_call(
				header: relay_substrate_client::SyncHeader<
					relay_substrate_client::HeaderOf<
						<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::SourceChain
					>
				>,
				proof: bp_header_chain::justification::GrandpaJustification<
					relay_substrate_client::HeaderOf<
						<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::SourceChain
					>
				>,
				is_free_execution_expected: bool,
				context: bp_header_chain::justification::JustificationVerificationContext,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::TargetChain
			> {
				bp_runtime::paste::item! {
					$bridge_grandpa($submit_finality_proof {
						finality_target: Box::new(header.into_inner()),
						justification: proof,
						current_set_id: context.authority_set_id,
						is_free_execution_expected,
					})
				}
			}
		}
	};
}

/// Run Substrate-to-Substrate finality sync loop.
pub async fn run<P: SubstrateFinalitySyncPipeline>(
	source_client: Client<P::SourceChain>,
	target_client: Client<P::TargetChain>,
	headers_to_relay: HeadersToRelay,
	transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	metrics_params: MetricsParams,
) -> anyhow::Result<()> {
	log::info!(
		target: "bridge",
		"Starting {} -> {} finality proof relay: relaying {:?} headers",
		P::SourceChain::NAME,
		P::TargetChain::NAME,
		headers_to_relay,
	);

	finality_relay::run(
		SubstrateFinalitySource::<P>::new(source_client, None),
		SubstrateFinalityTarget::<P>::new(target_client, transaction_params.clone()),
		finality_relay::FinalitySyncParams {
			tick: std::cmp::max(
				P::SourceChain::AVERAGE_BLOCK_INTERVAL,
				P::TargetChain::AVERAGE_BLOCK_INTERVAL,
			),
			recent_finality_proofs_limit: RECENT_FINALITY_PROOFS_LIMIT,
			stall_timeout: transaction_stall_timeout(
				transaction_params.mortality,
				P::TargetChain::AVERAGE_BLOCK_INTERVAL,
				relay_utils::STALL_TIMEOUT,
			),
			headers_to_relay,
		},
		metrics_params,
		futures::future::pending(),
	)
	.await
	.map_err(|e| anyhow::format_err!("{}", e))
}

/// Relay single header. No checks are made to ensure that transaction will succeed.
pub async fn relay_single_header<P: SubstrateFinalitySyncPipeline>(
	source_client: Client<P::SourceChain>,
	target_client: Client<P::TargetChain>,
	transaction_params: TransactionParams<AccountKeyPairOf<P::TargetChain>>,
	header_number: BlockNumberOf<P::SourceChain>,
) -> anyhow::Result<()> {
	let finality_source = SubstrateFinalitySource::<P>::new(source_client, None);
	let (header, proof) = finality_source.header_and_finality_proof(header_number).await?;
	let Some(proof) = proof else {
		return Err(anyhow::format_err!(
			"Unable to submit {} header #{} to {}: no finality proof",
			P::SourceChain::NAME,
			header_number,
			P::TargetChain::NAME,
		));
	};

	let finality_target = SubstrateFinalityTarget::<P>::new(target_client, transaction_params);
	let tx_tracker = finality_target.submit_finality_proof(header, proof, false).await?;
	match tx_tracker.wait().await {
		TrackedTransactionStatus::Finalized(_) => Ok(()),
		TrackedTransactionStatus::Lost => Err(anyhow::format_err!(
			"Transaction with {} header #{} is considered lost at {}",
			P::SourceChain::NAME,
			header_number,
			P::TargetChain::NAME,
		)),
	}
}
