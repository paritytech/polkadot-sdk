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

mod block_checker;
mod equivocation_loop;
mod mock;
mod reporter;

use async_trait::async_trait;
use bp_header_chain::{FinalityProof, FindEquivocations};
use finality_relay::{FinalityPipeline, SourceClientBase};
use relay_utils::{relay_loop::Client as RelayClient, MaybeConnectionError, TransactionTracker};
use std::{fmt::Debug, time::Duration};

pub use equivocation_loop::run;

#[cfg(not(test))]
const RECONNECT_DELAY: Duration = relay_utils::relay_loop::RECONNECT_DELAY;
#[cfg(test)]
const RECONNECT_DELAY: Duration = mock::TEST_RECONNECT_DELAY;

pub trait EquivocationDetectionPipeline: FinalityPipeline {
	/// Block number of the target chain.
	type TargetNumber: relay_utils::BlockNumberBase;
	/// The context needed for validating finality proofs.
	type FinalityVerificationContext: Debug + Send;
	/// The type of the equivocation proof.
	type EquivocationProof: Clone + Debug + Send + Sync;
	/// The equivocations finder.
	type EquivocationsFinder: FindEquivocations<
		Self::FinalityProof,
		Self::FinalityVerificationContext,
		Self::EquivocationProof,
	>;
}

type HeaderFinalityInfo<P> = bp_header_chain::HeaderFinalityInfo<
	<P as FinalityPipeline>::FinalityProof,
	<P as EquivocationDetectionPipeline>::FinalityVerificationContext,
>;

/// Source client used in equivocation detection loop.
#[async_trait]
pub trait SourceClient<P: EquivocationDetectionPipeline>: SourceClientBase<P> {
	/// Transaction tracker to track submitted transactions.
	type TransactionTracker: TransactionTracker;

	/// Report equivocation.
	async fn report_equivocation(
		&self,
		at: P::Hash,
		equivocation: P::EquivocationProof,
	) -> Result<Self::TransactionTracker, Self::Error>;
}

/// Target client used in equivocation detection loop.
#[async_trait]
pub trait TargetClient<P: EquivocationDetectionPipeline>: RelayClient {
	/// Get the best finalized header number.
	async fn best_finalized_header_number(&self) -> Result<P::TargetNumber, Self::Error>;

	/// Get the hash of the best source header known by the target at the provided block number.
	async fn best_synced_header_hash(
		&self,
		at: P::TargetNumber,
	) -> Result<Option<P::Hash>, Self::Error>;

	/// Get the data stored by the target at the specified block for validating source finality
	/// proofs.
	async fn finality_verification_context(
		&self,
		at: P::TargetNumber,
	) -> Result<P::FinalityVerificationContext, Self::Error>;

	/// Get the finality info associated to the source headers synced with the target chain at the
	/// specified block.
	async fn synced_headers_finality_info(
		&self,
		at: P::TargetNumber,
	) -> Result<Vec<HeaderFinalityInfo<P>>, Self::Error>;
}

/// The context needed for finding equivocations inside finality proofs and reporting them.
#[derive(Debug, PartialEq)]
struct EquivocationReportingContext<P: EquivocationDetectionPipeline> {
	pub synced_header_hash: P::Hash,
	pub synced_verification_context: P::FinalityVerificationContext,
}

impl<P: EquivocationDetectionPipeline> EquivocationReportingContext<P> {
	/// Try to get the `EquivocationReportingContext` used by the target chain
	/// at the provided block.
	pub async fn try_read_from_target<TC: TargetClient<P>>(
		target_client: &TC,
		at: P::TargetNumber,
	) -> Result<Option<Self>, TC::Error> {
		let maybe_best_synced_header_hash = target_client.best_synced_header_hash(at).await?;
		Ok(match maybe_best_synced_header_hash {
			Some(best_synced_header_hash) => Some(EquivocationReportingContext {
				synced_header_hash: best_synced_header_hash,
				synced_verification_context: target_client
					.finality_verification_context(at)
					.await?,
			}),
			None => None,
		})
	}

	/// Update with the new context introduced by the `HeaderFinalityInfo<P>` if any.
	pub fn update(&mut self, info: HeaderFinalityInfo<P>) {
		if let Some(new_verification_context) = info.new_verification_context {
			self.synced_header_hash = info.finality_proof.target_header_hash();
			self.synced_verification_context = new_verification_context;
		}
	}
}

async fn handle_client_error<C: RelayClient>(client: &mut C, e: C::Error) {
	if e.is_connection_error() {
		client.reconnect_until_success(RECONNECT_DELAY).await;
	} else {
		async_std::task::sleep(RECONNECT_DELAY).await;
	}
}
