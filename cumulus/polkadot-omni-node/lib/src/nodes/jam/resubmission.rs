// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The pluggable work-package resubmission policy.
//!
//! The collation task owns the mechanics (submission, status stream, re-contexting); the policy
//! is a small pure strategy that decides what to do next, so smarter variants (early soft
//! resubmit, congestion awareness) can plug in later and be unit-tested without a network.

use jam_interface::WorkPackageStatus;

/// What the collation task should do with the in-flight work package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyAction {
	/// Keep waiting for further status updates.
	Wait,
	/// The package landed (`Reported` or later); stop tracking it.
	Done,
	/// Build a fresh refine context around the same block and submit again.
	Resubmit,
	/// Give up on this package.
	Abandon,
}

pub(crate) trait ResubmissionPolicy: Send {
	fn on_status(&mut self, status: &WorkPackageStatus) -> PolicyAction;

	/// The status stream closed without a final verdict (e.g. connection loss).
	fn on_stream_closed(&mut self) -> PolicyAction;
}

/// The phase-1 policy: wait until the package is reported; on `Failed` (or a dead status
/// stream) re-context and resubmit the same block, up to `max_resubmits` times.
///
/// Soft resubmission (same anchor, no `Reported` within ~2 blocks) is deliberately not
/// implemented yet; in phases 1–3 nothing ties a block to its anchor, so re-contexting is
/// always safe and rebuilds are never needed.
pub(crate) struct RecontextOnFailure {
	max_resubmits: u32,
	resubmits: u32,
}

impl RecontextOnFailure {
	pub(crate) fn new(max_resubmits: u32) -> Self {
		Self { max_resubmits, resubmits: 0 }
	}

	fn try_resubmit(&mut self) -> PolicyAction {
		if self.resubmits < self.max_resubmits {
			self.resubmits += 1;
			PolicyAction::Resubmit
		} else {
			PolicyAction::Abandon
		}
	}
}

impl ResubmissionPolicy for RecontextOnFailure {
	fn on_status(&mut self, status: &WorkPackageStatus) -> PolicyAction {
		match status {
			WorkPackageStatus::Reportable { .. } => PolicyAction::Wait,
			WorkPackageStatus::Reported { .. } | WorkPackageStatus::Ready { .. } =>
				PolicyAction::Done,
			WorkPackageStatus::Failed(_) => self.try_resubmit(),
		}
	}

	fn on_stream_closed(&mut self) -> PolicyAction {
		self.try_resubmit()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use jam_interface::BlockDesc;
	use jam_types::{CoreIndex, HeaderHash, WorkReportHash};

	fn reportable(remaining_blocks: u16) -> WorkPackageStatus {
		WorkPackageStatus::Reportable { remaining_blocks }
	}

	fn reported() -> WorkPackageStatus {
		WorkPackageStatus::Reported {
			reported_in: BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 1 },
			core: CoreIndex::default(),
			report_hash: WorkReportHash::from([2u8; 32]),
		}
	}

	#[test]
	fn waits_while_reportable_and_finishes_on_reported() {
		let mut policy = RecontextOnFailure::new(2);
		assert_eq!(policy.on_status(&reportable(8)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reportable(3)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reported()), PolicyAction::Done);
	}

	#[test]
	fn failure_resubmits_until_the_budget_is_spent() {
		let mut policy = RecontextOnFailure::new(2);
		let failed = WorkPackageStatus::Failed("anchor expired".into());
		assert_eq!(policy.on_status(&failed), PolicyAction::Resubmit);
		assert_eq!(policy.on_status(&failed), PolicyAction::Resubmit);
		assert_eq!(policy.on_status(&failed), PolicyAction::Abandon);
	}

	#[test]
	fn stream_loss_counts_against_the_same_budget() {
		let mut policy = RecontextOnFailure::new(1);
		assert_eq!(policy.on_stream_closed(), PolicyAction::Resubmit);
		let failed = WorkPackageStatus::Failed("anchor expired".into());
		assert_eq!(policy.on_status(&failed), PolicyAction::Abandon);
	}
}
