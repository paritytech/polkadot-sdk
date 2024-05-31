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

//! Types and functions intended to ease adding of new Substrate -> Substrate
//! equivocation detection pipelines.

mod source;
mod target;

use crate::{
	equivocation::{source::SubstrateEquivocationSource, target::SubstrateEquivocationTarget},
	finality_base::{engine::Engine, SubstrateFinalityPipeline, SubstrateFinalityProof},
	TransactionParams,
};

use async_trait::async_trait;
use bp_runtime::{AccountIdOf, BlockNumberOf, HashOf};
use equivocation_detector::EquivocationDetectionPipeline;
use finality_relay::FinalityPipeline;
use pallet_grandpa::{Call as GrandpaCall, Config as GrandpaConfig};
use relay_substrate_client::{AccountKeyPairOf, CallOf, Chain, ChainWithTransactions, Client};
use relay_utils::metrics::MetricsParams;
use sp_core::Pair;
use sp_runtime::traits::{Block, Header};
use std::marker::PhantomData;

/// Convenience trait that adds bounds to `SubstrateEquivocationDetectionPipeline`.
pub trait BaseSubstrateEquivocationDetectionPipeline:
	SubstrateFinalityPipeline<SourceChain = Self::BoundedSourceChain>
{
	/// Bounded `SubstrateFinalityPipeline::SourceChain`.
	type BoundedSourceChain: ChainWithTransactions<AccountId = Self::BoundedSourceChainAccountId>;

	/// Bounded `AccountIdOf<SubstrateFinalityPipeline::SourceChain>`.
	type BoundedSourceChainAccountId: From<<AccountKeyPairOf<Self::BoundedSourceChain> as Pair>::Public>
		+ Send;
}

impl<T> BaseSubstrateEquivocationDetectionPipeline for T
where
	T: SubstrateFinalityPipeline,
	T::SourceChain: ChainWithTransactions,
	AccountIdOf<T::SourceChain>: From<<AccountKeyPairOf<Self::SourceChain> as Pair>::Public>,
{
	type BoundedSourceChain = T::SourceChain;
	type BoundedSourceChainAccountId = AccountIdOf<T::SourceChain>;
}

/// Substrate -> Substrate equivocation detection pipeline.
#[async_trait]
pub trait SubstrateEquivocationDetectionPipeline:
	BaseSubstrateEquivocationDetectionPipeline
{
	/// How the `report_equivocation` call is built ?
	type ReportEquivocationCallBuilder: ReportEquivocationCallBuilder<Self>;

	/// Add relay guards if required.
	async fn start_relay_guards(
		source_client: &Client<Self::SourceChain>,
		enable_version_guard: bool,
	) -> relay_substrate_client::Result<()> {
		if enable_version_guard {
			relay_substrate_client::guard::abort_on_spec_version_change(
				source_client.clone(),
				source_client.simple_runtime_version().await?.spec_version,
			);
		}
		Ok(())
	}
}

type FinalityProoffOf<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::FinalityProof;
type FinalityVerificationContextfOf<P> =
	<<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
		<P as SubstrateFinalityPipeline>::SourceChain,
	>>::FinalityVerificationContext;
/// The type of the equivocation proof used by the `SubstrateEquivocationDetectionPipeline`
pub type EquivocationProofOf<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::EquivocationProof;
type EquivocationsFinderOf<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::EquivocationsFinder;
/// The type of the key owner proof used by the `SubstrateEquivocationDetectionPipeline`
pub type KeyOwnerProofOf<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::KeyOwnerProof;

/// Adapter that allows a `SubstrateEquivocationDetectionPipeline` to act as an
/// `EquivocationDetectionPipeline`.
#[derive(Clone, Debug)]
pub struct EquivocationDetectionPipelineAdapter<P: SubstrateEquivocationDetectionPipeline> {
	_phantom: PhantomData<P>,
}

impl<P: SubstrateEquivocationDetectionPipeline> FinalityPipeline
	for EquivocationDetectionPipelineAdapter<P>
{
	const SOURCE_NAME: &'static str = P::SourceChain::NAME;
	const TARGET_NAME: &'static str = P::TargetChain::NAME;

	type Hash = HashOf<P::SourceChain>;
	type Number = BlockNumberOf<P::SourceChain>;
	type FinalityProof = SubstrateFinalityProof<P>;
}

