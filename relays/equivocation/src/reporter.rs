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

pub struct EquivocationsReporter<'a, P: EquivocationDetectionPipeline, SC: SourceClient<P>> {
	pending_reports: Vec<TrackedTransactionFuture<'a, SC::TransactionTracker>>,
}

impl<'a, P: EquivocationDetectionPipeline, SC: SourceClient<P>> EquivocationsReporter<'a, P, SC> {
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use relay_utils::HeaderId;
	use std::sync::Mutex;

	#[async_std::test]
	async fn process_pending_reports_works() {
		let polled_reports = Mutex::new(vec![]);
		let finished_reports = Mutex::new(vec![]);

		let mut reporter =
			EquivocationsReporter::<TestEquivocationDetectionPipeline, TestSourceClient> {
				pending_reports: vec![
					Box::pin(async {
						polled_reports.lock().unwrap().push(1);
						finished_reports.lock().unwrap().push(1);
						TrackedTransactionStatus::Finalized(HeaderId(1, 1))
					}),
					Box::pin(async {
						polled_reports.lock().unwrap().push(2);
						finished_reports.lock().unwrap().push(2);
						TrackedTransactionStatus::Finalized(HeaderId(2, 2))
					}),
					Box::pin(async {
						polled_reports.lock().unwrap().push(3);
						std::future::pending::<()>().await;
						finished_reports.lock().unwrap().push(3);
						TrackedTransactionStatus::Finalized(HeaderId(3, 3))
					}),
					Box::pin(async {
						polled_reports.lock().unwrap().push(4);
						finished_reports.lock().unwrap().push(4);
						TrackedTransactionStatus::Finalized(HeaderId(4, 4))
					}),
				],
			};

		reporter.process_pending_reports().await;
		assert_eq!(*polled_reports.lock().unwrap(), vec![1, 2, 3, 4]);
		assert_eq!(*finished_reports.lock().unwrap(), vec![1, 2, 4]);
		assert_eq!(reporter.pending_reports.len(), 1);
	}
}
