// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{BuiltWorkPackage, Error, JamClient};
use jam_std_common::{RpcClient, WorkPackageStatus};

/// Submits work packages to the JAM network and monitors their status.
pub struct WorkPackageSubmitter<'a> {
	client: &'a JamClient,
}

impl<'a> WorkPackageSubmitter<'a> {
	pub fn new(client: &'a JamClient) -> Self {
		Self { client }
	}

	/// Submit a work package and return its hash for tracking.
	pub async fn submit(&self, wp: &BuiltWorkPackage) -> Result<(), Error> {
		tracing::info!(
			target: "cumulus-jam",
			hash = %wp.hash,
			core = wp.core,
			"Submitting work package to JAM network",
		);

		self.client.submit_work_package(wp.core, wp.encoded.clone(), &[]).await?;

		tracing::debug!(
			target: "cumulus-jam",
			hash = %wp.hash,
			"Work package submitted successfully",
		);

		Ok(())
	}

	/// Query the current status of a previously submitted work package.
	pub async fn status(&self, wp: &BuiltWorkPackage) -> Result<WorkPackageStatus, Error> {
		let best = self.client.best_block().await?;
		self.client.work_package_status(best.header_hash, wp.hash, wp.anchor).await
	}

	/// Subscribe to status updates for a work package and wait until it reaches
	/// `Ready` state or fails.
	///
	/// Returns the final `WorkPackageStatus` (either `Ready` or `Failed`).
	pub async fn wait_for_completion(
		&self,
		wp: &BuiltWorkPackage,
	) -> Result<WorkPackageStatus, Error> {
		let mut sub = RpcClient::subscribe_work_package_status(
			self.client.inner(),
			wp.hash,
			wp.anchor,
			false,
		)
		.await
		.map_err(|e| Error::Node(e.into()))?;

		while let Some(update) = sub.next().await {
			let update = update.map_err(|e| Error::Node(e.into()))?;
			let status = update.value;

			tracing::debug!(
				target: "cumulus-jam",
				hash = %wp.hash,
				?status,
				"Work package status update",
			);

			match &status {
				WorkPackageStatus::Ready { .. } => return Ok(status),
				WorkPackageStatus::Failed(reason) => {
					return Err(Error::WorkPackageFailed(reason.to_string()))
				},
				WorkPackageStatus::Reportable { .. } | WorkPackageStatus::Reported { .. } => {
					continue
				},
			}
		}

		Err(Error::Submission("Status subscription ended unexpectedly".into()))
	}
}
