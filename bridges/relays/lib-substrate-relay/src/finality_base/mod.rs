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
//! finality pipelines.

pub mod engine;

use crate::finality_base::engine::Engine;

use async_trait::async_trait;
use bp_runtime::{HashOf, HeaderIdOf};
use codec::Decode;
use futures::{stream::unfold, Stream, StreamExt};
use relay_substrate_client::{Chain, Client, Error};
use std::{fmt::Debug, pin::Pin};

/// Substrate -> Substrate finality related pipeline.
#[async_trait]
pub trait SubstrateFinalityPipeline: 'static + Clone + Debug + Send + Sync {
	/// Headers of this chain are submitted to the `TargetChain`.
	type SourceChain: Chain;
	/// Headers of the `SourceChain` are submitted to this chain.
	type TargetChain: Chain;
	/// Finality engine.
	type FinalityEngine: Engine<Self::SourceChain>;
}

/// Substrate finality proof. Specific to the used `FinalityEngine`.
pub type SubstrateFinalityProof<P> = <<P as SubstrateFinalityPipeline>::FinalityEngine as Engine<
	<P as SubstrateFinalityPipeline>::SourceChain,
>>::FinalityProof;

/// Substrate finality proofs stream.
pub type SubstrateFinalityProofsStream<P> =
	Pin<Box<dyn Stream<Item = SubstrateFinalityProof<P>> + Send>>;

/// Subscribe to new finality proofs.
pub async fn finality_proofs<P: SubstrateFinalityPipeline>(
	client: &Client<P::SourceChain>,
) -> Result<SubstrateFinalityProofsStream<P>, Error> {
	Ok(unfold(
		P::FinalityEngine::source_finality_proofs(client).await?,
		move |subscription| async move {
			loop {
				let log_error = |err| {
					log::error!(
						target: "bridge",
						"Failed to read justification target from the {} justifications stream: {:?}",
						P::SourceChain::NAME,
						err,
					);
				};

				let next_justification =
					subscription.next().await.map_err(|err| log_error(err.to_string())).ok()??;

				let decoded_justification =
					<P::FinalityEngine as Engine<P::SourceChain>>::FinalityProof::decode(
						&mut &next_justification[..],
					);

				let justification = match decoded_justification {
					Ok(j) => j,
					Err(err) => {
						log_error(format!("decode failed with error {err:?}"));
						continue
					},
				};

				return Some((justification, subscription))
			}
		},
	)
	.boxed())
}

/// Get the id of the best `SourceChain` header known to the `TargetChain` at the provided
/// target block using the exposed runtime API method.
///
/// The runtime API method should be `<TargetChain>FinalityApi::best_finalized()`.
pub async fn best_synced_header_id<SourceChain, TargetChain>(
	target_client: &Client<TargetChain>,
	at: HashOf<TargetChain>,
) -> Result<Option<HeaderIdOf<SourceChain>>, Error>
where
	SourceChain: Chain,
	TargetChain: Chain,
{
	// now let's read id of best finalized peer header at our best finalized block
	target_client
		.typed_state_call(SourceChain::BEST_FINALIZED_HEADER_ID_METHOD.into(), (), Some(at))
		.await
}
