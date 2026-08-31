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
//! The collation manager owns the mechanics (submission, status streams, the packages in
//! flight); the policy is a small pure strategy that decides what to do next, so smarter
//! variants (congestion awareness, resubmitting somebody else's missing package) can plug in
//! later and be unit-tested without a network.
//!
//! The policy is stateless: every package's own counters live in the manager's entry for it and
//! are handed in. Since phase 5a they are also all it needs — packages no longer depend on each
//! other, so the decision for one says nothing about any other.

use jam_interface::{Slot as JamSlot, WorkPackageStatus};

/// How long a package may go without a `Reported` before it is submitted again.
///
/// A package that never reached its guarantors is not visible any other way: nothing fails, the
/// status stream simply stays quiet. Since phase 5a this soft resubmission is also the only thing
/// that heals a lost block — the parachain service buffers the descendants until the missing
/// package lands, and a package this collator gives up on stalls them all.
pub(crate) const RESUBMIT_AFTER_SLOTS: JamSlot = 2;

/// What the collation manager should do with an in-flight work package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyAction {
	/// Keep waiting for further status updates.
	Wait,
	/// The package was reported; stop watching the clock on it. It stays tracked until the para
	/// head shows its height settled.
	Done,
	/// Submit the identical package again — same bytes, same hash, so JAM sees one package
	/// repeated rather than a second one.
	Resubmit,
	/// The package cannot be reported against this anchor any more: rebuild it around a fresh
	/// one. Legal for any package since phase 5a, because nothing names a package's hash.
	Reanchor,
	/// Give up on the package. Nothing else has to be undone — no other package depends on it.
	Forget,
}

pub(crate) trait ResubmissionPolicy: Send {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction;

	/// No status has said `Reported` yet: `waiting_slots` JAM slots have passed since the
	/// package was last submitted, and it has been resubmitted `resubmits` times.
	fn on_silence(&self, waiting_slots: JamSlot, resubmits: u32) -> PolicyAction;
}

/// The phase-5a policy: wait for a report, resubmit the identical package when one is late,
/// re-anchor a package that failed, and give up once the resubmit budget is spent.
///
/// Re-anchoring is a policy action again. It rewrites a package's bytes and therefore its hash,
/// which under phase 5's links would have orphaned every child that had named the old hash;
/// nothing names it any more, so the cheapest answer to a failure — usually an expired anchor —
/// is available for every package.
pub(crate) struct ReanchorThenForget {
	max_resubmits: u32,
}

impl ReanchorThenForget {
	pub(crate) fn new(max_resubmits: u32) -> Self {
		Self { max_resubmits }
	}
}

impl ResubmissionPolicy for ReanchorThenForget {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction {
		match status {
			WorkPackageStatus::Reportable { .. } => PolicyAction::Wait,
			WorkPackageStatus::Reported { .. } | WorkPackageStatus::Ready { .. } => {
				PolicyAction::Done
			},
			WorkPackageStatus::Failed(_) => PolicyAction::Reanchor,
		}
	}

	fn on_silence(&self, waiting_slots: JamSlot, resubmits: u32) -> PolicyAction {
		if waiting_slots < RESUBMIT_AFTER_SLOTS {
			PolicyAction::Wait
		} else if resubmits < self.max_resubmits {
			PolicyAction::Resubmit
		} else {
			PolicyAction::Forget
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
		let policy = ReanchorThenForget::new(2);
		assert_eq!(policy.on_status(&reportable(8)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reportable(3)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reported()), PolicyAction::Done);
	}

	/// A failure is almost always a spent anchor, and phase 5a can answer it: nothing names the
	/// package's hash, so the same block can go out again around a fresh anchor instead of the
	/// block being abandoned.
	#[test]
	fn a_failure_is_answered_by_re_anchoring() {
		let policy = ReanchorThenForget::new(2);
		let failed = WorkPackageStatus::Failed("anchor expired".into());
		assert_eq!(policy.on_status(&failed), PolicyAction::Reanchor);
	}

	/// Silence is given a couple of slots' grace — a package normally reports within one — and
	/// only then repeated.
	#[test]
	fn silence_is_tolerated_until_the_resubmit_window_passes() {
		let policy = ReanchorThenForget::new(2);
		assert_eq!(policy.on_silence(0, 0), PolicyAction::Wait);
		assert_eq!(policy.on_silence(RESUBMIT_AFTER_SLOTS - 1, 0), PolicyAction::Wait);
		assert_eq!(policy.on_silence(RESUBMIT_AFTER_SLOTS, 0), PolicyAction::Resubmit);
	}

	/// The budget is spent on resubmissions of the identical package; once it is gone the package
	/// is treated as lost, because nothing else will ever move it.
	#[test]
	fn a_silent_package_is_forgotten_once_the_budget_is_spent() {
		let policy = ReanchorThenForget::new(2);
		assert_eq!(policy.on_silence(4, 0), PolicyAction::Resubmit);
		assert_eq!(policy.on_silence(4, 1), PolicyAction::Resubmit);
		assert_eq!(policy.on_silence(4, 2), PolicyAction::Forget);
	}
}
