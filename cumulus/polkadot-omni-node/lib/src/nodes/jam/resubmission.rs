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
//!
//! The same policy drives *foreign* packages this collator verified through the sync path: an
//! unreported one is resent byte-identically, and one whose anchor expired is forgotten, which is
//! what makes another collator's lost block recoverable by a non-author.

use jam_interface::{Slot as JamSlot, WorkPackageStatus};

/// How long a package has to be reported, counted from its anchor: the anchor must still be in
/// JAM's recent history when the package is reported. Past this the package is forgotten; the
/// builder's stall re-root is the recovery.
pub(crate) const REPORT_DEADLINE_SLOTS: JamSlot = 8;

/// How long a package may go without appearing on chain before it is submitted again.
///
/// A package that never reached its guarantors is not visible any other way: β (the recent-blocks
/// history) simply does not name it. Since phase 5a this resubmission is also the only thing that
/// heals a lost block — the parachain service buffers the descendants until the missing package
/// lands, and a package this collator gives up on stalls them all.
pub(crate) const RESUBMIT_AFTER_SLOTS: JamSlot = 2;

/// What the collation manager should do with an in-flight work package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyAction {
	/// Keep waiting for further status updates.
	Wait,
	/// The package was reported; stop watching the clock on it. It stays tracked until the para
	/// head shows its height settled.
	Reported,
	/// Submit the identical package again — same bytes, same hash, so JAM sees one package
	/// repeated rather than a second one.
	Resend,
	/// Give up on the package. Nothing else has to be undone — no other package depends on it.
	Forget,
}

/// What the manager saw about one package when a JAM block arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Observed {
	/// The JAM slot of the block that just arrived.
	pub tip_slot: JamSlot,
	/// The JAM slot the package was last sent against.
	pub submitted_at: JamSlot,
	/// The JAM slot of the package's anchor, the start of its report window.
	pub anchor_slot: JamSlot,
	/// Whether β already names the package's hash.
	pub on_chain: bool,
}

pub(crate) trait ResubmissionPolicy: Send {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction;

	/// A new JAM block arrived: the package either appears in β or the clocks decide its fate.
	fn on_jam_block(&self, observed: Observed) -> PolicyAction;
}

/// The phase-5a policy: wait for a report, resend the identical package when β does not name it
/// after a couple of JAM slots, and give up once its anchor expires.
///
/// There is no resubmit budget any more: the anchor deadline bounds the repetition, so a package
/// is repeated for as long as resending can still help and forgotten exactly when the status
/// tracker would also fail it (`anchor + REPORT_DEADLINE_SLOTS` blocks).
pub(crate) struct ResendUntilAnchorExpires;

impl ResubmissionPolicy for ResendUntilAnchorExpires {
	fn on_status(&self, status: &WorkPackageStatus) -> PolicyAction {
		match status {
			WorkPackageStatus::Reportable { .. } => PolicyAction::Wait,
			WorkPackageStatus::Reported { .. } | WorkPackageStatus::Ready { .. } => {
				PolicyAction::Reported
			},
			WorkPackageStatus::Failed(_) => PolicyAction::Forget,
		}
	}

	fn on_jam_block(&self, observed: Observed) -> PolicyAction {
		if observed.on_chain {
			return PolicyAction::Reported;
		}
		// Expiry before resend: once the anchor is out of β the package can never be reported, so
		// a resend would be wasted.
		if observed.tip_slot.saturating_sub(observed.anchor_slot) >= REPORT_DEADLINE_SLOTS {
			PolicyAction::Forget
		} else if observed.tip_slot.saturating_sub(observed.submitted_at) >= RESUBMIT_AFTER_SLOTS {
			PolicyAction::Resend
		} else {
			PolicyAction::Wait
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

	fn observed(
		tip_slot: JamSlot,
		submitted_at: JamSlot,
		anchor_slot: JamSlot,
		on_chain: bool,
	) -> Observed {
		Observed { tip_slot, submitted_at, anchor_slot, on_chain }
	}

	#[test]
	fn waits_while_reportable_and_finishes_on_reported() {
		let policy = ResendUntilAnchorExpires;
		assert_eq!(policy.on_status(&reportable(8)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reportable(3)), PolicyAction::Wait);
		assert_eq!(policy.on_status(&reported()), PolicyAction::Reported);
	}

	/// `Ready` is still in flight — queued for accumulation, not accumulated — so it is treated
	/// exactly like `Reported`: the resend clock is stopped and the para head decides completion.
	#[test]
	fn a_ready_package_is_treated_as_reported() {
		let policy = ResendUntilAnchorExpires;
		let ready = WorkPackageStatus::Ready {
			reported_in: BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 1 },
			core: CoreIndex::default(),
			report_hash: WorkReportHash::from([2u8; 32]),
			ready_in: BlockDesc { header_hash: HeaderHash::from([3u8; 32]), slot: 2 },
		};
		assert_eq!(policy.on_status(&ready), PolicyAction::Reported);
	}

	/// A failure — "Not reported in time" or a spent anchor — is terminal now: re-anchoring is
	/// gone, so the package is forgotten and the builder's stall re-root recovers.
	#[test]
	fn a_failure_is_forgotten() {
		let policy = ResendUntilAnchorExpires;
		let failed = WorkPackageStatus::Failed("anchor expired".into());
		assert_eq!(policy.on_status(&failed), PolicyAction::Forget);
	}

	/// β not naming the package is given a couple of slots' grace — one read per JAM block is
	/// enough to cover every package, and a normal report lands within one — and only then is it
	/// repeated.
	#[test]
	fn silence_is_tolerated_until_the_resubmit_window_passes() {
		let policy = ResendUntilAnchorExpires;
		assert_eq!(policy.on_jam_block(observed(100, 100, 100, false)), PolicyAction::Wait);
		assert_eq!(
			policy.on_jam_block(observed(100, 100 - (RESUBMIT_AFTER_SLOTS - 1), 100, false)),
			PolicyAction::Wait,
		);
		assert_eq!(
			policy.on_jam_block(observed(100, 100 - RESUBMIT_AFTER_SLOTS, 100, false)),
			PolicyAction::Resend,
		);
	}

	/// The anchor's expiry wins over the resend clock: at `anchor + REPORT_DEADLINE_SLOTS` the
	/// package can never be reported, so it is forgotten even though it is also due for a resend.
	/// One slot earlier it is still repeated.
	#[test]
	fn the_anchor_expiry_wins_over_the_resend_clock() {
		let policy = ResendUntilAnchorExpires;
		// `submitted_at = 0` so the resend clock has long run out; only the anchor age decides.
		let at = |tip: JamSlot| observed(tip, 0, 0, false);

		assert_eq!(policy.on_jam_block(at(REPORT_DEADLINE_SLOTS - 1)), PolicyAction::Resend);
		assert_eq!(policy.on_jam_block(at(REPORT_DEADLINE_SLOTS)), PolicyAction::Forget);
	}

	/// Appearing in β is the truth, whatever the clocks say: a reported package is reported even
	/// when the resend window has passed and the anchor has expired.
	#[test]
	fn appearing_on_chain_wins_over_every_clock() {
		let policy = ResendUntilAnchorExpires;
		let expired_and_due = observed(100, 0, 0, true);
		assert_eq!(policy.on_jam_block(expired_and_due), PolicyAction::Reported);
	}
}
