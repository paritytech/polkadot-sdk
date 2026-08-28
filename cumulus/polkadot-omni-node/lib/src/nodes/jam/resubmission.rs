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
//! The collation manager owns the mechanics (submission, status streams, the chain of in-flight
//! packages); the policy is a small pure strategy that decides what to do next, so smarter
//! variants (congestion awareness, a solo-collator tail rebuild) can plug in later and be
//! unit-tested without a network.
//!
//! The policy is stateless: every package's own counters live in the manager's chain entry and
//! are handed in. There is nowhere else they could live — a chain of packages fails as a chain,
//! and the decision for one entry depends on where it sits in that chain.

use jam_interface::{Slot as JamSlot, WorkPackageStatus};

/// How long a package may go without a `Reported` before it is submitted again.
///
/// With several packages in flight at once, a package that never reached its guarantors is not
/// visible any other way: nothing fails, the status stream simply stays quiet, and every
/// descendant waits behind it.
pub(crate) const RESUBMIT_AFTER_SLOTS: JamSlot = 2;

/// What the collation manager should do with an in-flight work package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyAction {
	/// Keep waiting for further status updates.
	Wait,
	/// The package was reported; stop watching the clock on it. It stays in the chain until the
	/// para head shows it accumulated.
	Done,
	/// Submit the identical bundle again — same bytes, same hash, so every child's links stay
	/// valid.
	Resubmit,
	/// The package cannot be reported any more: drop it and every package chained onto it.
	DropTail,
}

pub(crate) trait ResubmissionPolicy: Send {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction;

	/// No status has said `Reported` yet: `waiting_slots` JAM slots have passed since the
	/// package was last submitted, and it has been resubmitted `resubmits` times.
	fn on_silence(&self, waiting_slots: JamSlot, resubmits: u32) -> PolicyAction;
}

/// The phase-5 policy: wait for a report, resubmit the same bundle when one is late, and drop
/// the tail when the package fails or the resubmit budget runs out.
///
/// Re-contexting is deliberately not a policy action any more. It rewrites a package's bytes and
/// therefore its hash, which every chained child has already named in its prerequisite and its
/// import — so it is a decision only the manager can make, and only for a package with no
/// children at all.
pub(crate) struct DropTailOnFailure {
	max_resubmits: u32,
}

impl DropTailOnFailure {
	pub(crate) fn new(max_resubmits: u32) -> Self {
		Self { max_resubmits }
	}
}

impl ResubmissionPolicy for DropTailOnFailure {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction {
		match status {
			WorkPackageStatus::Reportable { .. } => PolicyAction::Wait,
			WorkPackageStatus::Reported { .. } | WorkPackageStatus::Ready { .. } => {
				PolicyAction::Done
			},
			WorkPackageStatus::Failed(_) => PolicyAction::DropTail,
		}
	}

	fn on_silence(&self, waiting_slots: JamSlot, resubmits: u32) -> PolicyAction {
		if waiting_slots < RESUBMIT_AFTER_SLOTS {
			PolicyAction::Wait
		} else if resubmits < self.max_resubmits {
			PolicyAction::Resubmit
		} else {
			PolicyAction::DropTail
		}
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
		let policy = DropTailOnFailure::new(2);
		assert_eq!(policy.on_status(&reportable(8)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reportable(3)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reported()), PolicyAction::Done);
	}

	/// A failed package takes its descendants with it: their prerequisite and their import both
	/// name a package that will never be reported, so there is nothing to wait for.
	#[test]
	fn a_failure_drops_the_tail_rather_than_retrying() {
		let policy = DropTailOnFailure::new(2);
		let failed = WorkPackageStatus::Failed("anchor expired".into());
		assert_eq!(policy.on_status(&failed), PolicyAction::DropTail);
	}

	/// Silence is given a couple of slots' grace — a package normally reports within one — and
	/// only then repeated.
	#[test]
	fn silence_is_tolerated_until_the_resubmit_window_passes() {
		let policy = DropTailOnFailure::new(2);
		assert_eq!(policy.on_silence(0, 0), PolicyAction::Wait);
		assert_eq!(policy.on_silence(RESUBMIT_AFTER_SLOTS - 1, 0), PolicyAction::Wait);
		assert_eq!(policy.on_silence(RESUBMIT_AFTER_SLOTS, 0), PolicyAction::Resubmit);
	}

	/// The budget is spent on resubmissions of the same bundle; once it is gone the package is
	/// treated as lost, because nothing else will ever move it.
	#[test]
	fn a_silent_package_is_dropped_once_the_budget_is_spent() {
		let policy = DropTailOnFailure::new(2);
		assert_eq!(policy.on_silence(4, 0), PolicyAction::Resubmit);
		assert_eq!(policy.on_silence(4, 1), PolicyAction::Resubmit);
		assert_eq!(policy.on_silence(4, 2), PolicyAction::DropTail);
	}
}
