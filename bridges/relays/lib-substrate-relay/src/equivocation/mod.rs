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

use crate::finality_base::SubstrateFinalityPipeline;
use std::marker::PhantomData;

use crate::finality_base::engine::Engine;
use async_trait::async_trait;
use bp_runtime::{BlockNumberOf, HashOf};
use pallet_grandpa::{Call as GrandpaCall, Config as GrandpaConfig};
use relay_substrate_client::CallOf;
use sp_runtime::traits::{Block, Header};

/// Substrate -> Substrate equivocation detection pipeline.
#[async_trait]
pub trait SubstrateEquivocationDetectionPipeline: SubstrateFinalityPipeline {
	/// How the `report_equivocation` call is built ?
	type ReportEquivocationCallBuilder: ReportEquivocationCallBuilder<Self>;
}

type EquivocationProofOf<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::EquivocationProof;
type KeyOwnerProofOf<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::KeyOwnerProof;

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