impl<P: SubstrateEquivocationDetectionPipeline> EquivocationDetectionPipeline
	for EquivocationDetectionPipelineAdapter<P>
{
	type TargetNumber = BlockNumberOf<P::TargetChain>;
	type FinalityVerificationContext = FinalityVerificationContextfOf<P>;
	type EquivocationProof = EquivocationProofOf<P>;
	type EquivocationsFinder = EquivocationsFinderOf<P>;
}

/// Different ways of building `report_equivocation` calls.
pub trait ReportEquivocationCallBuilder<P: SubstrateEquivocationDetectionPipeline> {
	/// Build a `report_equivocation` call to be executed on the source chain.
	fn build_report_equivocation_call(
		equivocation_proof: EquivocationProofOf<P>,
		key_owner_proof: KeyOwnerProofOf<P>,
	) -> CallOf<P::SourceChain>;
}

/// Building the `report_equivocation` call when having direct access to the target chain runtime.
pub struct DirectReportGrandpaEquivocationCallBuilder<P, R> {
	_phantom: PhantomData<(P, R)>,
}

impl<P, R> ReportEquivocationCallBuilder<P> for DirectReportGrandpaEquivocationCallBuilder<P, R>
where
	P: SubstrateEquivocationDetectionPipeline,
	P::FinalityEngine: Engine<
		P::SourceChain,
		EquivocationProof = sp_consensus_grandpa::EquivocationProof<
			HashOf<P::SourceChain>,
			BlockNumberOf<P::SourceChain>,
		>,
	>,
	R: frame_system::Config<Hash = HashOf<P::SourceChain>>
		+ GrandpaConfig<KeyOwnerProof = KeyOwnerProofOf<P>>,
	<R::Block as Block>::Header: Header<Number = BlockNumberOf<P::SourceChain>>,
	CallOf<P::SourceChain>: From<GrandpaCall<R>>,
{
	fn build_report_equivocation_call(
		equivocation_proof: EquivocationProofOf<P>,
		key_owner_proof: KeyOwnerProofOf<P>,
	) -> CallOf<P::SourceChain> {
		GrandpaCall::<R>::report_equivocation {
			equivocation_proof: Box::new(equivocation_proof),
			key_owner_proof,
		}
		.into()
	}
}

/// Macro that generates `ReportEquivocationCallBuilder` implementation for the case where
/// we only have access to the mocked version of the source chain runtime.
#[rustfmt::skip]
#[macro_export]
macro_rules! generate_report_equivocation_call_builder {
	($pipeline:ident, $mocked_builder:ident, $grandpa:path, $report_equivocation:path) => {
		pub struct $mocked_builder;

		impl $crate::equivocation::ReportEquivocationCallBuilder<$pipeline>
			for $mocked_builder
		{
			fn build_report_equivocation_call(
				equivocation_proof: $crate::equivocation::EquivocationProofOf<$pipeline>,
				key_owner_proof: $crate::equivocation::KeyOwnerProofOf<$pipeline>,
			) -> relay_substrate_client::CallOf<
				<$pipeline as $crate::finality_base::SubstrateFinalityPipeline>::SourceChain
			> {
				bp_runtime::paste::item! {
					$grandpa($report_equivocation {
						equivocation_proof: Box::new(equivocation_proof),
						key_owner_proof: key_owner_proof
					})
				}
			}
		}
	};
}

/// Run Substrate-to-Substrate equivocations detection loop.
pub async fn run<P: SubstrateEquivocationDetectionPipeline>(
	source_client: Client<P::SourceChain>,
	target_client: Client<P::TargetChain>,
	source_transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	metrics_params: MetricsParams,
) -> anyhow::Result<()> {
	log::info!(
		target: "bridge",
		"Starting {} -> {} equivocations detection loop",
		P::SourceChain::NAME,
		P::TargetChain::NAME,
	);

	equivocation_detector::run(
		SubstrateEquivocationSource::<P>::new(source_client, source_transaction_params),
		SubstrateEquivocationTarget::<P>::new(target_client),
		P::TargetChain::AVERAGE_BLOCK_INTERVAL,
		metrics_params,
		futures::future::pending(),
	)
	.await
	.map_err(|e| anyhow::format_err!("{}", e))
}
