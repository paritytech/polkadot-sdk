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

use crate::{
	reporter::EquivocationsReporter, EquivocationDetectionPipeline, HeaderFinalityInfo,
	SourceClient, TargetClient,
};

use bp_header_chain::{FinalityProof, FindEquivocations};
use finality_relay::{FinalityProofsBuf, FinalityProofsStream};
use futures::{select, FutureExt};
use num_traits::Saturating;
use relay_utils::{
	metrics::MetricsParams,
	relay_loop::{reconnect_failed_client, RECONNECT_DELAY},
	FailedClient, MaybeConnectionError,
};
use std::{future::Future, time::Duration};

/// The context needed for finding equivocations inside finality proofs and reporting them.
struct EquivocationReportingContext<P: EquivocationDetectionPipeline> {
	synced_header_hash: P::Hash,
	synced_verification_context: P::FinalityVerificationContext,
}

impl<P: EquivocationDetectionPipeline> EquivocationReportingContext<P> {
	/// Try to get the `EquivocationReportingContext` used by the target chain
	/// at the provided block.
	async fn try_read_from_target<TC: TargetClient<P>>(
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
	fn update(&mut self, info: HeaderFinalityInfo<P>) {
		if let Some(new_verification_context) = info.new_verification_context {
			self.synced_header_hash = info.finality_proof.target_header_hash();
			self.synced_verification_context = new_verification_context;
		}
	}
}

/// Equivocations detection loop state.
struct EquivocationDetectionLoop<
	P: EquivocationDetectionPipeline,
	SC: SourceClient<P>,
	TC: TargetClient<P>,
> {
	source_client: SC,
	target_client: TC,

	from_block_num: Option<P::TargetNumber>,
	until_block_num: Option<P::TargetNumber>,

	reporter: EquivocationsReporter<P, SC>,

	finality_proofs_stream: FinalityProofsStream<P, SC>,
	finality_proofs_buf: FinalityProofsBuf<P>,
}

impl<P: EquivocationDetectionPipeline, SC: SourceClient<P>, TC: TargetClient<P>>
	EquivocationDetectionLoop<P, SC, TC>
{
	async fn handle_source_error(&mut self, e: SC::Error) {
		if e.is_connection_error() {
			reconnect_failed_client(
				FailedClient::Source,
				RECONNECT_DELAY,
				&mut self.source_client,
				&mut self.target_client,
			)
			.await;
		} else {
			async_std::task::sleep(RECONNECT_DELAY).await;
		}
	}

	async fn handle_target_error(&mut self, e: TC::Error) {
		if e.is_connection_error() {
			reconnect_failed_client(
				FailedClient::Target,
				RECONNECT_DELAY,
				&mut self.source_client,
				&mut self.target_client,
			)
			.await;
		} else {
			async_std::task::sleep(RECONNECT_DELAY).await;
		}
	}

	async fn ensure_finality_proofs_stream(&mut self) {
		match self.finality_proofs_stream.ensure_stream(&self.source_client).await {
			Ok(_) => {},
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not connect to the {} `FinalityProofsStream`: {e:?}",
					P::SOURCE_NAME,
				);

				// Reconnect to the source client if needed
				self.handle_source_error(e).await
			},
		}
	}

	async fn best_finalized_target_block_number(&mut self) -> Option<P::TargetNumber> {
		match self.target_client.best_finalized_header_number().await {
			Ok(block_num) => Some(block_num),
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not read best finalized header number from {}: {e:?}",
					P::TARGET_NAME,
				);

				// Reconnect target client and move on
				self.handle_target_error(e).await;

				None
			},
		}
	}

	async fn build_equivocation_reporting_context(
		&mut self,
		block_num: P::TargetNumber,
	) -> Option<EquivocationReportingContext<P>> {
		match EquivocationReportingContext::try_read_from_target(
			&self.target_client,
			block_num.saturating_sub(1.into()),
		)
		.await
		{
			Ok(Some(context)) => Some(context),
			Ok(None) => None,
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not read {} `EquivocationReportingContext` from {} at block {block_num}: {e:?}",
					P::SOURCE_NAME,
					P::TARGET_NAME,
				);

				// Reconnect target client if needed and move on.
				self.handle_target_error(e).await;
				None
			},
		}
	}

	/// Try to get the finality info associated to the source headers synced with the target chain
	/// at the specified block.
	async fn synced_source_headers_at_target(
		&mut self,
		at: P::TargetNumber,
	) -> Vec<HeaderFinalityInfo<P>> {
		match self.target_client.synced_headers_finality_info(at).await {
			Ok(synced_headers) => synced_headers,
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not get {} headers synced to {} at block {at:?}",
					P::SOURCE_NAME,
					P::TARGET_NAME
				);

				// Reconnect in case of a connection error.
				self.handle_target_error(e).await;
				// And move on to the next block.
				vec![]
			},
		}
	}

	async fn report_equivocation(&mut self, at: P::Hash, equivocation: P::EquivocationProof) {
		match self.reporter.submit_report(&self.source_client, at, equivocation.clone()).await {
			Ok(_) => {},
			Err(e) => {
				log::error!(
					target: "bridge",
					"Could not submit equivocation report to {} for {equivocation:?}: {e:?}",
					P::SOURCE_NAME,
				);

				// Reconnect source client and move on
				self.handle_source_error(e).await;
			},
		}
	}

	async fn check_block(
		&mut self,
		block_num: P::TargetNumber,
		context: &mut EquivocationReportingContext<P>,
	) {
		let synced_headers = self.synced_source_headers_at_target(block_num).await;

		for synced_header in synced_headers {
			self.finality_proofs_buf.fill(&mut self.finality_proofs_stream);

			let equivocations = match P::EquivocationsFinder::find_equivocations(
				&context.synced_verification_context,
				&synced_header.finality_proof,
				self.finality_proofs_buf.buf().as_slice(),
			) {
				Ok(equivocations) => equivocations,
				Err(e) => {
					log::error!(
						target: "bridge",
						"Could not search for equivocations in the finality proof \
						for source header {:?} synced at target block {block_num:?}: {e:?}",
						synced_header.finality_proof.target_header_hash()
					);
					continue
				},
			};
			for equivocation in equivocations {
				self.report_equivocation(context.synced_header_hash, equivocation).await;
			}

			self.finality_proofs_buf
				.prune(synced_header.finality_proof.target_header_number(), None);
			context.update(synced_header);
		}
	}

	async fn do_run(&mut self, tick: Duration, exit_signal: impl Future<Output = ()>) {
		let exit_signal = exit_signal.fuse();
		futures::pin_mut!(exit_signal);

		loop {
			// Make sure that we are connected to the source finality proofs stream.
			self.ensure_finality_proofs_stream().await;
			// Check the status of the pending equivocation reports
			self.reporter.process_pending_reports().await;

			// Update blocks range.
			if let Some(block_number) = self.best_finalized_target_block_number().await {
				self.from_block_num.get_or_insert(block_number);
				self.until_block_num = Some(block_number);
			}
			let (from, until) = match (self.from_block_num, self.until_block_num) {
				(Some(from), Some(until)) => (from, until),
				_ => continue,
			};

			// Check the available blocks
			let mut current_block_number = from;
			while current_block_number <= until {
				let mut context =
					match self.build_equivocation_reporting_context(current_block_number).await {
						Some(context) => context,
						None => {
							current_block_number = current_block_number.saturating_add(1.into());
							continue
						},
					};
				self.check_block(current_block_number, &mut context).await;
				current_block_number = current_block_number.saturating_add(1.into());
			}
			self.until_block_num = Some(current_block_number);

			select! {
				_ = async_std::task::sleep(tick).fuse() => {},
				_ = exit_signal => return,
			}
		}
	}

	pub async fn run(
		source_client: SC,
		target_client: TC,
		tick: Duration,
		exit_signal: impl Future<Output = ()>,
	) -> Result<(), FailedClient> {
		let mut equivocation_detection_loop = Self {
			source_client,
			target_client,
			from_block_num: None,
			until_block_num: None,
			reporter: EquivocationsReporter::<P, SC>::new(),
			finality_proofs_stream: FinalityProofsStream::new(),
			finality_proofs_buf: FinalityProofsBuf::new(vec![]),
		};

		equivocation_detection_loop.do_run(tick, exit_signal).await;
		Ok(())
	}
}

/// Spawn the equivocations detection loop.
pub async fn run<P: EquivocationDetectionPipeline>(
	source_client: impl SourceClient<P>,
	target_client: impl TargetClient<P>,
	tick: Duration,
	metrics_params: MetricsParams,
	exit_signal: impl Future<Output = ()> + 'static + Send,
) -> Result<(), relay_utils::Error> {
	let exit_signal = exit_signal.shared();
	relay_utils::relay_loop(source_client, target_client)
		.with_metrics(metrics_params)
		.expose()
		.await?
		.run(
			format!("{}_to_{}_EquivocationDetection", P::SOURCE_NAME, P::TARGET_NAME),
			move |source_client, target_client, _metrics| {
				EquivocationDetectionLoop::run(
					source_client,
					target_client,
					tick,
					exit_signal.clone(),
				)
			},
		)
		.await
}
