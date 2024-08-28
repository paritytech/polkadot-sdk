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
#[cfg_attr(test, derive(Debug, PartialEq))]
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
#[cfg_attr(test, derive(Debug))]
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
#[cfg_attr(test, derive(Debug))]
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
				Ok(equivocations) =>
					if !equivocations.is_empty() {
						result.push(ReportEquivocations {
							source_block_hash: self.context.synced_header_hash,
							equivocations,
						})
					},
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
#[cfg_attr(test, derive(Debug))]
pub struct ReportEquivocations<P: EquivocationDetectionPipeline> {
	source_block_hash: P::Hash,
	equivocations: Vec<P::EquivocationProof>,
}

impl<P: EquivocationDetectionPipeline> ReportEquivocations<P> {
	pub async fn next<SC: SourceClient<P>>(
		mut self,
		source_client: &mut SC,
		reporter: &mut EquivocationsReporter<'_, P, SC>,
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
#[cfg_attr(test, derive(Debug))]
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use std::collections::HashMap;

	impl PartialEq for ReadContext<TestEquivocationDetectionPipeline> {
		fn eq(&self, other: &Self) -> bool {
			self.target_block_num == other.target_block_num &&
				self.synced_headers == other.synced_headers
		}
	}

	impl PartialEq for FindEquivocations<TestEquivocationDetectionPipeline> {
		fn eq(&self, other: &Self) -> bool {
			self.target_block_num == other.target_block_num &&
				self.synced_headers == other.synced_headers &&
				self.context == other.context
		}
	}

	impl PartialEq for ReportEquivocations<TestEquivocationDetectionPipeline> {
		fn eq(&self, other: &Self) -> bool {
			self.source_block_hash == other.source_block_hash &&
				self.equivocations == other.equivocations
		}
	}

	impl PartialEq for BlockChecker<TestEquivocationDetectionPipeline> {
		fn eq(&self, _other: &Self) -> bool {
			matches!(self, _other)
		}
	}

	#[async_std::test]
	async fn block_checker_works() {
		let mut source_client = TestSourceClient { ..Default::default() };
		let mut target_client = TestTargetClient {
			best_synced_header_hash: HashMap::from([(9, Ok(Some(5)))]),
			finality_verification_context: HashMap::from([(
				9,
				Ok(TestFinalityVerificationContext { check_equivocations: true }),
			)]),
			synced_headers_finality_info: HashMap::from([(
				10,
				Ok(vec![
					new_header_finality_info(6, None),
					new_header_finality_info(7, Some(false)),
					new_header_finality_info(8, None),
					new_header_finality_info(9, Some(true)),
					new_header_finality_info(10, None),
					new_header_finality_info(11, None),
					new_header_finality_info(12, None),
				]),
			)]),
			..Default::default()
		};
		let mut reporter =
			EquivocationsReporter::<TestEquivocationDetectionPipeline, TestSourceClient>::new();

		let block_checker = BlockChecker::new(10);
		assert!(block_checker
			.run(
				&mut source_client,
				&mut target_client,
				&mut FinalityProofsBuf::new(vec![
					TestFinalityProof(6, vec!["6-1"]),
					TestFinalityProof(7, vec![]),
					TestFinalityProof(8, vec!["8-1"]),
					TestFinalityProof(9, vec!["9-1"]),
					TestFinalityProof(10, vec![]),
					TestFinalityProof(11, vec!["11-1", "11-2"]),
					TestFinalityProof(12, vec!["12-1"])
				]),
				&mut reporter
			)
			.await
			.is_ok());
		assert_eq!(
			*source_client.reported_equivocations.lock().unwrap(),
			HashMap::from([(5, vec!["6-1"]), (9, vec!["11-1", "11-2", "12-1"])])
		);
	}

	#[async_std::test]
	async fn block_checker_works_with_empty_context() {
		let mut target_client = TestTargetClient {
			best_synced_header_hash: HashMap::from([(9, Ok(None))]),
			finality_verification_context: HashMap::from([(
				9,
				Ok(TestFinalityVerificationContext { check_equivocations: true }),
			)]),
			synced_headers_finality_info: HashMap::from([(
				10,
				Ok(vec![new_header_finality_info(6, None)]),
			)]),
			..Default::default()
		};
		let mut source_client = TestSourceClient { ..Default::default() };
		let mut reporter =
			EquivocationsReporter::<TestEquivocationDetectionPipeline, TestSourceClient>::new();

		let block_checker = BlockChecker::new(10);
		assert!(block_checker
			.run(
				&mut source_client,
				&mut target_client,
				&mut FinalityProofsBuf::new(vec![TestFinalityProof(6, vec!["6-1"])]),
				&mut reporter
			)
			.await
			.is_ok());
		assert_eq!(*source_client.reported_equivocations.lock().unwrap(), HashMap::default());
	}

	#[async_std::test]
	async fn read_synced_headers_handles_errors() {
		let mut target_client = TestTargetClient {
			synced_headers_finality_info: HashMap::from([
				(10, Err(TestClientError::NonConnection)),
				(11, Err(TestClientError::Connection)),
			]),
			..Default::default()
		};
		let mut source_client = TestSourceClient { ..Default::default() };
		let mut reporter =
			EquivocationsReporter::<TestEquivocationDetectionPipeline, TestSourceClient>::new();

		// NonConnection error
		let block_checker = BlockChecker::new(10);
		assert_eq!(
			block_checker
				.run(
					&mut source_client,
					&mut target_client,
					&mut FinalityProofsBuf::new(vec![]),
					&mut reporter
				)
				.await,
			Err(BlockChecker::ReadSyncedHeaders(ReadSyncedHeaders { target_block_num: 10 }))
		);
		assert_eq!(target_client.num_reconnects, 0);

		// Connection error
		let block_checker = BlockChecker::new(11);
		assert_eq!(
			block_checker
				.run(
					&mut source_client,
					&mut target_client,
					&mut FinalityProofsBuf::new(vec![]),
					&mut reporter
				)
				.await,
			Err(BlockChecker::ReadSyncedHeaders(ReadSyncedHeaders { target_block_num: 11 }))
		);
		assert_eq!(target_client.num_reconnects, 1);
	}

	#[async_std::test]
	async fn read_context_handles_errors() {
		let mut target_client = TestTargetClient {
			synced_headers_finality_info: HashMap::from([(10, Ok(vec![])), (11, Ok(vec![]))]),
			best_synced_header_hash: HashMap::from([
				(9, Err(TestClientError::NonConnection)),
				(10, Err(TestClientError::Connection)),
			]),
			..Default::default()
		};
		let mut source_client = TestSourceClient { ..Default::default() };
		let mut reporter =
			EquivocationsReporter::<TestEquivocationDetectionPipeline, TestSourceClient>::new();

		// NonConnection error
		let block_checker = BlockChecker::new(10);
		assert_eq!(
			block_checker
				.run(
					&mut source_client,
					&mut target_client,
					&mut FinalityProofsBuf::new(vec![]),
					&mut reporter
				)
				.await,
			Err(BlockChecker::ReadContext(ReadContext {
				target_block_num: 10,
				synced_headers: vec![]
			}))
		);
		assert_eq!(target_client.num_reconnects, 0);

		// Connection error
		let block_checker = BlockChecker::new(11);
		assert_eq!(
			block_checker
				.run(
					&mut source_client,
					&mut target_client,
					&mut FinalityProofsBuf::new(vec![]),
					&mut reporter
				)
				.await,
			Err(BlockChecker::ReadContext(ReadContext {
				target_block_num: 11,
				synced_headers: vec![]
			}))
		);
		assert_eq!(target_client.num_reconnects, 1);
	}
}
