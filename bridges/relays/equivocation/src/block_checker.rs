// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{
	handle_client_error, reporter::EquivocationsReporter, EquivocationDetectionPipeline,
	EquivocationReportingContext, HeaderFinalityInfo, SourceClient, TargetClient,
};

use bp_header_chain::{FinalityProof, FindEquivocations as FindEquivocationsT};
use finality_relay::FinalityProofsBuf;
use futures::future::{BoxFuture, FutureExt};
use num_traits::Saturating;

/// First step in the block checking state machine.
///
/// Getting the finality info associated to the source headers synced with the target chain
/// at the specified block.
pub struct ReadSyncedHeaders<P: EquivocationDetectionPipeline> {
	pub target_block_num: P::TargetNumber,
}

impl<P: EquivocationDetectionPipeline> ReadSyncedHeaders<P> {
	pub async fn next<TC: TargetClient<P>>(
		self,
		target_client: &mut TC,
	) -> Result<ReadContext<P>, Self> {
		match target_client.synced_headers_finality_info(self.target_block_num).await {
			Ok(synced_headers) =>
				Ok(ReadContext { target_block_num: self.target_block_num, synced_headers }),
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not get {} headers synced to {} at block {}: {e:?}",
					P::SOURCE_NAME,
					P::TARGET_NAME,
					self.target_block_num
				);

				// Reconnect target client in case of a connection error.
				handle_client_error(target_client, e).await;

				Err(self)
			},
		}
	}
}

/// Second step in the block checking state machine.
///
/// Reading the equivocation reporting context from the target chain.
pub struct ReadContext<P: EquivocationDetectionPipeline> {
	target_block_num: P::TargetNumber,
	synced_headers: Vec<HeaderFinalityInfo<P>>,
}

impl<P: EquivocationDetectionPipeline> ReadContext<P> {
	pub async fn next<TC: TargetClient<P>>(
		self,
		target_client: &mut TC,
	) -> Result<Option<FindEquivocations<P>>, Self> {
		match EquivocationReportingContext::try_read_from_target::<TC>(
			target_client,
			self.target_block_num.saturating_sub(1.into()),
		)
		.await
		{
			Ok(Some(context)) => Ok(Some(FindEquivocations {
				target_block_num: self.target_block_num,
				synced_headers: self.synced_headers,
				context,
			})),
			Ok(None) => Ok(None),
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not read {} `EquivocationReportingContext` from {} at block {}: {e:?}",
					P::SOURCE_NAME,
					P::TARGET_NAME,
					self.target_block_num.saturating_sub(1.into()),
				);

				// Reconnect target client in case of a connection error.
				handle_client_error(target_client, e).await;

				Err(self)
			},
		}
	}
}

/// Third step in the block checking state machine.
///
/// Searching for equivocations in the source headers synced with the target chain.
pub struct FindEquivocations<P: EquivocationDetectionPipeline> {
	target_block_num: P::TargetNumber,
	synced_headers: Vec<HeaderFinalityInfo<P>>,
	context: EquivocationReportingContext<P>,
}

impl<P: EquivocationDetectionPipeline> FindEquivocations<P> {
	pub fn next(
		mut self,
		finality_proofs_buf: &mut FinalityProofsBuf<P>,
	) -> Vec<ReportEquivocations<P>> {
		let mut result = vec![];
		for synced_header in self.synced_headers {
			match P::EquivocationsFinder::find_equivocations(
				&self.context.synced_verification_context,
				&synced_header.finality_proof,
				finality_proofs_buf.buf().as_slice(),
			) {
				Ok(equivocations) => result.push(ReportEquivocations {
					source_block_hash: self.context.synced_header_hash,
					equivocations,
				}),
				Err(e) => {
					log::error!(
						target: "bridge",
						"Could not search for equivocations in the finality proof \
						for source header {:?} synced at target block {}: {e:?}",
						synced_header.finality_proof.target_header_hash(),
						self.target_block_num
					);
				},
			};

			finality_proofs_buf.prune(synced_header.finality_proof.target_header_number(), None);
			self.context.update(synced_header);
		}

		result
	}
}

/// Fourth step in the block checking state machine.
///
/// Reporting the detected equivocations (if any).
pub struct ReportEquivocations<P: EquivocationDetectionPipeline> {
	source_block_hash: P::Hash,
	equivocations: Vec<P::EquivocationProof>,
}

impl<P: EquivocationDetectionPipeline> ReportEquivocations<P> {
	pub async fn next<SC: SourceClient<P>>(
		mut self,
		source_client: &mut SC,
		reporter: &mut EquivocationsReporter<P, SC>,
	) -> Result<(), Self> {
		let mut unprocessed_equivocations = vec![];
		for equivocation in self.equivocations {
			match reporter
				.submit_report(source_client, self.source_block_hash, equivocation.clone())
				.await
			{
				Ok(_) => {},
				Err(e) => {
					log::error!(
						target: "bridge",
						"Could not submit equivocation report to {} for {equivocation:?}: {e:?}",
						P::SOURCE_NAME,
					);

					// Mark the equivocation as unprocessed
					unprocessed_equivocations.push(equivocation);
					// Reconnect source client in case of a connection error.
					handle_client_error(source_client, e).await;
				},
			}
		}

		self.equivocations = unprocessed_equivocations;
		if !self.equivocations.is_empty() {
			return Err(self)
		}

		Ok(())
	}
}

/// Block checking state machine.
pub enum BlockChecker<P: EquivocationDetectionPipeline> {
	ReadSyncedHeaders(ReadSyncedHeaders<P>),
	ReadContext(ReadContext<P>),
	ReportEquivocations(Vec<ReportEquivocations<P>>),
}

impl<P: EquivocationDetectionPipeline> BlockChecker<P> {
	pub fn new(target_block_num: P::TargetNumber) -> Self {
		Self::ReadSyncedHeaders(ReadSyncedHeaders { target_block_num })
	}

	pub fn run<'a, SC: SourceClient<P>, TC: TargetClient<P>>(
		self,
		source_client: &'a mut SC,
		target_client: &'a mut TC,
		finality_proofs_buf: &'a mut FinalityProofsBuf<P>,
		reporter: &'a mut EquivocationsReporter<P, SC>,
	) -> BoxFuture<'a, Result<(), Self>> {
		async move {
			match self {
				Self::ReadSyncedHeaders(state) => {
					let read_context =
						state.next(target_client).await.map_err(Self::ReadSyncedHeaders)?;
					Self::ReadContext(read_context)
						.run(source_client, target_client, finality_proofs_buf, reporter)
						.await
				},
				Self::ReadContext(state) => {
					let maybe_find_equivocations =
						state.next(target_client).await.map_err(Self::ReadContext)?;
					let find_equivocations = match maybe_find_equivocations {
						Some(find_equivocations) => find_equivocations,
						None => return Ok(()),
					};
					Self::ReportEquivocations(find_equivocations.next(finality_proofs_buf))
						.run(source_client, target_client, finality_proofs_buf, reporter)
						.await
				},
				Self::ReportEquivocations(state) => {
					let mut failures = vec![];
					for report_equivocations in state {
						if let Err(failure) =
							report_equivocations.next(source_client, reporter).await
						{
							failures.push(failure);
						}
					}

					if !failures.is_empty() {
						return Err(Self::ReportEquivocations(failures))
					}

					Ok(())
				},
			}
		}
		.boxed()
	}
}
