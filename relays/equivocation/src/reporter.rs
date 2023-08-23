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

//! Helper struct used for submitting finality reports and tracking their status.

use crate::{EquivocationDetectionPipeline, SourceClient};

use futures::FutureExt;
use relay_utils::{TrackedTransactionFuture, TrackedTransactionStatus, TransactionTracker};
use std::{
	future::poll_fn,
	task::{Context, Poll},
};

pub struct EquivocationsReporter<P: EquivocationDetectionPipeline, SC: SourceClient<P>> {
	pending_reports: Vec<TrackedTransactionFuture<SC::TransactionTracker>>,
}

impl<P: EquivocationDetectionPipeline, SC: SourceClient<P>> EquivocationsReporter<P, SC> {
	pub fn new() -> Self {
		Self { pending_reports: vec![] }
	}

	/// Submit a `report_equivocation()` transaction to the source chain.
	///
	/// We store the transaction tracker for future monitoring.
	pub async fn submit_report(
		&mut self,
		source_client: &SC,
		at: P::Hash,
		equivocation: P::EquivocationProof,
	) -> Result<(), SC::Error> {
		let pending_report = source_client.report_equivocation(at, equivocation).await?;
		self.pending_reports.push(pending_report.wait());

		Ok(())
	}

	fn do_process_pending_reports(&mut self, cx: &mut Context<'_>) -> Poll<()> {
		self.pending_reports.retain_mut(|pending_report| {
			match pending_report.poll_unpin(cx) {
				Poll::Ready(tx_status) => {
					match tx_status {
						TrackedTransactionStatus::Lost => {
							log::error!(target: "bridge", "Equivocation report tx was lost");
						},
						TrackedTransactionStatus::Finalized(id) => {
							log::error!(target: "bridge", "Equivocation report tx was finalized in source block {id:?}");
						},
					}

					// The future was processed. Drop it.
					false
				},
				Poll::Pending => {
					// The future is still pending. Retain it.
					true
				},
			}
		});

		Poll::Ready(())
	}

	/// Iterate through all the pending `report_equivocation()` transactions
	/// and log the ones that finished.
	pub async fn process_pending_reports(&mut self) {
		poll_fn(|cx| self.do_process_pending_reports(cx)).await
	}
}
